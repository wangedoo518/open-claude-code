//! `wiki_maintainer` — engram-style single-pass maintainer (canonical §4 blade 3).
//!
//! The maintainer's job, per canonical §7 and `schema/CLAUDE.md` §8:
//!
//!   1. Read a raw/ file that just landed (`read_source`).
//!   2. Build a canonical prompt (system + user) asking the LLM to
//!      summarise the raw content into a `WikiPageProposal`.
//!   3. Fire one `chat_completion` call against the process-global
//!      `codex_broker` (via the `BrokerSender` trait for testability).
//!   4. Parse the assistant's response as strict JSON and return a
//!      `WikiPageProposal`.
//!
//! **NOT in this module:** writing the proposal to disk. That's
//! `wiki_store::write_wiki_page`'s job, called by the HTTP handler
//! after the human presses "Approve & Write". Keeping the propose
//! step separate from the write step is what makes canonical §4
//! blade 2 (權限確認) operational: every write goes through a
//! human decision.
//!
//! ## Why a trait instead of `Arc<CodexBroker>` directly?
//!
//! The orphan rule means `wiki_maintainer` cannot `impl` anything
//! for `CodexBroker` (that type lives in `desktop-core`). So we
//! define a thin `BrokerSender` trait here and provide a wrapper
//! struct `BrokerAdapter` inside `desktop-core` that implements it.
//! Tests use `MockBrokerSender` with canned responses so this crate
//! is fully unit-testable without an HTTP client or a running
//! broker pool.

pub mod prompt;

use api::{MessageRequest, MessageResponse, OutputContentBlock};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The maintainer's output. The LLM returns this as JSON per the
/// prompt template in `prompt.rs`. The HTTP handler then calls
/// `wiki_store::write_wiki_page` to materialize it on disk.
///
/// Field layout is deliberately flat — no nested frontmatter map —
/// so the JSON round-trip is obvious to both the LLM and the
/// frontend. A future sprint can add `tags`, `backlinks`, `category`
/// without breaking existing parsed responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiPageProposal {
    /// URL-safe slug. Must be kebab-case ASCII. Used as the wiki
    /// page filename (`{slug}.md`) and as the primary key.
    pub slug: String,
    /// Human-readable display title. May contain CJK, spaces, etc.
    pub title: String,
    /// Short one-line summary (≤ 200 chars per CLAUDE.md §Triggers).
    /// Stored in the frontmatter as `summary`.
    pub summary: String,
    /// Full markdown body. This is what lands in the file after the
    /// frontmatter block. The LLM is asked to keep this under 200
    /// words per canonical §4 blade 3's copyright policy.
    pub body: String,
    /// The raw/ entry id that seeded this proposal. Echoed back so
    /// the HTTP handler can log the provenance.
    pub source_raw_id: u32,
}

/// Errors raised by the maintainer.
#[derive(Debug, thiserror::Error)]
pub enum MaintainerError {
    /// Couldn't find or read the raw entry we're supposed to
    /// maintain. Propagated from `wiki_store::read_raw_entry`.
    #[error("raw entry not available: {0}")]
    RawNotAvailable(String),
    /// Downstream broker error. The broker's own error type is
    /// stringified so this crate doesn't need to depend on
    /// `desktop-core`.
    #[error("broker error: {0}")]
    Broker(String),
    /// The LLM returned content that couldn't be parsed as a
    /// `WikiPageProposal`. Carries the stringified parse error and
    /// up to 512 chars of the offending payload so the human can
    /// see what went wrong without leaking multi-KB LLM noise.
    #[error("malformed LLM response: {reason}")]
    BadJson {
        reason: String,
        /// First 512 chars of the response, for debugging.
        preview: String,
    },
    /// The LLM returned valid JSON but missing required fields or
    /// values outside the accepted shape.
    #[error("invalid proposal shape: {0}")]
    InvalidProposal(String),
    /// wiki_store operation failed during absorb_batch or query_wiki.
    #[error("wiki store error: {0}")]
    Store(String),
    /// absorb_batch was cancelled by the user via CancellationToken.
    #[error("absorb cancelled by user")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, MaintainerError>;

/// The one external dependency this crate has on the LLM provider.
///
/// Why a trait: `wiki_maintainer` cannot depend on `desktop-core`
/// (that would create a cycle because desktop-core is the crate
/// that instantiates `CodexBroker`, and desktop-core will want to
/// CALL the maintainer). By going through a trait, the maintainer
/// stays at a lower layer of the workspace dep graph and tests
/// can inject a mock.
///
/// The implementer (`desktop-core::BrokerAdapter`) wraps
/// `Arc<CodexBroker>` and translates the broker's own error type
/// into `MaintainerError::Broker(String)`.
#[async_trait]
pub trait BrokerSender: Send + Sync {
    async fn chat_completion(
        &self,
        request: MessageRequest,
    ) -> Result<MessageResponse>;
}

/// Produce a `WikiPageProposal` for a single raw entry.
///
/// Flow:
///
///   1. Read the raw entry (body + metadata).
///   2. Build the concept prompt from the canonical template.
///   3. Call `broker.chat_completion(request).await`.
///   4. Extract the first assistant text block.
///   5. Parse it as JSON → `WikiPageProposal`.
///
/// Testability: takes the broker as a generic `&impl BrokerSender`
/// so unit tests can pass a `MockBrokerSender` with canned JSON.
pub async fn propose_for_raw_entry(
    paths: &wiki_store::WikiPaths,
    raw_id: u32,
    broker: &(impl BrokerSender + ?Sized),
) -> Result<WikiPageProposal> {
    // Step 1 — fetch the raw entry and its body text.
    let (entry, body) = wiki_store::read_raw_entry(paths, raw_id).map_err(|e| {
        MaintainerError::RawNotAvailable(format!("raw_id={raw_id}: {e}"))
    })?;

    // Step 2 — build the prompt.
    let request = prompt::build_concept_request(&entry, &body);

    // Step 3 — fire the broker call.
    let response = broker.chat_completion(request).await?;

    // Step 4 — pull the first assistant text block from the response.
    let raw_json = extract_first_text(&response).ok_or_else(|| {
        MaintainerError::InvalidProposal(
            "LLM response contained no text block".to_string(),
        )
    })?;

    // Step 5 — parse as JSON and validate.
    // On parse failure, log the raw LLM response (first 200 chars) so
    // maintainer failures are debuggable. The error still propagates
    // so the caller (HTTP handler) can show the user a meaningful
    // status — we intentionally do NOT auto-reject here, letting the
    // user decide how to handle the bad response.
    match parse_proposal(&raw_json, raw_id) {
        Ok(proposal) => Ok(proposal),
        Err(e) => {
            eprintln!("[maintainer] LLM response parse failed for raw_id={raw_id}: {e}");
            eprintln!(
                "[maintainer] raw response preview: {}",
                raw_json.chars().take(200).collect::<String>()
            );
            Err(e)
        }
    }
}

/// Extract the first `OutputContentBlock::Text` out of a
/// `MessageResponse`. The LLM is prompted to return a single JSON
/// object as plain text; tool calls and thinking blocks are ignored.
fn extract_first_text(response: &MessageResponse) -> Option<String> {
    for block in &response.content {
        if let OutputContentBlock::Text { text } = block {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Parse a raw LLM response into a validated `WikiPageProposal`.
/// Tolerates leading ```json fences around the object because
/// models like to add them even when told not to.
fn parse_proposal(raw: &str, expected_raw_id: u32) -> Result<WikiPageProposal> {
    let payload = strip_code_fences(raw);

    #[derive(Debug, Deserialize)]
    struct Raw {
        slug: String,
        title: String,
        summary: String,
        body: String,
        #[serde(default)]
        source_raw_id: Option<u32>,
    }

    let parsed: Raw = serde_json::from_str(payload).map_err(|e| {
        MaintainerError::BadJson {
            reason: e.to_string(),
            preview: payload.chars().take(512).collect(),
        }
    })?;

    if parsed.slug.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "slug is empty".to_string(),
        ));
    }
    if parsed.title.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "title is empty".to_string(),
        ));
    }
    if parsed.body.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "body is empty".to_string(),
        ));
    }
    // Slug must be kebab-case ASCII to match wiki_store::slugify
    // output. We don't sanitize here — the LLM is told to return a
    // pre-sanitized slug, and invalid shapes are surfaced loudly.
    if !parsed
        .slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(MaintainerError::InvalidProposal(format!(
            "slug contains invalid chars: {}",
            parsed.slug
        )));
    }

    Ok(WikiPageProposal {
        slug: parsed.slug,
        title: parsed.title,
        summary: parsed.summary,
        body: parsed.body,
        // Prefer the echoed source id, fall back to the caller's.
        source_raw_id: parsed.source_raw_id.unwrap_or(expected_raw_id),
    })
}

/// Strip any leading/trailing ``` or ```json fences from an LLM
/// response. Many models wrap their JSON in a code fence even when
/// asked not to. This is a lossless text transform — if the payload
/// has no fences it returns unchanged.
fn strip_code_fences(raw: &str) -> &str {
    let mut payload = raw.trim();
    // Leading ```json or ```
    if let Some(rest) = payload.strip_prefix("```json") {
        payload = rest.trim_start_matches('\n').trim();
    } else if let Some(rest) = payload.strip_prefix("```") {
        payload = rest.trim_start_matches('\n').trim();
    }
    // Trailing ```
    if let Some(stripped) = payload.strip_suffix("```") {
        payload = stripped.trim_end();
    }
    payload
}

/// The directory under `wiki/` where concept pages live. Exposed so
/// HTTP handlers that do their own file lookups can agree with
/// `wiki_store::write_wiki_page`.
pub const CONCEPTS_SUBDIR: &str = "concepts";

/// Resolve the absolute filesystem path for a concept page given
/// its slug. Pure — does not touch the filesystem.
#[must_use]
pub fn concept_page_path(paths: &wiki_store::WikiPaths, slug: &str) -> PathBuf {
    paths.wiki.join(CONCEPTS_SUBDIR).join(format!("{slug}.md"))
}

// ─────────────────────────────────────────────────────────────────────
// v2: absorb_batch types + function  (technical-design.md §4.2.2–4.2.3)
// ─────────────────────────────────────────────────────────────────────

/// Progress event sent per-entry during [`absorb_batch`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsorbProgressEvent {
    pub processed: usize,
    pub total: usize,
    pub current_entry_id: u32,
    pub action: String,
    pub page_slug: Option<String>,
    pub page_title: Option<String>,
    pub error: Option<String>,
}

/// Final result of [`absorb_batch`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsorbResult {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub duration_ms: u64,
    pub cancelled: bool,
}

/// Streaming chunk from [`query_wiki`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryChunkEvent {
    pub delta: String,
    pub source_refs: Vec<String>,
}

/// Final result of [`query_wiki`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub sources: Vec<QuerySource>,
    pub total_tokens: usize,
}

/// A single source page referenced in a query answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySource {
    pub slug: String,
    pub title: String,
    pub relevance_score: f32,
    pub snippet: String,
}

/// Source priority for absorb ordering (lower = higher priority).
/// Per 01-skill-engine.md §5.1: wechat-article > url > wechat-text > file > paste > voice.
fn source_priority(source: &str) -> u8 {
    match source {
        "wechat-article" => 1,
        "url" => 2,
        "wechat-text" => 3,
        "pdf" | "docx" | "pptx" => 4,
        "query" => 5,  // v2: crystallized query results
        "paste" => 6,
        "voice" => 7,
        _ => 8,
    }
}

/// Compute confidence score for a wiki page based on evidence quality.
/// Per 01-skill-engine.md §5.1 step 3g.
pub fn compute_confidence(source_count: usize, newest_source_age_days: i64, has_conflict: bool) -> f32 {
    if has_conflict {
        return 0.3; // contested
    }
    if source_count >= 3 && newest_source_age_days < 30 {
        return 0.9; // high
    }
    if source_count >= 2 && newest_source_age_days < 90 {
        return 0.6; // medium
    }
    0.2 // low
}

/// Determine wiki category for a proposal. MVP: default "concept".
fn determine_category(_proposal: &WikiPageProposal) -> String {
    "concept".to_string()
}

/// Batch-absorb raw entries into wiki pages.
///
/// Follows the algorithm in `01-skill-engine.md §5.1` exactly:
/// 1. Filter already-absorbed entries
/// 2. Sort by source priority + ingested_at
/// 3. Per entry: propose → write → log → progress
/// 4. Checkpoint every 15 entries (rebuild index + backlinks)
/// 5. Final checkpoint on completion
pub async fn absorb_batch(
    paths: &wiki_store::WikiPaths,
    entry_ids: Vec<u32>,
    broker: &(impl BrokerSender + ?Sized),
    progress_tx: tokio::sync::mpsc::Sender<AbsorbProgressEvent>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<AbsorbResult> {
    let start = std::time::Instant::now();
    let total = entry_ids.len();
    let mut result = AbsorbResult {
        created: 0,
        updated: 0,
        skipped: 0,
        failed: 0,
        duration_ms: 0,
        cancelled: false,
    };

    // ── Step 1: Filter already-absorbed entries ──
    let mut pending: Vec<u32> = Vec::new();
    for &id in &entry_ids {
        if wiki_store::is_entry_absorbed(paths, id) {
            result.skipped += 1;
            let _ = progress_tx
                .send(AbsorbProgressEvent {
                    processed: result.skipped,
                    total,
                    current_entry_id: id,
                    action: "skip".to_string(),
                    page_slug: None,
                    page_title: None,
                    error: None,
                })
                .await;
        } else {
            pending.push(id);
        }
    }

    // ── Step 2: Sort by source priority + ingested_at ──
    let mut entries_with_meta: Vec<(u32, wiki_store::RawEntry)> = Vec::new();
    for &id in &pending {
        match wiki_store::read_raw_entry(paths, id) {
            Ok((entry, _body)) => entries_with_meta.push((id, entry)),
            Err(e) => {
                result.failed += 1;
                let _ = progress_tx
                    .send(AbsorbProgressEvent {
                        processed: result.created + result.updated + result.skipped + result.failed,
                        total,
                        current_entry_id: id,
                        action: "skip".to_string(),
                        page_slug: None,
                        page_title: None,
                        error: Some(format!("无法读取 raw entry: {e}")),
                    })
                    .await;
            }
        }
    }
    entries_with_meta.sort_by(|a, b| {
        source_priority(&a.1.source)
            .cmp(&source_priority(&b.1.source))
            .then_with(|| a.1.ingested_at.cmp(&b.1.ingested_at))
    });

    let mut processed_in_batch = 0usize;

    // ── Step 3: Main absorb loop ──
    for (id, _entry_meta) in &entries_with_meta {
        // Cancel check
        if cancel_token.is_cancelled() {
            result.cancelled = true;
            break;
        }

        processed_in_batch += 1;

        // 3a: Read raw entry content
        let (_entry, _body) = match wiki_store::read_raw_entry(paths, *id) {
            Ok(pair) => pair,
            Err(e) => {
                result.failed += 1;
                let _ = progress_tx
                    .send(AbsorbProgressEvent {
                        processed: result.created + result.updated + result.skipped + result.failed,
                        total,
                        current_entry_id: *id,
                        action: "skip".to_string(),
                        page_slug: None,
                        page_title: None,
                        error: Some(format!("读取失败: {e}")),
                    })
                    .await;
                continue;
            }
        };

        // 3b: Read index.md for context (used by future LLM-based merge)
        let _index_content = std::fs::read_to_string(
            paths.wiki.join(wiki_store::WIKI_INDEX_FILENAME),
        )
        .unwrap_or_default();

        // 3c+d: Build prompt and call LLM via propose_for_raw_entry
        let proposal = match propose_for_raw_entry(paths, *id, broker).await {
            Ok(p) => p,
            Err(e) => {
                // LLM failure → skip this entry, continue batch
                result.failed += 1;
                let _ = progress_tx
                    .send(AbsorbProgressEvent {
                        processed: result.created + result.updated + result.skipped + result.failed,
                        total,
                        current_entry_id: *id,
                        action: "skip".to_string(),
                        page_slug: None,
                        page_title: None,
                        error: Some(format!("LLM 调用或解析失败: {e}")),
                    })
                    .await;
                continue;
            }
        };

        // 3f: Determine create vs update
        let page_exists = wiki_store::read_wiki_page(paths, &proposal.slug).is_ok();
        let action;
        let final_body;
        let category = determine_category(&proposal);

        if page_exists {
            // Update: append new content (merge via topic-driven structure)
            let (_existing_summary, existing_body) = wiki_store::read_wiki_page(paths, &proposal.slug)
                .map_err(|e| MaintainerError::Store(e.to_string()))?;
            final_body = format!("{}\n\n---\n\n{}", existing_body, proposal.body);
            action = "update";
        } else {
            final_body = proposal.body.clone();
            action = "create";
        }

        // 3g: Write to disk
        match wiki_store::write_wiki_page_in_category(
            paths,
            &category,
            &proposal.slug,
            &proposal.title,
            &proposal.summary,
            &final_body,
            Some(*id),
        ) {
            Ok(_) => {}
            Err(e) => {
                result.failed += 1;
                let _ = progress_tx
                    .send(AbsorbProgressEvent {
                        processed: result.created + result.updated + result.skipped + result.failed,
                        total,
                        current_entry_id: *id,
                        action: "skip".to_string(),
                        page_slug: Some(proposal.slug.clone()),
                        page_title: Some(proposal.title.clone()),
                        error: Some(format!("磁盘写入失败: {e}")),
                    })
                    .await;
                continue;
            }
        }

        // 3i: Conflict detection (simplified: skip LLM-based detection for MVP)
        // Full LLM-based conflict detection deferred to later sprint.

        // 3j: Record absorb log
        let log_entry = wiki_store::AbsorbLogEntry {
            entry_id: *id,
            timestamp: wiki_store::now_iso8601(),
            action: action.to_string(),
            page_slug: Some(proposal.slug.clone()),
            page_title: Some(proposal.title.clone()),
            page_category: Some(category.clone()),
        };
        let _ = wiki_store::append_absorb_log(paths, log_entry);

        // 3j-extra: Append wiki/log.md
        let verb = if action == "create" {
            "absorb-create"
        } else {
            "absorb-update"
        };
        let _ = wiki_store::append_wiki_log(paths, verb, &proposal.title);

        // 3g-confidence: Compute and update page confidence score.
        {
            let absorb_log = wiki_store::list_absorb_log(paths).unwrap_or_default();
            let source_count = absorb_log
                .iter()
                .filter(|e| e.page_slug.as_deref() == Some(&proposal.slug) && e.action != "skip")
                .count()
                + 1; // +1 for the current write
            // newest_source_age: 0 days since we just wrote it
            let conf = compute_confidence(source_count, 0, false);
            let _ = wiki_store::update_page_confidence(paths, &proposal.slug, conf);
        }

        // 3k: Update counters and send progress
        if action == "create" {
            result.created += 1;
        } else {
            result.updated += 1;
        }

        let _ = progress_tx
            .send(AbsorbProgressEvent {
                processed: result.created + result.updated + result.skipped + result.failed,
                total,
                current_entry_id: *id,
                action: action.to_string(),
                page_slug: Some(proposal.slug.clone()),
                page_title: Some(proposal.title.clone()),
                error: None,
            })
            .await;

        // ── Step 4: Checkpoint every 15 entries ──
        if processed_in_batch % 15 == 0 && processed_in_batch > 0 {
            let _ = wiki_store::rebuild_wiki_index(paths);
            if let Ok(bl_index) = wiki_store::build_backlinks_index(paths) {
                let _ = wiki_store::save_backlinks_index(paths, &bl_index);
            }
        }
    }

    // ── Step 5: Final checkpoint ──
    let _ = wiki_store::rebuild_wiki_index(paths);
    if let Ok(bl_index) = wiki_store::build_backlinks_index(paths) {
        let _ = wiki_store::save_backlinks_index(paths, &bl_index);
    }

    result.duration_ms = start.elapsed().as_millis() as u64;
    Ok(result)
}

// ─────────────────────────────────────────────────────────────────────
// v2: query_wiki  (technical-design.md §4.2.2, 01-skill-engine.md §5.2)
// ─────────────────────────────────────────────────────────────────────

/// Compute keyword-based relevance score between a question and a page.
fn compute_relevance(
    question: &str,
    page: &wiki_store::WikiPageSummary,
    backlinks: &wiki_store::BacklinksIndex,
) -> f32 {
    let mut score: f32 = 0.0;
    let q_lower = question.to_lowercase();
    let title_lower = page.title.to_lowercase();

    // Exact title match
    if q_lower.contains(&title_lower) || title_lower.contains(&q_lower) {
        score += 1.0;
    }

    // Keyword matching
    let keywords: Vec<&str> = q_lower
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|w| w.len() >= 2)
        .collect();
    for kw in &keywords {
        if title_lower.contains(kw) {
            score += 0.3;
        }
        if page.summary.to_lowercase().contains(kw) {
            score += 0.15;
        }
    }

    // Backlink boost
    let inbound = backlinks.get(&page.slug).map(|v| v.len()).unwrap_or(0);
    score += (inbound as f32 * 0.05).min(0.3);

    score.min(1.0)
}

/// Wiki-grounded Q&A: retrieve relevant pages, build RAG prompt,
/// return answer with source citations.
pub async fn query_wiki(
    paths: &wiki_store::WikiPaths,
    question: &str,
    max_sources: usize,
    broker: &(impl BrokerSender + ?Sized),
    response_tx: tokio::sync::mpsc::Sender<QueryChunkEvent>,
) -> Result<QueryResult> {
    // Step 1: Load wiki index
    let all_pages = wiki_store::list_all_wiki_pages(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    if all_pages.is_empty() {
        return Err(MaintainerError::RawNotAvailable(
            "wiki 为空, 无法回答问题".to_string(),
        ));
    }

    // Step 2: Score and rank pages by relevance
    let backlinks = wiki_store::load_backlinks_index(paths).unwrap_or_default();
    let mut scored: Vec<(f32, &wiki_store::WikiPageSummary)> = Vec::new();
    for page in &all_pages {
        let score = compute_relevance(question, page, &backlinks);
        if score > 0.0 {
            scored.push((score, page));
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let top_k: Vec<_> = scored.into_iter().take(max_sources).collect();

    // Step 3: Read top-K page bodies and build context
    let mut context_parts: Vec<String> = Vec::new();
    let mut sources: Vec<QuerySource> = Vec::new();
    for (score, page) in &top_k {
        if let Ok((_summary, body)) = wiki_store::read_wiki_page(paths, &page.slug) {
            let snippet: String = body.chars().take(200).collect();
            sources.push(QuerySource {
                slug: page.slug.clone(),
                title: page.title.clone(),
                relevance_score: *score,
                snippet,
            });
            context_parts.push(format!(
                "## {} (slug: {})\n\n{}",
                page.title, page.slug, body
            ));
        }
    }

    // Step 4: Build RAG prompt
    let wiki_context = context_parts.join("\n\n---\n\n");
    let system = format!(
        "你是 ClawWiki 知识问答助手。基于以下 wiki 页面回答用户问题。\n\
         引用时使用 [页面标题](concepts/slug.md) 格式。\n\
         如果 wiki 中没有相关信息, 明确说明。\n\n\
         --- Wiki 上下文 ---\n\n{wiki_context}"
    );
    let request = api::MessageRequest {
        model: prompt::MAINTAINER_MODEL.to_string(),
        max_tokens: 2000,
        system: Some(system),
        messages: vec![api::InputMessage {
            role: "user".to_string(),
            content: vec![api::InputContentBlock::Text {
                text: question.to_string(),
            }],
        }],
        tools: None,
        tool_choice: None,
        stream: false,
    };

    // Step 5: Call LLM
    let response = broker.chat_completion(request).await?;
    let answer_text = extract_first_text(&response).unwrap_or_default();
    let answer_for_crystal = answer_text.clone();

    // Send answer chunk (MVP: one-shot, not streaming)
    let source_refs: Vec<String> = sources.iter().map(|s| s.slug.clone()).collect();
    let _ = response_tx
        .send(QueryChunkEvent {
            delta: answer_text,
            source_refs,
        })
        .await;

    let total_tokens = (response.usage.input_tokens + response.usage.output_tokens) as usize;

    // Step 6: Crystallization — write substantive answers to raw/ for future absorption.
    // Per 01-skill-engine.md §5.2 step 6 and technical-design.md §2.2.
    if answer_for_crystal.len() > 200 {
        let slug = wiki_store::slugify(question);
        let fm = wiki_store::RawFrontmatter::for_paste("query", None);
        let body = format!("# Query: {}\n\n{}", question, answer_for_crystal);
        let _ = wiki_store::write_raw_entry(paths, "query", &slug, &body, &fm);
    }

    Ok(QueryResult {
        sources,
        total_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::{MessageResponse, OutputContentBlock, Usage};
    use tempfile::tempdir;

    fn sample_response(text: &str) -> MessageResponse {
        MessageResponse {
            id: "msg-test".to_string(),
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![OutputContentBlock::Text {
                text: text.to_string(),
            }],
            model: "test".to_string(),
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            request_id: None,
        }
    }

    #[test]
    fn parse_proposal_happy_path() {
        // Note: `"body"` uses actual `\n` escape sequences (plain
        // string, not raw) so serde_json sees literal newlines,
        // and the whole literal is plain (not raw) because raw
        // strings don't process escapes.
        let json = "{\
            \"slug\":\"llm-wiki\",\
            \"title\":\"LLM Wiki\",\
            \"summary\":\"Karpathy three-layer cognitive asset architecture.\",\
            \"body\":\"# LLM Wiki\\n\\nKarpathy three-layer model.\",\
            \"source_raw_id\":7\
        }";
        let parsed = parse_proposal(json, 1).unwrap();
        assert_eq!(parsed.slug, "llm-wiki");
        assert_eq!(parsed.title, "LLM Wiki");
        assert_eq!(parsed.source_raw_id, 7, "should use echoed source_raw_id");
    }

    #[test]
    fn parse_proposal_falls_back_to_expected_raw_id() {
        let json = r#"{
            "slug": "s",
            "title": "T",
            "summary": "S",
            "body": "B"
        }"#;
        let parsed = parse_proposal(json, 42).unwrap();
        assert_eq!(parsed.source_raw_id, 42);
    }

    #[test]
    fn parse_proposal_strips_json_fence() {
        let json = "```json\n{\"slug\":\"a\",\"title\":\"T\",\"summary\":\"s\",\"body\":\"b\"}\n```";
        let parsed = parse_proposal(json, 1).unwrap();
        assert_eq!(parsed.slug, "a");
    }

    #[test]
    fn parse_proposal_strips_bare_fence() {
        let json = "```\n{\"slug\":\"a\",\"title\":\"T\",\"summary\":\"s\",\"body\":\"b\"}\n```";
        let parsed = parse_proposal(json, 1).unwrap();
        assert_eq!(parsed.slug, "a");
    }

    #[test]
    fn parse_proposal_rejects_missing_slug() {
        let json = r#"{"slug":"","title":"T","summary":"s","body":"b"}"#;
        let err = parse_proposal(json, 1).unwrap_err();
        assert!(matches!(err, MaintainerError::InvalidProposal(_)));
    }

    #[test]
    fn parse_proposal_rejects_invalid_slug_chars() {
        let json = r#"{"slug":"has space","title":"T","summary":"s","body":"b"}"#;
        let err = parse_proposal(json, 1).unwrap_err();
        assert!(matches!(err, MaintainerError::InvalidProposal(_)));
    }

    #[test]
    fn parse_proposal_rejects_bad_json() {
        let err = parse_proposal("{not json", 1).unwrap_err();
        match err {
            MaintainerError::BadJson { reason, preview } => {
                assert!(!reason.is_empty());
                assert!(preview.contains("not json"));
            }
            other => panic!("expected BadJson, got {other:?}"),
        }
    }

    #[test]
    fn extract_first_text_ignores_empty_blocks() {
        let resp = sample_response("   \n\n");
        assert!(extract_first_text(&resp).is_none());
    }

    #[test]
    fn extract_first_text_returns_trimmed() {
        let resp = sample_response("  hello  ");
        assert_eq!(extract_first_text(&resp).as_deref(), Some("hello"));
    }

    #[test]
    fn concept_page_path_honors_wiki_root() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let path = concept_page_path(&paths, "llm-wiki");
        assert!(path.ends_with("wiki/concepts/llm-wiki.md"));
    }

    // ── MockBrokerSender + propose_for_raw_entry integration ──

    struct MockBrokerSender {
        canned: String,
    }

    #[async_trait]
    impl BrokerSender for MockBrokerSender {
        async fn chat_completion(
            &self,
            _request: MessageRequest,
        ) -> Result<MessageResponse> {
            Ok(sample_response(&self.canned))
        }
    }

    fn seed_raw(paths: &wiki_store::WikiPaths, body: &str) -> u32 {
        let fm = wiki_store::RawFrontmatter::for_paste("paste", None);
        wiki_store::write_raw_entry(paths, "paste", "test seed", body, &fm)
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn propose_roundtrips_canned_json_response() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let raw_id = seed_raw(&paths, "Karpathy LLM Wiki is a three-layer architecture.");

        // Plain string (not raw) so `\n` becomes an escape that
        // serde_json sees as a literal newline. `{{` / `}}` escape
        // braces; `{raw_id}` is a format! placeholder.
        let canned = format!(
            "{{\
                \"slug\":\"llm-wiki\",\
                \"title\":\"LLM Wiki\",\
                \"summary\":\"Three layers.\",\
                \"body\":\"# LLM Wiki\\n\\nBody.\",\
                \"source_raw_id\":{raw_id}\
            }}"
        );
        let broker = MockBrokerSender { canned };

        let proposal = propose_for_raw_entry(&paths, raw_id, &broker).await.unwrap();
        assert_eq!(proposal.slug, "llm-wiki");
        assert_eq!(proposal.title, "LLM Wiki");
        assert_eq!(proposal.source_raw_id, raw_id);
        assert!(proposal.body.starts_with("# LLM Wiki"));
    }

    #[tokio::test]
    async fn propose_raises_on_missing_raw_entry() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let broker = MockBrokerSender {
            canned: "unused".to_string(),
        };
        let err = propose_for_raw_entry(&paths, 999, &broker).await.unwrap_err();
        assert!(matches!(err, MaintainerError::RawNotAvailable(_)));
    }

    #[tokio::test]
    async fn propose_raises_on_malformed_llm_response() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let raw_id = seed_raw(&paths, "body");
        let broker = MockBrokerSender {
            canned: "this is not json".to_string(),
        };
        let err = propose_for_raw_entry(&paths, raw_id, &broker).await.unwrap_err();
        assert!(matches!(err, MaintainerError::BadJson { .. }));
    }

    #[tokio::test]
    async fn propose_surfaces_broker_error() {
        struct FailingBroker;
        #[async_trait]
        impl BrokerSender for FailingBroker {
            async fn chat_completion(
                &self,
                _request: MessageRequest,
            ) -> Result<MessageResponse> {
                Err(MaintainerError::Broker("simulated network down".to_string()))
            }
        }
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let raw_id = seed_raw(&paths, "body");
        let err = propose_for_raw_entry(&paths, raw_id, &FailingBroker).await.unwrap_err();
        assert!(matches!(err, MaintainerError::Broker(_)));
    }

    // ── absorb_batch tests ──────────────────────────────────────────

    /// MockBrokerSender that returns different proposals per call,
    /// cycling through a list of canned responses.
    struct SequentialBroker {
        responses: std::sync::Mutex<Vec<String>>,
    }

    impl SequentialBroker {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl BrokerSender for SequentialBroker {
        async fn chat_completion(
            &self,
            _request: MessageRequest,
        ) -> Result<MessageResponse> {
            let mut lock = self.responses.lock().unwrap();
            let text = if lock.is_empty() {
                // Fallback: return a generic proposal
                r#"{"slug":"fallback","title":"Fallback","summary":"s","body":"b","source_raw_id":0}"#.to_string()
            } else {
                lock.remove(0)
            };
            Ok(sample_response(&text))
        }
    }

    fn make_proposal_json(slug: &str, title: &str, raw_id: u32) -> String {
        format!(
            "{{\"slug\":\"{slug}\",\"title\":\"{title}\",\
             \"summary\":\"Summary of {title}.\",\
             \"body\":\"# {title}\\n\\nBody content.\",\
             \"source_raw_id\":{raw_id}}}"
        )
    }

    #[tokio::test]
    async fn absorb_batch_happy_path_creates_pages() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        // Seed 3 raw entries
        let id1 = seed_raw(&paths, "Content about Transformer architecture.");
        let id2 = seed_raw(&paths, "Content about attention mechanism.");
        let id3 = seed_raw(&paths, "More about Transformer for update.");

        // Broker returns proposals: 2 creates + 1 that targets same slug (update)
        let broker = SequentialBroker::new(vec![
            make_proposal_json("transformer", "Transformer", id1),
            make_proposal_json("attention", "Attention Mechanism", id2),
            make_proposal_json("transformer", "Transformer", id3), // update
        ]);

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let cancel = tokio_util::sync::CancellationToken::new();

        let result = absorb_batch(&paths, vec![id1, id2, id3], &broker, tx, cancel)
            .await
            .unwrap();

        assert_eq!(result.created, 2, "should create 2 new pages");
        assert_eq!(result.updated, 1, "should update 1 existing page");
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed, 0);
        assert!(!result.cancelled);

        // Verify pages exist on disk
        assert!(wiki_store::read_wiki_page(&paths, "transformer").is_ok());
        assert!(wiki_store::read_wiki_page(&paths, "attention").is_ok());

        // Verify absorb log was written
        let log = wiki_store::list_absorb_log(&paths).unwrap();
        assert_eq!(log.len(), 3);

        // Verify progress events were sent
        let mut events = Vec::new();
        rx.close();
        while let Some(evt) = rx.recv().await {
            events.push(evt);
        }
        assert_eq!(events.len(), 3, "should have 3 progress events");
    }

    #[tokio::test]
    async fn absorb_batch_skips_already_absorbed() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        let id1 = seed_raw(&paths, "Content.");

        // Pre-populate absorb log so id1 is "already absorbed"
        wiki_store::append_absorb_log(
            &paths,
            wiki_store::AbsorbLogEntry {
                entry_id: id1,
                timestamp: wiki_store::now_iso8601(),
                action: "create".to_string(),
                page_slug: Some("existing".to_string()),
                page_title: Some("Existing".to_string()),
                page_category: Some("concept".to_string()),
            },
        )
        .unwrap();

        let broker = MockBrokerSender {
            canned: "unused".to_string(),
        };

        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        let cancel = tokio_util::sync::CancellationToken::new();

        let result = absorb_batch(&paths, vec![id1], &broker, tx, cancel)
            .await
            .unwrap();

        assert_eq!(result.skipped, 1, "should skip already-absorbed entry");
        assert_eq!(result.created, 0);
        assert_eq!(result.updated, 0);
    }

    #[tokio::test]
    async fn absorb_batch_handles_cancellation() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        let id1 = seed_raw(&paths, "Content 1.");
        let id2 = seed_raw(&paths, "Content 2.");

        // Cancel immediately before processing starts
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();

        let broker = MockBrokerSender {
            canned: make_proposal_json("test", "Test", id1),
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(32);

        let result = absorb_batch(&paths, vec![id1, id2], &broker, tx, cancel)
            .await
            .unwrap();

        assert!(result.cancelled);
    }

    // ── query_wiki tests ──────────────────────────────────────────

    #[tokio::test]
    async fn query_wiki_returns_answer_with_sources() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        // Create a wiki page so there's something to query
        wiki_store::write_wiki_page_in_category(
            &paths,
            "concept",
            "transformer",
            "Transformer Architecture",
            "Self-attention based neural network.",
            "# Transformer\n\nA Transformer uses self-attention to process sequences.",
            Some(1),
        )
        .unwrap();

        let broker = MockBrokerSender {
            canned: "Transformer 是基于自注意力机制的模型。".to_string(),
        };

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let result = query_wiki(&paths, "什么是 Transformer?", 5, &broker, tx)
            .await
            .unwrap();

        assert!(!result.sources.is_empty(), "should have at least one source");
        assert_eq!(result.sources[0].slug, "transformer");

        // Check that a chunk was sent
        let chunk = rx.recv().await.expect("should receive a chunk");
        assert!(chunk.delta.contains("Transformer"));
    }

    #[tokio::test]
    async fn query_wiki_returns_error_on_empty_wiki() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        let broker = MockBrokerSender {
            canned: "unused".to_string(),
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(32);

        let err = query_wiki(&paths, "anything?", 5, &broker, tx)
            .await
            .unwrap_err();
        assert!(matches!(err, MaintainerError::RawNotAvailable(_)));
    }

    // ── Additional Phase 2 tests ────────────────────────────────

    #[test]
    fn source_priority_ordering() {
        assert!(source_priority("wechat-article") < source_priority("url"));
        assert!(source_priority("url") < source_priority("wechat-text"));
        assert!(source_priority("wechat-text") < source_priority("pdf"));
        assert!(source_priority("pdf") < source_priority("paste"));
        assert!(source_priority("paste") < source_priority("voice"));
        assert!(source_priority("voice") < source_priority("unknown_type"));
    }

    #[test]
    fn determine_category_defaults_to_concept() {
        let proposal = WikiPageProposal {
            slug: "test".to_string(),
            title: "Test".to_string(),
            summary: "S".to_string(),
            body: "B".to_string(),
            source_raw_id: 1,
        };
        assert_eq!(determine_category(&proposal), "concept");
    }

    #[test]
    fn compute_relevance_exact_title_match() {
        let page = wiki_store::WikiPageSummary {
            slug: "transformer".to_string(),
            title: "Transformer".to_string(),
            summary: "A neural network architecture".to_string(),
            source_raw_id: None,
            created_at: "2026-04-14T00:00:00Z".to_string(),
            byte_size: 500,
            category: "concept".to_string(),
            confidence: 0.0,
        };
        let backlinks = wiki_store::BacklinksIndex::new();
        let score = compute_relevance("transformer", &page, &backlinks);
        // Should get +1.0 for exact match + keyword bonus.
        assert!(score >= 1.0, "exact match score should be >= 1.0, got {score}");
    }

    #[test]
    fn compute_relevance_no_match() {
        let page = wiki_store::WikiPageSummary {
            slug: "transformer".to_string(),
            title: "Transformer".to_string(),
            summary: "A neural network".to_string(),
            source_raw_id: None,
            created_at: "2026-04-14T00:00:00Z".to_string(),
            byte_size: 500,
            category: "concept".to_string(),
            confidence: 0.0,
        };
        let backlinks = wiki_store::BacklinksIndex::new();
        let score = compute_relevance("完全不相关的问题", &page, &backlinks);
        assert!(score < 0.5, "unrelated query should score low, got {score}");
    }

    #[test]
    fn compute_relevance_backlink_boost() {
        // Use a query that partially matches (keyword only, not exact title match)
        // so the score stays below 1.0 before the backlink boost.
        let page = wiki_store::WikiPageSummary {
            slug: "deep-learning-fundamentals".to_string(),
            title: "Deep Learning Fundamentals".to_string(),
            summary: "Core concepts of neural networks".to_string(),
            source_raw_id: None,
            created_at: "2026-04-14T00:00:00Z".to_string(),
            byte_size: 500,
            category: "concept".to_string(),
            confidence: 0.0,
        };
        let mut backlinks = wiki_store::BacklinksIndex::new();
        backlinks.insert(
            "deep-learning-fundamentals".to_string(),
            vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        );
        // "neural" matches summary keyword (+0.15) but not title exactly.
        let score_with = compute_relevance("neural", &page, &backlinks);

        let empty_backlinks = wiki_store::BacklinksIndex::new();
        let score_without = compute_relevance("neural", &page, &empty_backlinks);

        assert!(
            score_with > score_without,
            "backlink boost should increase score: {score_with} > {score_without}"
        );
    }

    #[tokio::test]
    async fn absorb_batch_llm_failure_skips_entry() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        let id1 = seed_raw(&paths, "Content that will fail LLM.");

        // Broker returns invalid JSON → parse failure → skip.
        let broker = MockBrokerSender {
            canned: "this is not valid json at all".to_string(),
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        let cancel = tokio_util::sync::CancellationToken::new();

        let result = absorb_batch(&paths, vec![id1], &broker, tx, cancel)
            .await
            .unwrap();

        assert_eq!(result.failed, 1, "LLM failure should increment failed count");
        assert_eq!(result.created, 0);
    }

    #[tokio::test]
    async fn query_wiki_multiple_sources_sorted_by_relevance() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        // Create two pages: one highly relevant, one less so.
        wiki_store::write_wiki_page_in_category(
            &paths,
            "concept",
            "transformer",
            "Transformer Architecture",
            "Self-attention based model.",
            "# Transformer\n\nThe Transformer uses self-attention.",
            Some(1),
        )
        .unwrap();
        wiki_store::write_wiki_page_in_category(
            &paths,
            "concept",
            "rnn",
            "Recurrent Neural Network",
            "Sequential processing model.",
            "# RNN\n\nRNNs process sequences step by step.",
            Some(2),
        )
        .unwrap();

        let broker = MockBrokerSender {
            canned: "Transformer 和 RNN 都是序列处理模型。".to_string(),
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);

        let result = query_wiki(&paths, "Transformer architecture", 5, &broker, tx)
            .await
            .unwrap();

        assert!(result.sources.len() >= 1);
        // First source should be the more relevant one.
        assert_eq!(result.sources[0].slug, "transformer");

        // Verify chunk was sent.
        let chunk = rx.recv().await.expect("should receive chunk");
        assert!(!chunk.delta.is_empty());
    }

    // ── Cognitive Compound Interest tests ────────────────────────

    #[test]
    fn compute_confidence_high() {
        assert_eq!(compute_confidence(3, 10, false), 0.9);
    }

    #[test]
    fn compute_confidence_low_single_old_source() {
        assert_eq!(compute_confidence(1, 100, false), 0.2);
    }

    #[test]
    fn compute_confidence_contested() {
        assert_eq!(compute_confidence(5, 5, true), 0.3);
    }

    #[test]
    fn compute_confidence_medium() {
        assert_eq!(compute_confidence(2, 60, false), 0.6);
    }

    #[test]
    fn source_priority_query_is_5() {
        assert_eq!(source_priority("query"), 5);
    }

    #[test]
    fn source_priority_ordering_with_query() {
        assert!(source_priority("pdf") < source_priority("query"));
        assert!(source_priority("query") < source_priority("paste"));
    }

    #[tokio::test]
    async fn crystallization_writes_raw_entry() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        // Create a wiki page so query has something to find.
        wiki_store::write_wiki_page_in_category(
            &paths, "concept", "test-topic", "Test Topic", "Summary",
            "# Test\n\nThis is a long test page with enough content to be found by the query.",
            Some(1),
        ).unwrap();

        // Use a broker that returns a long answer (> 200 chars).
        let long_answer = "x".repeat(250);
        let broker = MockBrokerSender {
            canned: long_answer.clone(),
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        let _ = query_wiki(&paths, "test topic question", 5, &broker, tx).await;

        // Verify a raw entry with source="query" was created.
        let raws = wiki_store::list_raw_entries(&paths).unwrap();
        let query_raws: Vec<_> = raws.iter().filter(|r| r.source == "query").collect();
        assert!(
            !query_raws.is_empty(),
            "crystallization should create a raw entry with source='query'"
        );
    }

    #[tokio::test]
    async fn no_crystallization_for_short_answer() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        wiki_store::write_wiki_page_in_category(
            &paths, "concept", "short-test", "Short", "S",
            "# Short page.", Some(1),
        ).unwrap();

        // Broker returns a short answer (< 200 chars).
        let broker = MockBrokerSender {
            canned: "Short answer.".to_string(),
        };
        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        let _ = query_wiki(&paths, "short question", 5, &broker, tx).await;

        let raws = wiki_store::list_raw_entries(&paths).unwrap();
        let query_raws: Vec<_> = raws.iter().filter(|r| r.source == "query").collect();
        assert!(query_raws.is_empty(), "short answer should NOT crystallize");
    }
}

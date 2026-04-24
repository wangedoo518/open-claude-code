//! Prompt templates for the engram-style maintainer.
//!
//! Canonical §7.3 row 6/7 pins the maintainer to "engram" shape:
//! one `chat_completion` call per raw entry that returns a strict
//! JSON `WikiPageProposal`. This module owns the exact text that
//! gets sent to the LLM.
//!
//! ## Why a dedicated module
//!
//! * Keeps the prompt reviewable in isolation (the LLM output
//!   quality is 80% determined by this file).
//! * Lets tests pin the prompt shape so regressions in the
//!   template are caught at `cargo test` time, not at runtime.
//! * Future sprints can add more prompt builders
//!   (`build_conflict_prompt`, `build_stale_verify_prompt`, ...)
//!   without touching the rest of the crate.

use api::{InputContentBlock, InputMessage, MessageRequest};
use wiki_store::RawEntry;

/// Model name the maintainer asks for. Canonical §7.3 row 6 says
/// "Codex GPT-5.4". The broker ignores this when it picks a token
/// out of the pool and instead uses the endpoint's default model,
/// but the request still has to carry SOMETHING in this field
/// because `MessageRequest.model` is non-optional upstream.
pub const MAINTAINER_MODEL: &str = "gpt-5.4";

/// Conservative output cap for the proposal response. Canonical
/// CLAUDE.md §Triggers says "≤ 200 words"; ~800 tokens gives the
/// LLM headroom for JSON framing + title + body without running
/// away. Set low to cap cost per ingest.
pub const MAX_OUTPUT_TOKENS: u32 = 800;

/// System prompt pinned to the canonical CLAUDE.md §Triggers rules:
///   - summarise ≤ 200 words, quote ≤ 15 words
///   - return STRICT JSON, nothing else
///   - use canonical schema v1 frontmatter
///   - title + slug + summary + body
///
/// The system prompt is static and verbatim-reviewable via
/// `SYSTEM_PROMPT`. Tests assert the critical invariants (word
/// cap, quote cap, JSON-only) so the template can't drift silently.
pub const SYSTEM_PROMPT: &str = r#"You are the wiki-maintainer agent for ClawWiki — the user's "外脑" (external brain).

Your single job on this turn: read the user-supplied raw entry and produce a concept wiki page proposal.

HARD RULES (canonical `schema/CLAUDE.md` §Triggers and §"Never do"):

1. Respond with STRICT JSON ONLY. No prose, no markdown fences, no code blocks.
2. The JSON object MUST have exactly these fields:
   - slug      (string, kebab-case ASCII, e.g. "llm-wiki")
   - title     (string, human-readable display title; may contain CJK)
   - summary   (string, one sentence, ≤ 200 characters)
   - body      (string, markdown, ≤ 200 words)
   - source_raw_id (integer, copy from the raw entry id)
   - conflict_with (array of strings, optional; existing wiki slugs contradicted by this raw)
   - conflict_reason (string, optional; short reason when conflict_with is non-empty)
3. Quote ≤ 15 consecutive words from the raw source (hard copyright cap).
4. If the raw source contradicts an existing page slug named in the provided context,
   set conflict_with/conflict_reason. Do not silently overwrite contested facts.
5. NEVER emit backlinks to non-existent pages.
6. If you cannot produce a confident summary, respond with an object
   that sets `summary` to "uncertain: {reason}" and a minimal body.
   DO NOT refuse and DO NOT return a non-JSON apology — an uncertain
   proposal is better than a parse error.
"#;

/// Build the concept-page request. The assistant will see a
/// single user message containing the raw entry metadata + body,
/// and is asked to return the JSON proposal.
///
/// Pinned invariants:
///   - `system` is set to [`SYSTEM_PROMPT`]
///   - `stream` is `false` (MVP uses non-streaming)
///   - `max_tokens` is [`MAX_OUTPUT_TOKENS`]
///   - The user message includes `source_raw_id: {id}` so the LLM
///     can echo it back into the response JSON
pub fn build_concept_request(entry: &RawEntry, body: &str) -> MessageRequest {
    let user_text = format!(
        "Raw entry:\n\
         - id: {id}\n\
         - filename: {filename}\n\
         - source: {source}\n\
         - ingested_at: {ingested_at}\n\
         \n\
         Body:\n\
         {body}\n\
         \n\
         Produce the concept wiki page JSON proposal now. \
         Remember: JSON only, ≤ 200 words in body, ≤ 15 words quoted \
         from the raw source, source_raw_id must equal {id}.",
        id = entry.id,
        filename = entry.filename,
        source = entry.source,
        ingested_at = entry.ingested_at,
        body = body,
    );

    MessageRequest {
        model: MAINTAINER_MODEL.to_string(),
        max_tokens: MAX_OUTPUT_TOKENS,
        system: Some(SYSTEM_PROMPT.to_string()),
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text { text: user_text }],
        }],
        tools: None,
        tool_choice: None,
        stream: false,
    }
}

// ── W2: merge (update-existing) prompt ────────────────────────────
//
// W2 flips `update_existing` from a deterministic "append under a
// dated heading" step into a two-phase LLM proposal. The prompt
// below is the merge ask: given an existing wiki page and a new
// raw entry, return a JSON object with the merged page markdown
// plus a one-line summary of what changed. The summary is what
// the frontend shows in the diff preview header, so it must stay
// short and factual.

/// System prompt for the merge step. Kept separate from the
/// concept system prompt because the expectations differ:
///   - this prompt expects TWO inputs (existing page + raw body)
///   - the output is the FULL merged page, not a fresh proposal
///   - preservation (don't drop existing info) matters more than
///     conciseness (the 200-word cap of concept generation does
///     not apply — the merged page can grow as knowledge grows)
pub const MERGE_SYSTEM_PROMPT: &str = r#"你是 ClawWiki 的 Wiki 维护者。
现在要把一则新素材合并到一页已经存在的概念页面上。

Hard rules:
1. 输出 STRICT JSON only (no prose, no code fences). Exactly two fields:
   - after_markdown  (string): the complete merged Markdown body (no YAML frontmatter).
   - summary         (string): one short sentence in Chinese describing what you changed.
2. 保持原页面的章节结构，只在确实有新信息时补充或修改；不要平白无故重写。
3. 不要丢失原有信息。若素材和原内容冲突，追加「## 待确认」小节并并列展示两种说法。
4. 若素材完全不相关或没有可合并的新信息，把 after_markdown 原样返回原内容，summary 写 "未合并：原因"。
5. 引用素材时单段引用不超过 15 个连续词。
"#;

/// Build the merge request for [`propose_update`]. The user message
/// contains both the existing page body and the raw entry body, with
/// a short header telling the LLM which is which. The assistant is
/// asked to return `{after_markdown, summary}`.
///
/// Output cap: 4000 tokens. Higher than concept generation because a
/// merged page can legitimately be larger than a fresh proposal
/// (existing content + new insertions). Still bounded so a runaway
/// response can't blow up the broker budget.
pub const MERGE_MAX_OUTPUT_TOKENS: u32 = 4000;

/// Build the merge chat request.
///
/// `target_slug` and `target_title` are plumbed in so the LLM knows
/// which page it's editing; they show up in the user message as
/// context but are not part of the expected output.
pub fn build_merge_request(
    target_slug: &str,
    target_title: &str,
    existing_body: &str,
    raw_body: &str,
) -> MessageRequest {
    let user_text = format!(
        "目标页面:\n\
         - slug: {target_slug}\n\
         - title: {target_title}\n\
         \n\
         ── 现有 Markdown 正文 ──\n\
         {existing_body}\n\
         ── 新素材（raw body）──\n\
         {raw_body}\n\
         \n\
         请把新素材合并进现有正文，返回 {{\"after_markdown\": \"...\", \"summary\": \"...\"}}。"
    );

    MessageRequest {
        model: MAINTAINER_MODEL.to_string(),
        max_tokens: MERGE_MAX_OUTPUT_TOKENS,
        system: Some(MERGE_SYSTEM_PROMPT.to_string()),
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text { text: user_text }],
        }],
        tools: None,
        tool_choice: None,
        stream: false,
    }
}

// ── W3: combined (multi-source) merge prompt ──────────────────────
//
// W3 adds a "combined" merge path that folds 2..=6 inbox raw bodies
// into a single wiki page in one LLM call. The system prompt reuses
// the same hard rules as the single-source merge (`MERGE_SYSTEM_PROMPT`)
// and appends one extra clause telling the LLM it may see multiple
// sources at once and should deduplicate across them.
//
// Output cap: 8000 tokens. Higher than the single-source merge
// (4000) because a page absorbing multiple raw bodies can legitimately
// grow larger. Still bounded so a runaway response can't blow up the
// broker budget.

/// System prompt addendum appended to [`MERGE_SYSTEM_PROMPT`] when
/// the combined merge path runs. Split into its own constant so tests
/// can assert the combined prompt strictly extends the single-source
/// one without rewriting any hard rules.
pub const COMBINED_SYSTEM_PROMPT_SUFFIX: &str =
    "\n可处理一或多则素材。若有多条素材，请权衡融合、去重，避免内容冗余。\n";

/// Output cap for the combined merge step. Higher than the
/// single-source merge's 4000 because a multi-source merge can
/// legitimately grow the page further.
pub const COMBINED_MERGE_MAX_OUTPUT_TOKENS: u32 = 8000;

/// Build the combined merge chat request.
///
/// `sources` is an already-validated slice of `(inbox_id, raw_title,
/// raw_body)` tuples — the caller (`propose_combined_update`) is
/// responsible for asserting `2 <= sources.len() <= 6` before calling.
/// Each source is rendered as a numbered block in the user prompt so
/// the LLM can refer back to specific material if needed.
///
/// Expected LLM response shape (unchanged from single-source merge):
/// `{"after_markdown": "...", "summary": "..."}`.
pub fn build_combined_merge_request(
    target_slug: &str,
    target_title: &str,
    before_markdown: &str,
    sources: &[(u32, String, String)],
) -> MessageRequest {
    let n = sources.len();
    let mut user_text = String::new();
    user_text.push_str(&format!(
        "目标页面:\n\
         - slug: {target_slug}\n\
         - title: {target_title}\n\
         \n\
         ── 现有 Markdown 正文 ──\n\
         {before_markdown}\n\
         \n\
         ── 以下是 {n} 条新素材需要合并到此页 ──\n\
         \n",
    ));
    for (i, (inbox_id, raw_title, raw_body)) in sources.iter().enumerate() {
        let idx = i + 1;
        user_text.push_str(&format!(
            "=== 素材 {idx}: {raw_title} (inbox #{inbox_id}) ===\n\
             {raw_body}\n\
             \n",
        ));
    }
    user_text.push_str(&format!(
        "请把所有 {n} 条新素材统一合并进现有正文，保持一致风格，\n\
         返回 {{\"after_markdown\": \"...\", \"summary\": \"...\"}}。"
    ));

    let system = format!("{MERGE_SYSTEM_PROMPT}{COMBINED_SYSTEM_PROMPT_SUFFIX}");

    MessageRequest {
        model: MAINTAINER_MODEL.to_string(),
        max_tokens: COMBINED_MERGE_MAX_OUTPUT_TOKENS,
        system: Some(system),
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text { text: user_text }],
        }],
        tools: None,
        tool_choice: None,
        stream: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiki_store::RawEntry;

    fn sample_entry() -> RawEntry {
        RawEntry {
            id: 42,
            filename: "00042_paste_hello-world_2026-04-09.md".to_string(),
            source: "paste".to_string(),
            slug: "hello-world".to_string(),
            date: "2026-04-09".to_string(),
            source_url: None,
            ingested_at: "2026-04-09T14:22:00Z".to_string(),
            byte_size: 1234,
            content_hash: None,
            original_url: None,
        }
    }

    #[test]
    fn system_prompt_includes_canonical_word_cap() {
        assert!(SYSTEM_PROMPT.contains("≤ 200 words"));
        assert!(SYSTEM_PROMPT.contains("≤ 15"));
    }

    #[test]
    fn system_prompt_enforces_json_only() {
        assert!(SYSTEM_PROMPT.contains("STRICT JSON ONLY"));
        assert!(SYSTEM_PROMPT.contains("slug"));
        assert!(SYSTEM_PROMPT.contains("title"));
        assert!(SYSTEM_PROMPT.contains("summary"));
        assert!(SYSTEM_PROMPT.contains("body"));
        assert!(SYSTEM_PROMPT.contains("source_raw_id"));
        assert!(SYSTEM_PROMPT.contains("conflict_with"));
        assert!(SYSTEM_PROMPT.contains("conflict_reason"));
    }

    #[test]
    fn build_concept_request_shape() {
        let entry = sample_entry();
        let req = build_concept_request(&entry, "Some raw content about LLM Wiki.");
        assert_eq!(req.model, MAINTAINER_MODEL);
        assert_eq!(req.max_tokens, MAX_OUTPUT_TOKENS);
        assert!(!req.stream);
        assert_eq!(req.system.as_deref(), Some(SYSTEM_PROMPT));
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
    }

    #[test]
    fn build_concept_request_includes_raw_metadata() {
        let entry = sample_entry();
        let req = build_concept_request(&entry, "Body content here.");
        let first = match &req.messages[0].content[0] {
            InputContentBlock::Text { text } => text.clone(),
            _ => panic!("expected text block"),
        };
        assert!(first.contains("id: 42"));
        assert!(first.contains("00042_paste_hello-world_2026-04-09.md"));
        assert!(first.contains("Body content here."));
        assert!(first.contains("source_raw_id must equal 42"));
    }

    #[test]
    fn merge_system_prompt_pins_json_shape_and_preserve_rule() {
        assert!(MERGE_SYSTEM_PROMPT.contains("after_markdown"));
        assert!(MERGE_SYSTEM_PROMPT.contains("summary"));
        assert!(MERGE_SYSTEM_PROMPT.contains("STRICT JSON"));
        assert!(MERGE_SYSTEM_PROMPT.contains("不要丢失原有信息"));
    }

    #[test]
    fn build_merge_request_shape_and_body() {
        let req = build_merge_request(
            "attention",
            "注意力机制",
            "# Attention\n\nOriginal body.",
            "New insights about multi-head attention.",
        );
        assert_eq!(req.model, MAINTAINER_MODEL);
        assert_eq!(req.max_tokens, MERGE_MAX_OUTPUT_TOKENS);
        assert!(!req.stream);
        assert_eq!(req.system.as_deref(), Some(MERGE_SYSTEM_PROMPT));
        assert_eq!(req.messages.len(), 1);
        let text = match &req.messages[0].content[0] {
            InputContentBlock::Text { text } => text.clone(),
            _ => panic!("expected text block"),
        };
        assert!(text.contains("slug: attention"));
        assert!(text.contains("title: 注意力机制"));
        assert!(text.contains("Original body."));
        assert!(text.contains("multi-head attention"));
    }

    // ── W3 combined prompt tests ──────────────────────────────────

    #[test]
    fn combined_system_suffix_is_extension_only() {
        // The combined path must preserve every single-source hard
        // rule verbatim. It only ADDS guidance for the multi-source
        // case; it must not rewrite the base prompt.
        assert!(COMBINED_SYSTEM_PROMPT_SUFFIX.contains("多条素材"));
        assert!(COMBINED_SYSTEM_PROMPT_SUFFIX.contains("融合"));
        assert!(COMBINED_SYSTEM_PROMPT_SUFFIX.contains("去重"));
    }

    #[test]
    fn build_combined_merge_request_shape_and_body() {
        let sources = vec![
            (10_u32, "Transformer 论文".to_string(), "Body ten.".to_string()),
            (11_u32, "Attention survey".to_string(), "Body eleven.".to_string()),
            (12_u32, "Flash attention".to_string(), "Body twelve.".to_string()),
        ];
        let req = build_combined_merge_request(
            "attention",
            "注意力机制",
            "# Attention\n\n原始正文。",
            &sources,
        );
        assert_eq!(req.model, MAINTAINER_MODEL);
        assert_eq!(req.max_tokens, COMBINED_MERGE_MAX_OUTPUT_TOKENS);
        assert!(!req.stream);

        // System prompt must contain the full MERGE_SYSTEM_PROMPT
        // (hard rules preserved) PLUS the combined suffix.
        let system = req.system.as_deref().expect("system prompt set");
        assert!(system.contains("after_markdown"));
        assert!(system.contains("不要丢失原有信息"));
        assert!(system.contains("多条素材"));

        let text = match &req.messages[0].content[0] {
            InputContentBlock::Text { text } => text.clone(),
            _ => panic!("expected text block"),
        };
        assert!(text.contains("slug: attention"));
        assert!(text.contains("title: 注意力机制"));
        assert!(text.contains("原始正文。"));
        // All 3 sources rendered, each tagged with its inbox id.
        assert!(text.contains("素材 1: Transformer 论文 (inbox #10)"));
        assert!(text.contains("Body ten."));
        assert!(text.contains("素材 2: Attention survey (inbox #11)"));
        assert!(text.contains("Body eleven."));
        assert!(text.contains("素材 3: Flash attention (inbox #12)"));
        assert!(text.contains("Body twelve."));
        // Final instruction carries the N.
        assert!(text.contains("请把所有 3 条新素材"));
    }
}

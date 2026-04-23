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
use std::collections::{HashMap, HashSet};
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
// W1 Maintainer Workbench: three-choice maintain decision workflow
// ─────────────────────────────────────────────────────────────────────
//
// Replaces the W0 "black-box approve" flow with a structured decision:
// the human picks exactly one of CreateNew / UpdateExisting / Reject.
// The backend executes the chosen action end-to-end and returns an
// outcome the frontend can render in the Workbench Result pane.
//
// Design notes:
//
//   * `CreateNew` reuses `propose_for_raw_entry` + `write_wiki_page` —
//     same path the legacy `approve-with-write` handler used, but
//     orchestrated server-side so the frontend never has to re-send
//     a proposal body.
//
//   * `UpdateExisting` does NOT call the LLM in this first version.
//     It loads the target page, appends the raw entry body under a
//     dated heading (`## 更新 [YYYY-MM-DD]`), and writes back. A
//     future version will call an LLM-driven merge — see TODO below.
//
//   * `Reject` is filesystem-only: it writes the reason into the
//     inbox entry, flips status to Rejected, and appends a human-
//     readable audit line to `wiki/log.md`. No broker needed.
//
//   * The top-level `execute_maintain` is async (not the sync `fn`
//     the initial spec sketched) because `CreateNew` needs a broker.
//     The extra arg is added here — a sync-only signature would have
//     forced the HTTP handler to dispatch on the enum itself, which
//     would defeat the "one entry point" intent. The frontend contract
//     (flat `action` / `target_page_slug` / `rejection_reason` fields)
//     is unaffected.

/// Which maintainer action the user picked in the Workbench.
///
/// Wire format: `#[serde(tag = "kind", rename_all = "snake_case")]`
/// so the JSON looks like `{"kind":"create_new"}` /
/// `{"kind":"update_existing","target_page_slug":"..."}` /
/// `{"kind":"reject","reason":"..."}`. The HTTP handler in
/// `desktop-server` translates from the flat frontend contract
/// (`action` / `target_page_slug` / `rejection_reason`) into this
/// tagged enum before calling `execute_maintain`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaintainAction {
    /// Generate a fresh wiki page from the inbox's raw entry.
    /// Legacy propose → approve-with-write path, now server-driven.
    CreateNew,
    /// Append the raw body into an existing wiki page. The merge
    /// strategy is pure append in v1; an LLM-driven merge is TODO.
    UpdateExisting { target_page_slug: String },
    /// Discard the inbox task with a user-provided reason. Does not
    /// touch the wiki — only audit-logs the decision.
    Reject { reason: String },
}

/// Outcome returned by [`execute_maintain`], mirroring the shape
/// surfaced to the frontend via the `/api/wiki/inbox/{id}/maintain`
/// response.
///
/// Wire format: same `tag = "kind"` / `snake_case` convention as
/// `MaintainAction`. The desktop-server handler flattens this into
/// the TS `MaintainResponse` (`outcome` + optional siblings).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaintainOutcome {
    /// A brand-new page was written at `target_page_slug`.
    Created { target_page_slug: String },
    /// An existing page at `target_page_slug` was updated in place.
    Updated { target_page_slug: String },
    /// The inbox task was rejected with `reason`; wiki untouched.
    Rejected { reason: String },
    /// Something went wrong — `error` is a user-visible string.
    Failed { error: String },
}

/// Execute a maintain decision end-to-end.
///
/// Dispatches on the `MaintainAction` variant, performs the filesystem
/// side effects (wiki write, inbox status update, log append), and
/// returns a `MaintainOutcome`. A single entry point for the HTTP
/// handler so all three paths converge on the same "update the
/// `InboxEntry` with maintain bookkeeping fields" behavior.
///
/// Behavior summary by variant:
///
/// | Variant | LLM? | Wiki write | Inbox fields written |
/// |---------|------|------------|----------------------|
/// | `CreateNew` | yes (via `propose_for_raw_entry`) | new page | `maintain_action`, `target_page_slug`, `proposed_wiki_slug`, status=Approved |
/// | `UpdateExisting` | no (append v1) | existing page | `maintain_action`, `target_page_slug`, status=Approved |
/// | `Reject` | no | none | `maintain_action`, `rejection_reason`, status=Rejected + `wiki/log.md` line |
///
/// Errors surface as `Err` rather than `Ok(MaintainOutcome::Failed)`
/// for propagation clarity — the HTTP handler maps them onto the
/// `Failed` variant before returning to the frontend.
pub async fn execute_maintain(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
    action: MaintainAction,
    broker: &(impl BrokerSender + ?Sized),
) -> Result<MaintainOutcome> {
    match action {
        MaintainAction::CreateNew => create_new(paths, inbox_id, broker).await,
        MaintainAction::UpdateExisting { target_page_slug } => {
            update_existing(paths, inbox_id, &target_page_slug)
        }
        MaintainAction::Reject { reason } => reject(paths, inbox_id, &reason),
    }
}

/// Path A — create a brand-new wiki page from the inbox's raw entry.
///
/// Pipeline:
///   1. Resolve `source_raw_id` from the inbox entry.
///   2. Call `propose_for_raw_entry` to get a `WikiPageProposal`.
///   3. `wiki_store::write_wiki_page` to persist the concept page.
///   4. Side-effects: append to `wiki/log.md`, rebuild index,
///      notify affected pages.
///   5. Patch the `InboxEntry` with `maintain_action="create_new"`,
///      `proposed_wiki_slug`, `target_page_slug`, status=Approved.
///
/// Mirrors the legacy `/api/wiki/inbox/{id}/approve-with-write`
/// handler, minus the "frontend re-sends the proposal body" step.
pub async fn create_new(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
    broker: &(impl BrokerSender + ?Sized),
) -> Result<MaintainOutcome> {
    // Step 1: locate the inbox entry + its raw_id.
    let entries = wiki_store::list_inbox_entries(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let entry = entries
        .iter()
        .find(|e| e.id == inbox_id)
        .ok_or_else(|| {
            MaintainerError::RawNotAvailable(format!("inbox entry not found: {inbox_id}"))
        })?;
    let raw_id = entry.source_raw_id.ok_or_else(|| {
        MaintainerError::InvalidProposal(format!(
            "inbox entry {inbox_id} has no source_raw_id"
        ))
    })?;

    // Step 2: LLM proposal.
    let proposal = propose_for_raw_entry(paths, raw_id, broker).await?;

    // Step 3: write concept page.
    wiki_store::write_wiki_page(
        paths,
        &proposal.slug,
        &proposal.title,
        &proposal.summary,
        &proposal.body,
        Some(proposal.source_raw_id),
    )
    .map_err(|e| MaintainerError::Store(e.to_string()))?;

    // Step 4: side-effects (all soft-fail — the page is already on disk).
    let log_title = if proposal.title.is_empty() {
        proposal.slug.clone()
    } else {
        proposal.title.clone()
    };
    let _ = wiki_store::append_wiki_log(paths, "write-concept", &log_title);
    let _ = wiki_store::append_changelog_entry(paths, "write-concept", &log_title);
    let _ = wiki_store::rebuild_wiki_index(paths);
    let _ = wiki_store::notify_affected_pages(paths, &proposal.slug, &proposal.title);

    // Step 5: mark inbox entry approved + stamp maintain fields.
    patch_inbox_after_maintain(
        paths,
        inbox_id,
        InboxMaintainPatch {
            status: wiki_store::InboxStatus::Approved,
            maintain_action: Some("create_new"),
            proposed_wiki_slug: Some(proposal.slug.clone()),
            target_page_slug: Some(proposal.slug.clone()),
            rejection_reason: None,
        },
    )?;

    // P1 provenance: `wiki_page_applied` — the create_new path
    // writes a brand-new wiki page. Upstream = inbox + raw that
    // seeded the proposal; downstream = the new wiki page.
    wiki_store::provenance::fire_event(
        paths,
        wiki_store::provenance::LineageEvent {
            event_id: wiki_store::provenance::new_event_id(),
            event_type: wiki_store::provenance::LineageEventType::WikiPageApplied,
            timestamp_ms: wiki_store::provenance::now_unix_ms(),
            upstream: vec![
                wiki_store::provenance::LineageRef::Inbox { id: inbox_id },
                wiki_store::provenance::LineageRef::Raw { id: raw_id },
            ],
            downstream: vec![wiki_store::provenance::LineageRef::WikiPage {
                slug: proposal.slug.clone(),
                title: Some(proposal.title.clone()),
            }],
            display_title: wiki_store::provenance::display_title_wiki_page_applied(
                &proposal.slug,
            ),
            metadata: serde_json::json!({
                "path": "create_new",
                "title": proposal.title,
            }),
        },
    );

    Ok(MaintainOutcome::Created {
        target_page_slug: proposal.slug,
    })
}

/// Path B — append the inbox's raw body into an existing wiki page.
///
/// v1 strategy: pure append. The raw body is appended under a
/// dated heading (`## 更新 [YYYY-MM-DD]`) so the provenance is
/// obvious in the page's edit history. This is intentionally
/// simpler than a semantic merge — it keeps the caller on a
/// deterministic, fast, LLM-free path while the UX stabilizes.
///
/// TODO(W2+): swap the append for an LLM-driven merge call that
/// reconciles the two bodies semantically (e.g. "add the raw
/// content under the appropriate section, dedupe, preserve voice").
/// The merge entry point will need a `BrokerSender` so the function
/// signature will grow an `&broker` param at that point.
pub fn update_existing(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
    target_page_slug: &str,
) -> Result<MaintainOutcome> {
    // Step 1: read the inbox entry (for source_raw_id) + raw body.
    let entries = wiki_store::list_inbox_entries(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let entry = entries
        .iter()
        .find(|e| e.id == inbox_id)
        .ok_or_else(|| {
            MaintainerError::RawNotAvailable(format!("inbox entry not found: {inbox_id}"))
        })?;
    let raw_id = entry.source_raw_id.ok_or_else(|| {
        MaintainerError::InvalidProposal(format!(
            "inbox entry {inbox_id} has no source_raw_id"
        ))
    })?;

    let (_raw_entry, raw_body) = wiki_store::read_raw_entry(paths, raw_id)
        .map_err(|e| MaintainerError::RawNotAvailable(e.to_string()))?;

    // Step 2: load the target page (summary + body). Error propagates
    // if the slug doesn't exist — it's a user-controlled input and the
    // handler will surface the 404 equivalent.
    let (summary, existing_body) = wiki_store::read_wiki_page(paths, target_page_slug)
        .map_err(|e| MaintainerError::Store(format!(
            "target page `{target_page_slug}` not found: {e}"
        )))?;

    // Step 3: append strategy — dated heading + raw body under it.
    // The date is ISO `YYYY-MM-DD` per the existing log format.
    let today = {
        let iso = wiki_store::now_iso8601();
        iso.split('T').next().unwrap_or(&iso).to_string()
    };
    let mut merged = existing_body.trim_end_matches('\n').to_string();
    merged.push_str("\n\n## 更新 [");
    merged.push_str(&today);
    merged.push_str("]\n\n");
    merged.push_str(raw_body.trim_end_matches('\n'));
    merged.push('\n');

    // Step 4: write back. Preserve existing title/summary — only body changes.
    wiki_store::write_wiki_page_in_category(
        paths,
        &summary.category,
        &summary.slug,
        &summary.title,
        &summary.summary,
        &merged,
        summary.source_raw_id,
    )
    .map_err(|e| MaintainerError::Store(e.to_string()))?;

    // Step 5: side-effects. log + rebuild index mirror `create_new`.
    let log_title = if summary.title.is_empty() {
        summary.slug.clone()
    } else {
        summary.title.clone()
    };
    let _ = wiki_store::append_wiki_log(paths, "update-concept", &log_title);
    let _ = wiki_store::append_changelog_entry(paths, "update-concept", &log_title);
    let _ = wiki_store::rebuild_wiki_index(paths);

    // Step 6: mark inbox approved + stamp maintain fields.
    patch_inbox_after_maintain(
        paths,
        inbox_id,
        InboxMaintainPatch {
            status: wiki_store::InboxStatus::Approved,
            maintain_action: Some("update_existing"),
            proposed_wiki_slug: None,
            target_page_slug: Some(target_page_slug.to_string()),
            rejection_reason: None,
        },
    )?;

    // P1 provenance: `wiki_page_applied` — the v1 deterministic
    // append path ran a raw body into an existing page. Upstream =
    // inbox + raw; downstream = the updated wiki page.
    wiki_store::provenance::fire_event(
        paths,
        wiki_store::provenance::LineageEvent {
            event_id: wiki_store::provenance::new_event_id(),
            event_type: wiki_store::provenance::LineageEventType::WikiPageApplied,
            timestamp_ms: wiki_store::provenance::now_unix_ms(),
            upstream: vec![
                wiki_store::provenance::LineageRef::Inbox { id: inbox_id },
                wiki_store::provenance::LineageRef::Raw { id: raw_id },
            ],
            downstream: vec![wiki_store::provenance::LineageRef::WikiPage {
                slug: target_page_slug.to_string(),
                title: Some(summary.title.clone()),
            }],
            display_title: wiki_store::provenance::display_title_wiki_page_applied(
                target_page_slug,
            ),
            metadata: serde_json::json!({
                "path": "update_existing",
            }),
        },
    );

    Ok(MaintainOutcome::Updated {
        target_page_slug: target_page_slug.to_string(),
    })
}

/// Path C — reject the inbox task with a human-visible reason.
///
/// No wiki mutation. Writes:
///   * the reason into `InboxEntry.rejection_reason`
///   * `maintain_action="reject"`
///   * status = Rejected
///   * one audit line into `wiki/log.md`
///
/// Log format (canonical §8 Triggers): a single `- [YYYY-MM-DD HH:MM]
/// reject-inbox | inbox/{id} — reason: {reason}` entry. Reusing
/// `append_wiki_log` keeps the file shape consistent with
/// `write-concept` / `update-concept` lines; the verb string is
/// `reject-inbox` so a grep can separate the three paths.
pub fn reject(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
    reason: &str,
) -> Result<MaintainOutcome> {
    // Step 1: ensure the inbox entry exists so we fail fast if the
    // id is stale, rather than appending a log entry for a ghost.
    let entries = wiki_store::list_inbox_entries(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let entry_title = entries
        .iter()
        .find(|e| e.id == inbox_id)
        .map(|e| e.title.clone())
        .ok_or_else(|| {
            MaintainerError::RawNotAvailable(format!("inbox entry not found: {inbox_id}"))
        })?;

    // Step 2: stamp maintain fields on the inbox entry + flip status.
    patch_inbox_after_maintain(
        paths,
        inbox_id,
        InboxMaintainPatch {
            status: wiki_store::InboxStatus::Rejected,
            maintain_action: Some("reject"),
            proposed_wiki_slug: None,
            target_page_slug: None,
            rejection_reason: Some(reason.to_string()),
        },
    )?;

    // Step 3: append a reject line to wiki/log.md. The `reject-inbox`
    // verb + `inbox/{id} — reason: {reason}` title gives us a clean
    // greppable audit trail (see canonical §8).
    let title = format!("inbox/{inbox_id} — reason: {reason}");
    let _ = wiki_store::append_wiki_log(paths, "reject-inbox", &title);

    // P1 provenance: `inbox_rejected`. Upstream = the inbox id that
    // was just rejected; downstream = empty (reject does not produce
    // a further event). Reason is echoed into metadata so the UI can
    // surface why without re-reading log.md.
    wiki_store::provenance::fire_event(
        paths,
        wiki_store::provenance::LineageEvent {
            event_id: wiki_store::provenance::new_event_id(),
            event_type: wiki_store::provenance::LineageEventType::InboxRejected,
            timestamp_ms: wiki_store::provenance::now_unix_ms(),
            upstream: vec![wiki_store::provenance::LineageRef::Inbox { id: inbox_id }],
            downstream: vec![],
            display_title: wiki_store::provenance::display_title_inbox_rejected(
                &entry_title,
            ),
            metadata: serde_json::json!({
                "reason": reason,
            }),
        },
    );

    Ok(MaintainOutcome::Rejected {
        reason: reason.to_string(),
    })
}

/// Private helper: atomically patch an inbox entry's W1 maintain
/// bookkeeping fields + status + `resolved_at`. Used by all three
/// maintain paths so the field write set stays consistent.
struct InboxMaintainPatch {
    status: wiki_store::InboxStatus,
    maintain_action: Option<&'static str>,
    proposed_wiki_slug: Option<String>,
    target_page_slug: Option<String>,
    rejection_reason: Option<String>,
}

/// Apply an `InboxMaintainPatch` to the given entry by reading the
/// current inbox list, mutating in place, and re-saving.
///
/// Separate from `wiki_store::resolve_inbox_entry` because that
/// helper only flips status — the W1 fields (`maintain_action`,
/// `target_page_slug`, `rejection_reason`) aren't part of its contract.
/// Rather than overload the existing function, we go through the
/// raw load/save pair here so the maintainer crate owns the maintain
/// fields end-to-end.
fn patch_inbox_after_maintain(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
    patch: InboxMaintainPatch,
) -> Result<()> {
    wiki_store::update_inbox_maintain(
        paths,
        inbox_id,
        patch.status,
        patch.maintain_action.map(str::to_string),
        patch.proposed_wiki_slug,
        patch.target_page_slug,
        patch.rejection_reason,
    )
    .map(|_| ())
    .map_err(|e| MaintainerError::Store(e.to_string()))
}

// ─────────────────────────────────────────────────────────────────────
// W2: two-phase update_existing (propose → review → apply)
// ─────────────────────────────────────────────────────────────────────
//
// W1 shipped `update_existing` as a deterministic "append under a
// dated heading" step. W2 splits that into two HTTP hops so the
// human can preview the LLM's merge before it lands on disk:
//
//   1. `propose_update`   — read existing page + raw body, call the
//                           LLM for a merge, persist the proposal
//                           on the InboxEntry, return to the UI.
//   2. `apply_update_proposal` — user clicked Apply; write the
//                           `proposed_after_markdown` to disk, flip
//                           the inbox to `Approved`, clear the
//                           staged markdown but keep the summary.
//   3. `cancel_update_proposal` — user backed out; clear the
//                           staged proposal so they can pick a
//                           different action.
//
// The legacy deterministic append (`update_existing` function above)
// is kept in place for backward compat with any caller that still
// dispatches through `execute_maintain`; the new HTTP endpoints in
// `desktop-server` (`/api/wiki/inbox/{id}/proposal{,/apply,/cancel}`)
// are the W2-native path.

/// A staged merge from `propose_update`. Carries everything the
/// frontend needs to render a diff preview plus the data
/// `apply_update_proposal` needs to commit — no further LLM call
/// is necessary at apply time (that would be nondeterministic).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateProposal {
    /// Slug of the wiki page the proposal targets.
    pub target_slug: String,
    /// Existing page body at the moment the proposal was generated.
    /// Used by the frontend for the "before" pane of the diff and
    /// by `apply_update_proposal` to detect concurrent edits.
    pub before_markdown: String,
    /// LLM-produced merged page body. Does not contain YAML
    /// frontmatter — the apply step reuses the existing summary.
    pub after_markdown: String,
    /// One-line human-readable description of what changed.
    pub summary: String,
    /// Unix milliseconds timestamp of when the proposal was
    /// generated. Surfaced in the UI so the user can see whether
    /// a stale proposal needs regenerating.
    pub generated_at: u64,
}

/// Phase 1 of the two-phase update: call the LLM for a merge and
/// persist the proposal on the inbox entry.
///
/// Flow:
///   1. Resolve the inbox entry and its `source_raw_id`.
///   2. Read the existing wiki page (snapshotted as `before`).
///   3. Read the raw entry body.
///   4. Build the merge prompt, fire one `chat_completion`.
///   5. Parse `{after_markdown, summary}` out of the response.
///   6. Persist the proposal on the inbox entry (fields
///      `proposal_status=pending`, `proposed_after_markdown=after`,
///      `before_markdown_snapshot=before`, `proposal_summary=summary`).
///   7. Also stamp `maintain_action="update_existing"` +
///      `target_page_slug=target_slug` so the UI can rediscover
///      which page the proposal targets after a refresh.
///   8. Return `UpdateProposal` for the immediate HTTP response.
pub async fn propose_update(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
    target_slug: &str,
    broker: &(impl BrokerSender + ?Sized),
) -> Result<UpdateProposal> {
    // Step 1 — resolve inbox entry.
    let entries = wiki_store::list_inbox_entries(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let entry = entries.iter().find(|e| e.id == inbox_id).ok_or_else(|| {
        MaintainerError::RawNotAvailable(format!("inbox entry not found: {inbox_id}"))
    })?;
    let raw_id = entry.source_raw_id.ok_or_else(|| {
        MaintainerError::InvalidProposal(format!(
            "inbox entry {inbox_id} has no source_raw_id"
        ))
    })?;

    // Step 2 — read existing page (snapshot for concurrent-edit detection).
    let (target_summary, before_markdown) = wiki_store::read_wiki_page(paths, target_slug)
        .map_err(|e| {
            MaintainerError::Store(format!(
                "target page `{target_slug}` not found: {e}"
            ))
        })?;

    // Step 3 — read raw body.
    let (_raw_entry, raw_body) = wiki_store::read_raw_entry(paths, raw_id)
        .map_err(|e| MaintainerError::RawNotAvailable(e.to_string()))?;

    // Step 4 — build merge prompt, call broker.
    let request = prompt::build_merge_request(
        target_slug,
        &target_summary.title,
        &before_markdown,
        &raw_body,
    );
    let response = broker.chat_completion(request).await?;
    let raw_text = extract_first_text(&response).ok_or_else(|| {
        MaintainerError::InvalidProposal(
            "LLM merge response contained no text block".to_string(),
        )
    })?;

    // Step 5 — parse `{after_markdown, summary}` JSON.
    let (after_markdown, summary) = parse_merge_response(&raw_text)?;

    // Step 6+7 — persist proposal on inbox entry.
    //   propose keeps status as Pending; only apply marks it Approved.
    //   We also stamp maintain_action + target_page_slug so an
    //   intervening page refresh doesn't lose the user's choice.
    wiki_store::update_inbox_proposal(
        paths,
        inbox_id,
        wiki_store::InboxProposalPatch {
            status: None, // stays Pending
            proposal_status: wiki_store::ClearableOption::Set("pending".to_string()),
            proposed_after_markdown: wiki_store::ClearableOption::Set(after_markdown.clone()),
            before_markdown_snapshot: wiki_store::ClearableOption::Set(before_markdown.clone()),
            proposal_summary: wiki_store::ClearableOption::Set(summary.clone()),
            maintain_action: wiki_store::ClearableOption::Set("update_existing".to_string()),
            target_page_slug: wiki_store::ClearableOption::Set(target_slug.to_string()),
        },
    )
    .map_err(|e| MaintainerError::Store(e.to_string()))?;

    let generated_at = unix_ms_now();

    // P1 provenance: `proposal_generated` — the LLM merge proposal has
    // been persisted to the inbox entry. The W3 preview path does NOT
    // fire (it produces a throwaway preview, not a persisted proposal).
    // Upstream = inbox + raw; downstream = the target wiki page the
    // proposal would touch if the user accepts.
    wiki_store::provenance::fire_event(
        paths,
        wiki_store::provenance::LineageEvent {
            event_id: wiki_store::provenance::new_event_id(),
            event_type: wiki_store::provenance::LineageEventType::ProposalGenerated,
            timestamp_ms: wiki_store::provenance::now_unix_ms(),
            upstream: vec![
                wiki_store::provenance::LineageRef::Inbox { id: inbox_id },
                wiki_store::provenance::LineageRef::Raw { id: raw_id },
            ],
            downstream: vec![wiki_store::provenance::LineageRef::WikiPage {
                slug: target_slug.to_string(),
                title: Some(target_summary.title.clone()),
            }],
            display_title: wiki_store::provenance::display_title_proposal_generated(
                target_slug,
            ),
            metadata: serde_json::json!({
                "summary": summary,
            }),
        },
    );

    Ok(UpdateProposal {
        target_slug: target_slug.to_string(),
        before_markdown,
        after_markdown,
        summary,
        generated_at,
    })
}

/// Phase 2 of the two-phase update: commit a previously-proposed
/// merge to disk. Idempotent for the "already applied" case — if
/// the proposal was already committed on a previous call we still
/// return `Updated` so the UI can stay simple.
///
/// Error cases:
///   * `InvalidProposal` — no pending proposal exists on this entry.
///   * `Store` — concurrent-edit detection: the page on disk no
///     longer matches `before_markdown_snapshot`. The user must
///     re-propose against the new content.
pub fn apply_update_proposal(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
) -> Result<MaintainOutcome> {
    // Step 1 — locate entry, validate that a pending proposal exists.
    let entries = wiki_store::list_inbox_entries(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let entry = entries
        .iter()
        .find(|e| e.id == inbox_id)
        .ok_or_else(|| {
            MaintainerError::RawNotAvailable(format!("inbox entry not found: {inbox_id}"))
        })?
        .clone();

    if entry.proposal_status.as_deref() != Some("pending") {
        return Err(MaintainerError::InvalidProposal(format!(
            "inbox entry {inbox_id} has no pending proposal (status={:?})",
            entry.proposal_status
        )));
    }
    let target_slug = entry.target_page_slug.as_deref().ok_or_else(|| {
        MaintainerError::InvalidProposal(format!(
            "inbox entry {inbox_id} proposal is missing target_page_slug"
        ))
    })?;
    let after_markdown = entry.proposed_after_markdown.clone().ok_or_else(|| {
        MaintainerError::InvalidProposal(format!(
            "inbox entry {inbox_id} proposal is missing proposed_after_markdown"
        ))
    })?;

    // Step 2 — concurrent-edit detection. If the page has changed
    // since we snapshotted it, refuse to apply: the user must
    // re-propose against the new content or resolve the conflict
    // manually. Wrapped in `Store` because it's effectively a
    // filesystem-state precondition failure.
    let (existing_summary, existing_body) = wiki_store::read_wiki_page(paths, target_slug)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    if let Some(snapshot) = entry.before_markdown_snapshot.as_deref() {
        if snapshot != existing_body {
            return Err(MaintainerError::Store(format!(
                "page `{target_slug}` changed since proposal was generated; \
                 please regenerate the proposal (conflict)"
            )));
        }
    }

    // Step 3 — write the merged body back, preserving category + title + summary.
    wiki_store::write_wiki_page_in_category(
        paths,
        &existing_summary.category,
        &existing_summary.slug,
        &existing_summary.title,
        &existing_summary.summary,
        &after_markdown,
        existing_summary.source_raw_id,
    )
    .map_err(|e| MaintainerError::Store(e.to_string()))?;

    // Step 4 — audit log + index rebuild (best-effort).
    let log_title = if existing_summary.title.is_empty() {
        existing_summary.slug.clone()
    } else {
        existing_summary.title.clone()
    };
    let _ = wiki_store::append_wiki_log(paths, "update-concept", &log_title);
    let _ = wiki_store::append_changelog_entry(paths, "update-concept", &log_title);
    let _ = wiki_store::rebuild_wiki_index(paths);

    // Step 5 — flip proposal_status to applied, inbox status to
    // Approved, clear the (bulky) staged markdown but keep the
    // summary for audit trail. Also stamp maintain_action /
    // target_page_slug so the W1 bookkeeping stays consistent with
    // create_new / reject (propose already stamped these, but we
    // overwrite defensively for the case where propose was missed).
    wiki_store::update_inbox_proposal(
        paths,
        inbox_id,
        wiki_store::InboxProposalPatch {
            status: Some(wiki_store::InboxStatus::Approved),
            proposal_status: wiki_store::ClearableOption::Set("applied".to_string()),
            proposed_after_markdown: wiki_store::ClearableOption::Clear,
            before_markdown_snapshot: wiki_store::ClearableOption::Clear,
            proposal_summary: wiki_store::ClearableOption::Keep, // retain for audit
            maintain_action: wiki_store::ClearableOption::Set("update_existing".to_string()),
            target_page_slug: wiki_store::ClearableOption::Set(target_slug.to_string()),
        },
    )
    .map_err(|e| MaintainerError::Store(e.to_string()))?;

    // P1 provenance: `wiki_page_applied` — W2 apply wrote the merged
    // after_markdown back to the target page. Raw id is recovered
    // from the original inbox entry so the event carries the full
    // `inbox + raw → wiki_page` triangle.
    let upstream_raw_id = entry.source_raw_id;
    let mut upstream: Vec<wiki_store::provenance::LineageRef> =
        vec![wiki_store::provenance::LineageRef::Inbox { id: inbox_id }];
    if let Some(rid) = upstream_raw_id {
        upstream.push(wiki_store::provenance::LineageRef::Raw { id: rid });
    }
    wiki_store::provenance::fire_event(
        paths,
        wiki_store::provenance::LineageEvent {
            event_id: wiki_store::provenance::new_event_id(),
            event_type: wiki_store::provenance::LineageEventType::WikiPageApplied,
            timestamp_ms: wiki_store::provenance::now_unix_ms(),
            upstream,
            downstream: vec![wiki_store::provenance::LineageRef::WikiPage {
                slug: target_slug.to_string(),
                title: Some(existing_summary.title.clone()),
            }],
            display_title: wiki_store::provenance::display_title_wiki_page_applied(
                target_slug,
            ),
            metadata: serde_json::json!({
                "path": "apply_update_proposal",
            }),
        },
    );

    Ok(MaintainOutcome::Updated {
        target_page_slug: target_slug.to_string(),
    })
}

/// Phase 2-alt: user bailed out of a proposal. Clears the staged
/// fields; leaves `maintain_action` / `target_page_slug` untouched
/// so the user can pick a different action without re-navigating.
///
/// Idempotent: calling cancel on an entry that has no pending
/// proposal is a no-op (returns `Ok(())`).
pub fn cancel_update_proposal(
    paths: &wiki_store::WikiPaths,
    inbox_id: u32,
) -> Result<()> {
    wiki_store::update_inbox_proposal(
        paths,
        inbox_id,
        wiki_store::InboxProposalPatch {
            status: None, // stays Pending
            proposal_status: wiki_store::ClearableOption::Set("cancelled".to_string()),
            proposed_after_markdown: wiki_store::ClearableOption::Clear,
            before_markdown_snapshot: wiki_store::ClearableOption::Clear,
            // Keep proposal_summary so the UI can still show "what the
            // last proposal did" even after cancel — helps the user
            // decide whether to re-propose.
            proposal_summary: wiki_store::ClearableOption::Keep,
            // Keep maintain_action / target_page_slug so the user's
            // choice of which page to update survives the cancel —
            // they can re-propose without re-navigating.
            maintain_action: wiki_store::ClearableOption::Keep,
            target_page_slug: wiki_store::ClearableOption::Keep,
        },
    )
    .map_err(|e| MaintainerError::Store(e.to_string()))?;
    Ok(())
}

/// Unix milliseconds wall-clock timestamp. Separate helper because
/// `SystemTime::now()` returns a `SystemTime` and coercing it into
/// a `u64` is noisy at call sites. Truncation on the `u128 → u64`
/// step is fine here: `u64` overflow on millisecond-since-epoch
/// doesn't happen until year 2554.
#[allow(clippy::cast_possible_truncation)]
fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Same as [`unix_ms_now`] but as `i64` for shapes that mirror
/// JS/TS `number` (Date.now()) on the wire. Used by the W3 combined
/// proposal response. Overflow has the same year-2554 floor as
/// `unix_ms_now`.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn unix_ms_now_i64() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Compute the SHA-256 of `input`, returning lowercase hex. Used by
/// the W3 combined proposal engine to build the `before_hash` guard
/// that detects concurrent edits between preview and apply.
#[must_use]
pub fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// W3: combined (multi-source) proposal engine
// ─────────────────────────────────────────────────────────────────────
//
// W3 layers a "combined" merge path on top of the single-source W2
// flow. The shape: user picks 2..=6 pending inbox entries plus one
// target page; server asks the LLM to fold them all in one shot;
// user reviews the diff; server atomically writes the wiki page and
// flips all N inbox entries to Approved.
//
// Ephemeral-bundle design (see W3 contract):
//   * Preview writes NO inbox staging fields — the diff is built on
//     the fly and returned in one response.
//   * Apply receives the frontend-echoed `after_markdown` + `summary`
//     plus an `expected_before_hash` so we can detect concurrent
//     edits without storing a snapshot.
//
// This keeps zero new InboxEntry fields (wiki_store is critical path
// for data loss) and reuses every W2 staging field for the apply
// flip.

/// Per-source metadata returned in the combined preview response. The
/// frontend uses these to render the "merging N sources" header
/// (title list + inbox ids) without re-fetching each raw entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedProposalSource {
    /// Inbox entry id (the row the user selected).
    pub inbox_id: u32,
    /// Human-readable title pulled from the inbox entry.
    pub title: String,
    /// The underlying raw entry id if known. Echoed to the frontend
    /// so the UI can link back to the original paste/url/file for
    /// inspection.
    pub source_raw_id: Option<u32>,
}

/// Response envelope for `POST /api/wiki/proposal/combined` —
/// the preview produced by [`propose_combined_update`]. Carries the
/// before/after markdown + a SHA-256 hash the frontend echoes back
/// to the apply call as `expected_before_hash`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedProposalResponse {
    pub target_slug: String,
    pub inbox_ids: Vec<u32>,
    pub before_markdown: String,
    pub after_markdown: String,
    pub summary: String,
    /// Lowercase hex SHA-256 of `before_markdown`, used by the apply
    /// endpoint for a concurrent-edit guard.
    pub before_hash: String,
    /// Epoch milliseconds when this preview was generated.
    pub generated_at: i64,
    /// One entry per source id, in the same order as `inbox_ids`.
    pub source_titles: Vec<CombinedProposalSource>,
}

/// Outcome envelope for [`apply_combined_proposal`]. The HTTP handler
/// stringifies `outcome` onto the wire so the frontend can branch on
/// a consistent string across success / concurrent-edit / partial /
/// stale-inbox paths.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedApplyResult {
    /// One of:
    ///   * `"applied"` — wiki page written, all N inbox entries flipped.
    ///   * `"concurrent_edit"` — the page changed between preview and
    ///     apply; no write happened.
    ///   * `"stale_inbox"` — one or more inbox ids disappeared or left
    ///     Pending between preview and apply; no write happened.
    ///   * `"partial_applied"` — wiki page wrote successfully but at
    ///     least one inbox flip failed; `failed_ids` lists the survivors.
    pub outcome: String,
    /// Slug of the target page that was (or would have been) updated.
    pub target_page_slug: String,
    /// Inbox ids that were successfully flipped to Approved.
    pub applied_inbox_ids: Vec<u32>,
    /// Inbox ids that failed to flip (only populated on
    /// `outcome == "partial_applied"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_inbox_ids: Vec<u32>,
    /// Single-line audit entry the server appended to `wiki/log.md`
    /// (empty string when no log line was written, e.g. on
    /// concurrent_edit / stale_inbox). Echoed to the frontend so a
    /// future UI can show "logged as …" without re-reading the log.
    pub audit_entry: String,
}

/// Bound on how many inbox sources a combined proposal may fold.
///
/// Lower bound (2) comes from the W3 contract: one-source merges stay
/// on the W2 single-source path so the LLM never sees combined-merge
/// framing for a trivial input. Upper bound (6) keeps the prompt
/// token budget bounded and gives us headroom below the 8000-token
/// `COMBINED_MERGE_MAX_OUTPUT_TOKENS` cap.
pub const COMBINED_MIN_SOURCES: usize = 2;
pub const COMBINED_MAX_SOURCES: usize = 6;

/// Validation error modes for the combined preview/apply flow. Kept
/// as plain `MaintainerError::InvalidProposal` strings so the HTTP
/// handler only needs to translate one error variant into a 400.
fn validate_combined_inputs(target_slug: &str, inbox_ids: &[u32]) -> Result<()> {
    if target_slug.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "target_slug is required".to_string(),
        ));
    }
    if inbox_ids.len() < COMBINED_MIN_SOURCES || inbox_ids.len() > COMBINED_MAX_SOURCES {
        return Err(MaintainerError::InvalidProposal(format!(
            "combined proposal requires {COMBINED_MIN_SOURCES}..={COMBINED_MAX_SOURCES} \
             inbox sources, got {}",
            inbox_ids.len()
        )));
    }
    // Duplicate ids would make the UI ambiguous and let the LLM see
    // the same body twice. Reject early rather than dedup silently.
    let mut seen = HashSet::with_capacity(inbox_ids.len());
    for id in inbox_ids {
        if !seen.insert(*id) {
            return Err(MaintainerError::InvalidProposal(format!(
                "duplicate inbox id: {id}"
            )));
        }
    }
    Ok(())
}

/// Phase 1 of the W3 combined flow: fan out to 1 LLM call that folds
/// N inbox entries into a single wiki page. Does NOT write to the
/// inbox file — the response is ephemeral and the frontend echoes
/// the critical pieces back on apply.
///
/// Flow:
///   1. Validate `inbox_ids.len() in 2..=6` + `target_slug` non-empty.
///   2. Load the inbox, resolve each id to a pending entry with a
///      `source_raw_id` (any missing/non-pending/no-raw → 400).
///   3. Read the target page (not found → 404 at the HTTP layer).
///   4. Read each raw body.
///   5. Compute the `before_hash` of the current page body.
///   6. Build the combined merge prompt, fire one `chat_completion`.
///   7. Parse `{after_markdown, summary}` out of the response.
///   8. Return `CombinedProposalResponse` for immediate HTTP send.
pub async fn propose_combined_update(
    paths: &wiki_store::WikiPaths,
    target_slug: &str,
    inbox_ids: &[u32],
    broker: &(impl BrokerSender + ?Sized),
) -> Result<CombinedProposalResponse> {
    // Step 1 — validate input shape.
    validate_combined_inputs(target_slug, inbox_ids)?;

    // Step 2 — resolve each inbox id to a Pending entry.
    let all_entries = wiki_store::list_inbox_entries(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let by_id: HashMap<u32, wiki_store::InboxEntry> = all_entries
        .into_iter()
        .map(|e| (e.id, e))
        .collect();

    let mut resolved: Vec<(wiki_store::InboxEntry, u32)> = Vec::with_capacity(inbox_ids.len());
    for &id in inbox_ids {
        let entry = by_id.get(&id).cloned().ok_or_else(|| {
            MaintainerError::InvalidProposal(format!("inbox entry not found: {id}"))
        })?;
        if !matches!(entry.status, wiki_store::InboxStatus::Pending) {
            return Err(MaintainerError::InvalidProposal(format!(
                "inbox {id} not pending (status={:?})",
                entry.status
            )));
        }
        let raw_id = entry.source_raw_id.ok_or_else(|| {
            MaintainerError::InvalidProposal(format!(
                "inbox {id} missing source_raw_id"
            ))
        })?;
        resolved.push((entry, raw_id));
    }

    // Step 3 — read target page.
    let (target_summary, before_markdown) = wiki_store::read_wiki_page(paths, target_slug)
        .map_err(|e| {
            MaintainerError::Store(format!(
                "target page `{target_slug}` not found: {e}"
            ))
        })?;

    // Step 4 — read each raw body, collecting (inbox_id, title, body).
    let mut prompt_sources: Vec<(u32, String, String)> = Vec::with_capacity(resolved.len());
    let mut source_titles: Vec<CombinedProposalSource> = Vec::with_capacity(resolved.len());
    for (entry, raw_id) in &resolved {
        let (_raw_entry, raw_body) = wiki_store::read_raw_entry(paths, *raw_id)
            .map_err(|e| MaintainerError::RawNotAvailable(e.to_string()))?;
        prompt_sources.push((entry.id, entry.title.clone(), raw_body));
        source_titles.push(CombinedProposalSource {
            inbox_id: entry.id,
            title: entry.title.clone(),
            source_raw_id: Some(*raw_id),
        });
    }

    // Step 5 — hash the before body for the apply-time concurrent-edit
    // guard. The frontend will echo this back in `expected_before_hash`.
    let before_hash = sha256_hex(&before_markdown);

    // Step 6 — build + fire combined prompt.
    let request = prompt::build_combined_merge_request(
        target_slug,
        &target_summary.title,
        &before_markdown,
        &prompt_sources,
    );
    let response = broker.chat_completion(request).await?;
    let raw_text = extract_first_text(&response).ok_or_else(|| {
        MaintainerError::InvalidProposal(
            "LLM combined merge response contained no text block".to_string(),
        )
    })?;

    // Step 7 — parse `{after_markdown, summary}`.
    let (after_markdown, summary) = parse_merge_response(&raw_text)?;

    Ok(CombinedProposalResponse {
        target_slug: target_slug.to_string(),
        inbox_ids: inbox_ids.to_vec(),
        before_markdown,
        after_markdown,
        summary,
        before_hash,
        generated_at: unix_ms_now_i64(),
        source_titles,
    })
}

/// Phase 2 of the W3 combined flow: atomically write the merged
/// markdown to the target page and flip N inbox entries to Approved.
///
/// Contract (matches the W3 spec):
///   * Every input that failed the preview guard fails again here —
///     we re-run `validate_combined_inputs` + re-check each entry is
///     still Pending with a `source_raw_id`. Stale state returns
///     `outcome: "stale_inbox"` so the UI can re-fetch.
///   * Concurrent-edit detection uses SHA-256 of the current page
///     body against `expected_before_hash`. Mismatch →
///     `outcome: "concurrent_edit"`.
///   * Inbox flips are best-effort: the wiki write lands first, then
///     each inbox `update_inbox_proposal` is attempted. A failure on
///     one flip does NOT roll back the wiki write — we return
///     `outcome: "partial_applied"` with the failing ids. This
///     matches the W3 atomicity note in the spec.
pub fn apply_combined_proposal(
    paths: &wiki_store::WikiPaths,
    target_slug: &str,
    inbox_ids: &[u32],
    expected_before_hash: &str,
    after_markdown: &str,
    summary: &str,
) -> Result<CombinedApplyResult> {
    // Step 1 — re-validate basic input shape (guards against hand-rolled
    // HTTP callers).
    validate_combined_inputs(target_slug, inbox_ids)?;
    if after_markdown.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "after_markdown is empty".to_string(),
        ));
    }
    if summary.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "summary is empty".to_string(),
        ));
    }

    // Step 2 — re-resolve inbox entries, mirroring the preview guards.
    // Stale state (missing id / non-pending / missing raw) fails with
    // outcome="stale_inbox" rather than erroring out, so the UI can
    // recover by re-fetching.
    let all_entries = wiki_store::list_inbox_entries(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let by_id: HashMap<u32, wiki_store::InboxEntry> = all_entries
        .into_iter()
        .map(|e| (e.id, e))
        .collect();
    for &id in inbox_ids {
        match by_id.get(&id) {
            None => {
                return Ok(CombinedApplyResult {
                    outcome: "stale_inbox".to_string(),
                    target_page_slug: target_slug.to_string(),
                    applied_inbox_ids: Vec::new(),
                    failed_inbox_ids: Vec::new(),
                    audit_entry: String::new(),
                });
            }
            Some(entry) => {
                if !matches!(entry.status, wiki_store::InboxStatus::Pending) {
                    return Ok(CombinedApplyResult {
                        outcome: "stale_inbox".to_string(),
                        target_page_slug: target_slug.to_string(),
                        applied_inbox_ids: Vec::new(),
                        failed_inbox_ids: Vec::new(),
                        audit_entry: String::new(),
                    });
                }
                if entry.source_raw_id.is_none() {
                    return Ok(CombinedApplyResult {
                        outcome: "stale_inbox".to_string(),
                        target_page_slug: target_slug.to_string(),
                        applied_inbox_ids: Vec::new(),
                        failed_inbox_ids: Vec::new(),
                        audit_entry: String::new(),
                    });
                }
            }
        }
    }

    // Step 3 — concurrent-edit detection: hash the current page and
    // compare to the expected hash captured at preview time.
    let (existing_summary, existing_body) = wiki_store::read_wiki_page(paths, target_slug)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let current_hash = sha256_hex(&existing_body);
    if current_hash != expected_before_hash {
        return Ok(CombinedApplyResult {
            outcome: "concurrent_edit".to_string(),
            target_page_slug: target_slug.to_string(),
            applied_inbox_ids: Vec::new(),
            failed_inbox_ids: Vec::new(),
            audit_entry: String::new(),
        });
    }

    // Step 4 — write the merged page, preserving category + title +
    // summary (the wiki-frontmatter summary, not the LLM's per-change
    // `summary` arg, which goes into the audit log).
    wiki_store::write_wiki_page_in_category(
        paths,
        &existing_summary.category,
        &existing_summary.slug,
        &existing_summary.title,
        &existing_summary.summary,
        after_markdown,
        existing_summary.source_raw_id,
    )
    .map_err(|e| MaintainerError::Store(e.to_string()))?;

    // Step 5 — atomic-ish loop over the N inbox entries. Best-effort
    // per W3 spec: a flip failure does not roll back the wiki write;
    // we collect success/failure lists and report `partial_applied`.
    // `before_markdown_snapshot` is set to the pre-write body we just
    // read, so a future audit can still answer "what was the page
    // before this batch".
    let mut applied: Vec<u32> = Vec::with_capacity(inbox_ids.len());
    let mut failed: Vec<u32> = Vec::new();
    for &id in inbox_ids {
        let patch = wiki_store::InboxProposalPatch {
            status: Some(wiki_store::InboxStatus::Approved),
            proposal_status: wiki_store::ClearableOption::Set("applied".to_string()),
            proposed_after_markdown: wiki_store::ClearableOption::Set(after_markdown.to_string()),
            before_markdown_snapshot: wiki_store::ClearableOption::Set(existing_body.clone()),
            proposal_summary: wiki_store::ClearableOption::Set(summary.to_string()),
            maintain_action: wiki_store::ClearableOption::Set("update_existing".to_string()),
            target_page_slug: wiki_store::ClearableOption::Set(target_slug.to_string()),
        };
        match wiki_store::update_inbox_proposal(paths, id, patch) {
            Ok(_) => applied.push(id),
            Err(_) => failed.push(id),
        }
    }

    // Step 6 — append the combined audit line. Format per W3 spec:
    // `## [YYYY-MM-DD HH:MM] update-concept-combined | {target_slug} (N sources: inbox/{ids comma-joined})`.
    // `append_wiki_log` wraps both the timestamp and the `## [...]`
    // framing; we pass the composed "title" portion so the line lands
    // with the right verb + right body.
    let ids_joined = applied
        .iter()
        .map(|id| format!("inbox/{id}"))
        .collect::<Vec<_>>()
        .join(",");
    let log_title = format!(
        "{target_slug} ({n} sources: {ids_joined})",
        n = applied.len()
    );
    let _ = wiki_store::append_wiki_log(paths, "update-concept-combined", &log_title);
    let _ = wiki_store::append_changelog_entry(paths, "update-concept-combined", &log_title);
    let _ = wiki_store::rebuild_wiki_index(paths);

    let outcome = if failed.is_empty() {
        "applied".to_string()
    } else {
        "partial_applied".to_string()
    };

    // P1 provenance: `combined_wiki_page_applied` — N inbox entries
    // were merged into a single wiki page in one apply call. Upstream
    // is every applied (inbox, raw) pair; downstream is the target
    // wiki page. `outcome` + `failed_ids` travel as metadata so the
    // UI can distinguish a full `applied` from `partial_applied`.
    let mut upstream_refs: Vec<wiki_store::provenance::LineageRef> = Vec::new();
    for id in &applied {
        upstream_refs.push(wiki_store::provenance::LineageRef::Inbox { id: *id });
        if let Some(entry) = by_id.get(id) {
            if let Some(rid) = entry.source_raw_id {
                upstream_refs.push(wiki_store::provenance::LineageRef::Raw { id: rid });
            }
        }
    }
    wiki_store::provenance::fire_event(
        paths,
        wiki_store::provenance::LineageEvent {
            event_id: wiki_store::provenance::new_event_id(),
            event_type: wiki_store::provenance::LineageEventType::CombinedWikiPageApplied,
            timestamp_ms: wiki_store::provenance::now_unix_ms(),
            upstream: upstream_refs,
            downstream: vec![wiki_store::provenance::LineageRef::WikiPage {
                slug: target_slug.to_string(),
                title: Some(existing_summary.title.clone()),
            }],
            display_title:
                wiki_store::provenance::display_title_combined_wiki_page_applied(
                    applied.len(),
                    target_slug,
                ),
            metadata: serde_json::json!({
                "outcome": outcome,
                "failed_ids": failed,
                "summary": summary,
            }),
        },
    );

    Ok(CombinedApplyResult {
        outcome,
        target_page_slug: target_slug.to_string(),
        applied_inbox_ids: applied,
        failed_inbox_ids: failed,
        // Echo the composed `verb | title` line so the UI can surface
        // it without re-reading log.md. The leading `## [...]` frame
        // that `append_wiki_log` adds is not included — keeping the
        // string compact for toasts.
        audit_entry: format!("update-concept-combined | {log_title}"),
    })
}

/// Parse a merge-step LLM response into `(after_markdown, summary)`.
/// Tolerates code fences the same way `parse_proposal` does.
fn parse_merge_response(raw: &str) -> Result<(String, String)> {
    let payload = strip_code_fences(raw);

    #[derive(Debug, Deserialize)]
    struct MergeResp {
        after_markdown: String,
        summary: String,
    }

    let parsed: MergeResp = serde_json::from_str(payload).map_err(|e| {
        MaintainerError::BadJson {
            reason: e.to_string(),
            preview: payload.chars().take(512).collect(),
        }
    })?;

    if parsed.after_markdown.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "merge response after_markdown is empty".to_string(),
        ));
    }
    if parsed.summary.trim().is_empty() {
        return Err(MaintainerError::InvalidProposal(
            "merge response summary is empty".to_string(),
        ));
    }

    Ok((parsed.after_markdown, parsed.summary))
}

// ─────────────────────────────────────────────────────────────────────
// v2: absorb_batch types + function  (technical-design.md §4.2.2–4.2.3)
// ─────────────────────────────────────────────────────────────────────

/// Progress event sent per-entry during [`absorb_batch`].
///
/// Wire shape (via `DesktopSessionEvent::AbsorbProgress` in
/// `desktop-core`): `{"type":"absorb_progress","task_id":"absorb-...",
/// "processed":2,"total":5,"current_entry_id":3,"action":"create",
/// "page_slug":"...","page_title":"...","error":null}` per
/// `technical-design.md §2.1` SSE Progress Event.
///
/// `task_id` disambiguates concurrent absorb streams when multiple
/// sessions subscribe to the same SSE fan-out. Minted by the HTTP
/// handler via `TaskManager::register("absorb")`, plumbed into
/// `absorb_batch` as a signature parameter, and stamped on every
/// event this loop emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AbsorbProgressEvent {
    pub task_id: String,
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
    task_id: String,
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
                    task_id: task_id.clone(),
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
                        task_id: task_id.clone(),
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
                        task_id: task_id.clone(),
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
                        task_id: task_id.clone(),
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
                        task_id: task_id.clone(),
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
                task_id: task_id.clone(),
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

// ─────────────────────────────────────────────────────────────────────
// Q2: Target Resolver — suggest target wiki pages for an inbox entry
// ─────────────────────────────────────────────────────────────────────
//
// Pure, synchronous scorer. Given an `InboxEntry` (freshly-ingested
// raw paste / URL) and the full list of wiki pages, return up to
// three `TargetCandidate` records ranked by a transparent 8-signal
// additive scoring scheme. Called by the HTTP handler
// `GET /api/wiki/inbox/{id}/candidates` before the user picks
// between `create_new` and `update_existing { target_page_slug }`.
//
// Why a pure function:
//   * Deterministic — same inputs always give the same output, so
//     the UI can cache and the audit log can replay.
//   * No I/O — tests run in microseconds; `resolve_target_candidates`
//     never touches the LLM, the network, or the filesystem.
//   * Composable — a future Q3 pass can layer vector similarity or
//     LLM re-ranking on top by simply post-processing the output.
//
// The scoring pipeline does NOT mutate any inbox or wiki state. It
// does NOT call `resolve_inbox_entry`, `propose_update`, or
// `apply_update_proposal`. Read-only by construction.

/// Confidence tier assigned to a candidate based on its final score.
/// Used by the UI to group hits into "Strong / Likely / Weak"
/// sections in the Workbench target picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateTier {
    /// score ≥ 80 — user can "one-click accept".
    Strong,
    /// 40 ≤ score < 80 — worth reviewing but not auto-accepting.
    Likely,
    /// 10 ≤ score < 40 — shown in an expandable "more options" group.
    Weak,
}

/// Provenance of a candidate: did it come from existing inbox state
/// (already resolved / already proposed) or from the Q2 scorer?
/// Drives which reason chips the UI renders and whether the "accept"
/// button short-circuits to the confirmed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    /// `inbox.target_page_slug` is already set — locked in via
    /// `maintain_action = "update_existing"`. Always tier=Strong.
    ExistingTarget,
    /// `inbox.proposed_wiki_slug` is set (maintainer proposal), but
    /// `target_page_slug` is not yet committed. Tier=Strong, score
    /// slightly lower than `ExistingTarget` to leave headroom.
    ExistingProposed,
    /// Produced by the 8-signal scorer in this module.
    Resolved,
}

/// One human-readable reason the scorer emitted for a candidate.
/// The frontend shows the `detail` string verbatim ( ≤ 50 chars,
/// Chinese copy) and uses `code` + `weight` for sorting / analytics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateReason {
    /// Stable machine id (e.g. `"exact_slug"`, `"title_overlap_high"`).
    /// Consumers can compile-match on this.
    pub code: String,
    /// How many points this reason contributed to the candidate's
    /// total score.
    pub weight: u32,
    /// Chinese short phrase ≤ 50 chars shown verbatim in the UI.
    pub detail: String,
}

/// One ranked target-page suggestion for an inbox entry.
/// Top-3 of these are returned by `resolve_target_candidates`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetCandidate {
    /// Slug of the proposed target page.
    pub slug: String,
    /// Display title, copied from the wiki page frontmatter.
    pub title: String,
    /// Sum of all reason weights. Used for tier assignment + sort.
    pub score: u32,
    /// Confidence tier derived from `score`.
    pub tier: CandidateTier,
    /// Provenance flag — see [`CandidateSource`].
    pub source: CandidateSource,
    /// Top-3 strongest reasons, highest `weight` first.
    pub reasons: Vec<CandidateReason>,
}

/// Stopwords used by [`tokenize_for_scoring`]. Single-token terms
/// that add noise to the Jaccard signal. Initial 20-word list spans
/// English function words + a handful of high-frequency Chinese
/// particles. Intentionally small — the scorer is additive, so
/// over-aggressive stopword removal would hurt precision. Kept as
/// a module-private constant so callers can't mutate it.
const STOPWORDS: &[&str] = &[
    // English function words (13)
    "a", "an", "the", "and", "or", "is", "are", "of", "in", "to", "for", "on", "at",
    // Chinese high-frequency particles (7)
    "的", "了", "是", "和", "在", "有", "这",
];

/// Lower-cased + trimmed, nothing else. The Q2 contract deliberately
/// keeps normalization simple: `to_lowercase` handles ASCII and
/// most CJK-neutral text, `trim` strips incidental whitespace.
/// Callers that need accent-folding can layer it on top.
fn normalize(s: &str) -> String {
    s.to_lowercase().trim().to_string()
}

/// Split a string into deduplicated tokens for Jaccard scoring.
///
/// Pipeline:
///   1. Lowercase.
///   2. Split on `[\s\-_/]+` (whitespace, dash, underscore, slash).
///   3. Keep tokens with `len() >= 2` (counted in chars, not bytes,
///      so one CJK glyph is already ≥ 2 by the char count convention
///      we use — a CJK char is one `char`, so `len() >= 2` means
///      "at least two characters", which is fine for both alphabets).
///   4. Drop tokens that appear in `STOPWORDS`.
///
/// Returns a `HashSet<String>` so downstream Jaccard can do
/// intersection / union in linear time with zero duplicates.
fn tokenize_for_scoring(s: &str) -> HashSet<String> {
    let lowered = s.to_lowercase();
    lowered
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '/')
        .filter(|t| !t.is_empty())
        // Use char count, not byte len: "你" has byte_len=3 but should
        // count as one character for the "≥ 2" gate. Since chars()
        // iterates Unicode scalar values, this is the right primitive.
        .filter(|t| t.chars().count() >= 2)
        .filter(|t| !STOPWORDS.contains(t))
        .map(|t| t.to_string())
        .collect()
}

/// Jaccard similarity between two token sets. Returns 0.0 when
/// either set is empty (vacuous overlap). Pure, no I/O.
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    let inter = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}

/// Slug-ify a title the same way the inbox / raw layer does so that
/// `exact_slug` can compare apples to apples. We don't call
/// `wiki_store::slugify` directly because it's a Critical write-path
/// helper — repurposing it for a read-only scorer pulls it into the
/// data-loss blast radius. Instead we do the lightweight subset
/// `resolve_target_candidates` actually needs: lowercase, replace
/// whitespace/underscore/slash with `-`, strip anything that isn't
/// alphanumeric or `-`, collapse runs of `-`, trim leading/trailing
/// `-`. Inputs identical modulo this transform produce identical
/// outputs.
fn derive_slug_for_scoring(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut last_was_dash = false;
    for c in lowered.chars() {
        let replaced = if c.is_whitespace() || c == '_' || c == '/' || c == '-' {
            Some('-')
        } else if c.is_ascii_alphanumeric() {
            Some(c)
        } else {
            // Keep non-ASCII alphanumerics (CJK titles → `机器学习`),
            // since the scorer wants a direct compare, not a URL-safe
            // slug. Drop ASCII punctuation entirely.
            if c.is_alphanumeric() {
                Some(c)
            } else {
                None
            }
        };
        if let Some(ch) = replaced {
            if ch == '-' {
                if last_was_dash {
                    continue;
                }
                last_was_dash = true;
            } else {
                last_was_dash = false;
            }
            out.push(ch);
        }
    }
    out.trim_matches('-').to_string()
}

/// Convert a score to a confidence tier. Thresholds match the Q2
/// contract: ≥ 80 strong, ≥ 40 likely, ≥ 10 weak, < 10 dropped by
/// the caller.
fn tier_for_score(score: u32) -> CandidateTier {
    if score >= 80 {
        CandidateTier::Strong
    } else if score >= 40 {
        CandidateTier::Likely
    } else {
        CandidateTier::Weak
    }
}

/// Score a single wiki page as a target candidate for an inbox
/// entry. Returns the score + the list of reasons that contributed,
/// NOT yet truncated to top-3 (caller does that after ranking so
/// the cutoff is applied to the final set, not per-page).
fn score_page_against_inbox(
    inbox_entry: &wiki_store::InboxEntry,
    page: &wiki_store::PageSummaryForResolver,
) -> (u32, Vec<CandidateReason>) {
    let mut reasons: Vec<CandidateReason> = Vec::new();
    let mut score: u32 = 0;

    // Signal 1: exact_slug (+100)
    // Derive a slug from the inbox title the same way the raw layer
    // does and compare to the wiki page's slug.
    let derived_slug = derive_slug_for_scoring(&inbox_entry.title);
    if !derived_slug.is_empty() && derived_slug == page.slug {
        score += 100;
        reasons.push(CandidateReason {
            code: "exact_slug".to_string(),
            weight: 100,
            detail: "标题推导 slug 与 wiki 完全一致".to_string(),
        });
    }

    // Signal 2: exact_title (+80)
    // Normalize() both sides, byte-equal.
    let n_inbox = normalize(&inbox_entry.title);
    let n_page = normalize(&page.title);
    if !n_inbox.is_empty() && n_inbox == n_page {
        score += 80;
        reasons.push(CandidateReason {
            code: "exact_title".to_string(),
            weight: 80,
            detail: "标题文本完全一致".to_string(),
        });
    }

    // Signals 3–5: title_overlap_{high,mid,low} — Jaccard buckets.
    let toks_inbox = tokenize_for_scoring(&inbox_entry.title);
    let toks_page = tokenize_for_scoring(&page.title);
    let j = jaccard(&toks_inbox, &toks_page);
    if j >= 0.7 {
        score += 60;
        reasons.push(CandidateReason {
            code: "title_overlap_high".to_string(),
            weight: 60,
            detail: format!("标题词重合度高 (Jaccard {:.2})", j),
        });
    } else if j >= 0.5 {
        score += 40;
        reasons.push(CandidateReason {
            code: "title_overlap_mid".to_string(),
            weight: 40,
            detail: format!("标题词重合度中 (Jaccard {:.2})", j),
        });
    } else if j >= 0.3 {
        score += 20;
        reasons.push(CandidateReason {
            code: "title_overlap_low".to_string(),
            weight: 20,
            detail: format!("标题词重合度低 (Jaccard {:.2})", j),
        });
    }

    // Signal 6: shared_raw_source (+50) — cheap: both inbox and
    // page know their source_raw_id in-memory. No extra I/O.
    if let (Some(inbox_raw), Some(page_raw)) =
        (inbox_entry.source_raw_id, page.source_raw_id)
    {
        if inbox_raw == page_raw {
            score += 50;
            reasons.push(CandidateReason {
                code: "shared_raw_source".to_string(),
                weight: 50,
                detail: format!("来源同一 raw #{inbox_raw}"),
            });
        }
    }
    // NOTE: This covers the "wiki was generated from the same raw_id
    // this inbox points at" case (the strongest cheap signal). The
    // richer "wiki was generated from a sibling raw in the same
    // source-cluster" case requires a raw-graph lookup and is
    // out-of-scope for Q2 MVP. See TODO below.

    // Signals 7–9 (graph_*) are added by
    // `apply_graph_signals_in_place` in a second pass over the top
    // preliminary hits — see `resolve_target_candidates` below.

    (score, reasons)
}

/// Second-pass enrichment: given a slice of (page-index, score,
/// reasons), look at each top preliminary hit's graph (backlinks /
/// related / outgoing) and add graph_* signals when OTHER top hits
/// appear in that graph. Runs only when the caller provides a graph
/// map — the handler passes `Some(...)` when the client requested
/// `?with_graph=true` and `None` for the default fast path.
fn apply_graph_signals_in_place(
    hits: &mut [(usize, u32, Vec<CandidateReason>)],
    pages: &[wiki_store::PageSummaryForResolver],
    page_graphs: &HashMap<String, wiki_store::PageGraph>,
) {
    // Snapshot the set of top candidate slugs so we can ask "does
    // page B appear in page A's graph?" in O(1).
    let top_slugs: HashSet<&str> = hits
        .iter()
        .map(|(idx, _, _)| pages[*idx].slug.as_str())
        .collect();

    for (idx, score, reasons) in hits.iter_mut() {
        let my_slug = pages[*idx].slug.as_str();
        let Some(graph) = page_graphs.get(my_slug) else {
            continue;
        };

        // graph_backlink (+25): some OTHER top hit links into me.
        if graph
            .backlinks
            .iter()
            .any(|n| n.slug != my_slug && top_slugs.contains(n.slug.as_str()))
        {
            *score += 25;
            reasons.push(CandidateReason {
                code: "graph_backlink".to_string(),
                weight: 25,
                detail: "同组候选反向链接至此页".to_string(),
            });
        }

        // graph_related (+15): some OTHER top hit is in my
        // `related` section (shared-signal adjacency).
        if graph
            .related
            .iter()
            .any(|r| r.slug != my_slug && top_slugs.contains(r.slug.as_str()))
        {
            *score += 15;
            reasons.push(CandidateReason {
                code: "graph_related".to_string(),
                weight: 15,
                detail: "与同组候选页相关联".to_string(),
            });
        }

        // graph_outgoing (+10): I link out to another top hit.
        if graph
            .outgoing
            .iter()
            .any(|n| n.slug != my_slug && top_slugs.contains(n.slug.as_str()))
        {
            *score += 10;
            reasons.push(CandidateReason {
                code: "graph_outgoing".to_string(),
                weight: 10,
                detail: "链出至同组候选页".to_string(),
            });
        }
    }
}

/// Q2 Target Resolver — pure scoring pipeline.
///
/// Returns up to three ranked `TargetCandidate` records for an inbox
/// entry, driven by the 8-signal additive scoring scheme documented
/// in the Q2 contract.
///
/// Short-circuits when the inbox entry has already locked in a target:
///   * `target_page_slug` present → single `ExistingTarget` hit,
///     score forced to 100, tier=Strong, single `existing_target`
///     reason chip.
///   * `proposed_wiki_slug` present (and no `target_page_slug`)
///     → single `ExistingProposed` hit, score forced to 90,
///     tier=Strong.
///
/// Otherwise (cold path):
///   1. Score every page with `score_page_against_inbox`.
///   2. Drop pages with `score < 10`.
///   3. Sort descending by score.
///   4. Take top-3 (NOTE: taken BEFORE graph signals, so graph
///      enrichment runs over the final set; keeps the O(N·M) graph
///      cost bounded at 3 · 3 = 9 set-contains lookups).
///   5. If `page_graphs` is `Some`, apply graph_* signals and
///      re-sort — the extra weight can shuffle the top-3 order.
///   6. Cap each candidate's reasons at top-3 by weight.
///
/// `page_graphs` is keyed by slug. Callers that don't need graph
/// enrichment should pass `None` to skip that pass (the HTTP
/// handler passes `Some(...)` only when `?with_graph=true`).
pub fn resolve_target_candidates(
    inbox_entry: &wiki_store::InboxEntry,
    pages: &[wiki_store::PageSummaryForResolver],
    page_graphs: Option<&HashMap<String, wiki_store::PageGraph>>,
) -> Vec<TargetCandidate> {
    // ── Fast paths: pre-existing target / proposal ───────────────
    if let Some(target_slug) = inbox_entry.target_page_slug.as_ref() {
        if let Some(page) = pages.iter().find(|p| &p.slug == target_slug) {
            return vec![TargetCandidate {
                slug: page.slug.clone(),
                title: page.title.clone(),
                score: 100,
                tier: CandidateTier::Strong,
                source: CandidateSource::ExistingTarget,
                reasons: vec![CandidateReason {
                    code: "existing_target".to_string(),
                    weight: 100,
                    detail: "已关联 wiki 页".to_string(),
                }],
            }];
        }
        // Fall through when the slug points at a deleted page —
        // let the cold path compute real candidates.
    }
    if inbox_entry.target_page_slug.is_none() {
        if let Some(prop_slug) = inbox_entry.proposed_wiki_slug.as_ref() {
            if let Some(page) = pages.iter().find(|p| &p.slug == prop_slug) {
                return vec![TargetCandidate {
                    slug: page.slug.clone(),
                    title: page.title.clone(),
                    score: 90,
                    tier: CandidateTier::Strong,
                    source: CandidateSource::ExistingProposed,
                    reasons: vec![CandidateReason {
                        code: "existing_proposed".to_string(),
                        weight: 90,
                        detail: "已存在同名提案页".to_string(),
                    }],
                }];
            }
            // Fall through when the proposal page doesn't exist yet —
            // the resolver can still suggest other targets.
        }
    }

    // ── Cold path: 8-signal scorer ───────────────────────────────
    let mut scored: Vec<(usize, u32, Vec<CandidateReason>)> = pages
        .iter()
        .enumerate()
        .map(|(idx, page)| {
            let (score, reasons) = score_page_against_inbox(inbox_entry, page);
            (idx, score, reasons)
        })
        .filter(|(_, score, _)| *score >= 10)
        .collect();

    // Sort descending by score.
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.truncate(3);

    // Optional graph enrichment over the top-3.
    if let Some(graphs) = page_graphs {
        apply_graph_signals_in_place(&mut scored, pages, graphs);
        scored.sort_by(|a, b| b.1.cmp(&a.1));
    }

    scored
        .into_iter()
        .map(|(idx, score, mut reasons)| {
            // Top-3 reasons by weight desc, stable tie-break on code.
            reasons.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.code.cmp(&b.code)));
            reasons.truncate(3);
            TargetCandidate {
                slug: pages[idx].slug.clone(),
                title: pages[idx].title.clone(),
                score,
                tier: tier_for_score(score),
                source: CandidateSource::Resolved,
                reasons,
            }
        })
        .collect()
}

// TODO (Q3): shared_raw_source currently matches only on the exact
// `page.source_raw_id == inbox.source_raw_id` case. A future pass
// should widen it to "the wiki page was generated from a raw that
// shares a source-cluster (e.g. same canonical URL) with this
// inbox's raw". Requires a raw-graph adjacency lookup in
// `wiki_store`. Tracked for Q3 scope.
//
// TODO (Q3): consider adding a "recent_touch" signal that boosts
// pages the user has touched in the last N days. Needs a signal
// source from `wiki/log.md` — out of scope for Q2 MVP.

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

        let result = absorb_batch(
            &paths,
            vec![id1, id2, id3],
            &broker,
            tx,
            "test-happy-path".to_string(),
            cancel,
        )
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

        let result = absorb_batch(
            &paths,
            vec![id1],
            &broker,
            tx,
            "test-skips-already-absorbed".to_string(),
            cancel,
        )
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

        let result = absorb_batch(
            &paths,
            vec![id1, id2],
            &broker,
            tx,
            "test-cancellation".to_string(),
            cancel,
        )
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

        let result = absorb_batch(
            &paths,
            vec![id1],
            &broker,
            tx,
            "test-llm-failure".to_string(),
            cancel,
        )
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

    // ── W1 Maintainer Workbench: execute_maintain tests ───────────

    /// Helper: seed a raw entry + its NewRaw inbox task.
    /// Returns `(inbox_id, raw_id)`.
    fn seed_raw_with_inbox(paths: &wiki_store::WikiPaths, body: &str) -> (u32, u32) {
        let raw_id = seed_raw(paths, body);
        let (raw_entry, _body) = wiki_store::read_raw_entry(paths, raw_id).unwrap();
        let inbox_entry =
            wiki_store::append_new_raw_task(paths, &raw_entry, "test-seed").unwrap();
        (inbox_entry.id, raw_id)
    }

    #[tokio::test]
    async fn execute_maintain_create_new_writes_page_and_patches_inbox() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let (inbox_id, raw_id) = seed_raw_with_inbox(&paths, "Transformer architecture notes.");

        let canned = make_proposal_json("transformer", "Transformer", raw_id);
        let broker = MockBrokerSender { canned };

        let outcome = execute_maintain(&paths, inbox_id, MaintainAction::CreateNew, &broker)
            .await
            .unwrap();

        match outcome {
            MaintainOutcome::Created { target_page_slug } => {
                assert_eq!(target_page_slug, "transformer");
            }
            other => panic!("expected Created, got {other:?}"),
        }

        // Wiki page written.
        assert!(wiki_store::read_wiki_page(&paths, "transformer").is_ok());

        // Inbox patched.
        let entries = wiki_store::list_inbox_entries(&paths).unwrap();
        let entry = entries.iter().find(|e| e.id == inbox_id).unwrap();
        assert_eq!(entry.status, wiki_store::InboxStatus::Approved);
        assert_eq!(entry.maintain_action.as_deref(), Some("create_new"));
        assert_eq!(entry.target_page_slug.as_deref(), Some("transformer"));
        assert_eq!(entry.proposed_wiki_slug.as_deref(), Some("transformer"));
        assert!(entry.rejection_reason.is_none());
    }

    #[tokio::test]
    async fn execute_maintain_update_existing_appends_under_dated_heading() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        // Pre-existing target page.
        wiki_store::write_wiki_page(
            &paths,
            "attention",
            "Attention Mechanism",
            "Summary of attention.",
            "# Attention\n\nOriginal body.",
            None,
        )
        .unwrap();

        let (inbox_id, _raw_id) = seed_raw_with_inbox(&paths, "New info about attention.");
        let broker = MockBrokerSender {
            canned: "unused".to_string(),
        };

        let outcome = execute_maintain(
            &paths,
            inbox_id,
            MaintainAction::UpdateExisting {
                target_page_slug: "attention".to_string(),
            },
            &broker,
        )
        .await
        .unwrap();

        match outcome {
            MaintainOutcome::Updated { target_page_slug } => {
                assert_eq!(target_page_slug, "attention");
            }
            other => panic!("expected Updated, got {other:?}"),
        }

        // Page body now has both the original content and a dated update heading.
        let (_summary, body) = wiki_store::read_wiki_page(&paths, "attention").unwrap();
        assert!(body.contains("Original body."));
        assert!(body.contains("## 更新 ["), "should have dated update heading");

        // Inbox patched.
        let entries = wiki_store::list_inbox_entries(&paths).unwrap();
        let entry = entries.iter().find(|e| e.id == inbox_id).unwrap();
        assert_eq!(entry.status, wiki_store::InboxStatus::Approved);
        assert_eq!(entry.maintain_action.as_deref(), Some("update_existing"));
        assert_eq!(entry.target_page_slug.as_deref(), Some("attention"));
    }

    #[tokio::test]
    async fn execute_maintain_reject_patches_inbox_and_logs() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let (inbox_id, _raw_id) = seed_raw_with_inbox(&paths, "Spammy content.");
        let broker = MockBrokerSender {
            canned: "unused".to_string(),
        };

        let outcome = execute_maintain(
            &paths,
            inbox_id,
            MaintainAction::Reject {
                reason: "low quality — off-topic".to_string(),
            },
            &broker,
        )
        .await
        .unwrap();

        match outcome {
            MaintainOutcome::Rejected { reason } => {
                assert!(reason.contains("low quality"));
            }
            other => panic!("expected Rejected, got {other:?}"),
        }

        // Inbox patched.
        let entries = wiki_store::list_inbox_entries(&paths).unwrap();
        let entry = entries.iter().find(|e| e.id == inbox_id).unwrap();
        assert_eq!(entry.status, wiki_store::InboxStatus::Rejected);
        assert_eq!(entry.maintain_action.as_deref(), Some("reject"));
        assert!(entry
            .rejection_reason
            .as_deref()
            .unwrap_or("")
            .contains("low quality"));

        // Log line appended.
        let log_path = wiki_store::wiki_log_path(&paths);
        let log = std::fs::read_to_string(&log_path).unwrap();
        assert!(log.contains("reject-inbox"), "log should carry reject-inbox verb");
        assert!(log.contains(&format!("inbox/{inbox_id}")));
    }

    #[test]
    fn inbox_entry_backward_compat_roundtrip() {
        // A pre-W1 inbox.json blob (no maintain fields present) must
        // deserialize cleanly with `#[serde(default)]` filling in None.
        let old_json = r#"[
            {
                "id": 1,
                "kind": "new-raw",
                "status": "pending",
                "title": "old entry",
                "description": "no W1 fields",
                "created_at": "2026-04-15T00:00:00Z"
            }
        ]"#;
        let parsed: Vec<wiki_store::InboxEntry> = serde_json::from_str(old_json).unwrap();
        assert_eq!(parsed.len(), 1);
        let entry = &parsed[0];
        assert_eq!(entry.id, 1);
        assert!(entry.maintain_action.is_none());
        assert!(entry.target_page_slug.is_none());
        assert!(entry.proposed_wiki_slug.is_none());
        assert!(entry.rejection_reason.is_none());
    }

    // ── W2 Proposal / Apply two-phase update tests ────────────────

    /// Canned broker that returns a hand-crafted merge response. We
    /// reuse `MockBrokerSender` instead of rolling a new mock because
    /// the propose_update path has exactly one broker call — the
    /// same "single canned response" shape as create_new.
    fn canned_merge_response(after: &str, summary: &str) -> String {
        serde_json::json!({
            "after_markdown": after,
            "summary": summary,
        })
        .to_string()
    }

    #[tokio::test]
    async fn propose_update_generates_before_after() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        // Seed a target page and a raw entry pointing to an inbox task.
        wiki_store::write_wiki_page(
            &paths,
            "attention",
            "Attention",
            "Summary.",
            "# Attention\n\nOriginal body.",
            None,
        )
        .unwrap();
        let (inbox_id, _raw_id) =
            seed_raw_with_inbox(&paths, "New insight: attention can be multi-head.");

        let broker = MockBrokerSender {
            canned: canned_merge_response(
                "# Attention\n\nOriginal body.\n\n## 多头注意力\n\n新素材：可以并行处理。",
                "在原文后追加多头注意力小节",
            ),
        };

        let proposal = propose_update(&paths, inbox_id, "attention", &broker)
            .await
            .unwrap();

        assert_eq!(proposal.target_slug, "attention");
        assert!(
            proposal.before_markdown.contains("Original body."),
            "before_markdown should hold the pre-merge body"
        );
        assert!(
            proposal.after_markdown.contains("多头注意力"),
            "after_markdown should hold the LLM-merged body"
        );
        assert!(
            proposal.before_markdown != proposal.after_markdown,
            "before != after: LLM actually produced a change"
        );
        assert!(proposal.summary.contains("追加"));
        assert!(proposal.generated_at > 0);

        // Inbox entry persists the proposal.
        let entries = wiki_store::list_inbox_entries(&paths).unwrap();
        let entry = entries.iter().find(|e| e.id == inbox_id).unwrap();
        assert_eq!(entry.proposal_status.as_deref(), Some("pending"));
        assert_eq!(
            entry.proposed_after_markdown.as_deref(),
            Some(proposal.after_markdown.as_str())
        );
        assert_eq!(
            entry.before_markdown_snapshot.as_deref(),
            Some(proposal.before_markdown.as_str())
        );
        assert_eq!(
            entry.proposal_summary.as_deref(),
            Some(proposal.summary.as_str())
        );
        // W1 bookkeeping also stamped so a page refresh knows the target.
        assert_eq!(entry.maintain_action.as_deref(), Some("update_existing"));
        assert_eq!(entry.target_page_slug.as_deref(), Some("attention"));
        // Status still pending — apply hasn't been called yet.
        assert_eq!(entry.status, wiki_store::InboxStatus::Pending);
    }

    #[tokio::test]
    async fn apply_update_proposal_writes_page_and_updates_inbox() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        wiki_store::write_wiki_page(
            &paths,
            "dropout",
            "Dropout",
            "Summary.",
            "# Dropout\n\nBefore content.",
            None,
        )
        .unwrap();
        let (inbox_id, _raw_id) = seed_raw_with_inbox(&paths, "Dropout details.");

        // Phase 1: propose.
        let merged = "# Dropout\n\nBefore content.\n\n## Extra\n\nmerged.";
        let broker = MockBrokerSender {
            canned: canned_merge_response(merged, "追加 Extra 小节"),
        };
        let _proposal = propose_update(&paths, inbox_id, "dropout", &broker)
            .await
            .unwrap();

        // Phase 2: apply.
        let outcome = apply_update_proposal(&paths, inbox_id).unwrap();
        match outcome {
            MaintainOutcome::Updated { target_page_slug } => {
                assert_eq!(target_page_slug, "dropout");
            }
            other => panic!("expected Updated, got {other:?}"),
        }

        // Page on disk has the merged body.
        let (_summary, body) = wiki_store::read_wiki_page(&paths, "dropout").unwrap();
        assert!(body.contains("## Extra"));
        assert!(body.contains("merged."));

        // Inbox entry: proposal_status=applied, status=Approved, staged
        // markdown cleared, summary retained for audit.
        let entries = wiki_store::list_inbox_entries(&paths).unwrap();
        let entry = entries.iter().find(|e| e.id == inbox_id).unwrap();
        assert_eq!(entry.status, wiki_store::InboxStatus::Approved);
        assert_eq!(entry.proposal_status.as_deref(), Some("applied"));
        assert!(entry.proposed_after_markdown.is_none(),
            "proposed_after_markdown must be cleared on apply");
        assert!(entry.before_markdown_snapshot.is_none(),
            "before_markdown_snapshot must be cleared on apply");
        assert_eq!(entry.proposal_summary.as_deref(), Some("追加 Extra 小节"),
            "proposal_summary retained for audit");
        assert_eq!(entry.maintain_action.as_deref(), Some("update_existing"));
        assert_eq!(entry.target_page_slug.as_deref(), Some("dropout"));
        assert!(entry.resolved_at.is_some(), "resolved_at stamped on apply");

        // Audit log has update-concept line.
        let log_path = wiki_store::wiki_log_path(&paths);
        let log = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(log.contains("update-concept"));
    }

    #[tokio::test]
    async fn cancel_update_proposal_clears_fields() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        wiki_store::write_wiki_page(
            &paths,
            "relu",
            "ReLU",
            "Summary.",
            "# ReLU\n\nBefore.",
            None,
        )
        .unwrap();
        let (inbox_id, _raw_id) = seed_raw_with_inbox(&paths, "ReLU insight.");

        let broker = MockBrokerSender {
            canned: canned_merge_response("# ReLU\n\nMerged.", "改写整段"),
        };
        propose_update(&paths, inbox_id, "relu", &broker).await.unwrap();

        // Cancel — should clear the staged markdown but keep the inbox pending.
        cancel_update_proposal(&paths, inbox_id).unwrap();

        let entries = wiki_store::list_inbox_entries(&paths).unwrap();
        let entry = entries.iter().find(|e| e.id == inbox_id).unwrap();
        assert_eq!(entry.proposal_status.as_deref(), Some("cancelled"));
        assert!(entry.proposed_after_markdown.is_none());
        assert!(entry.before_markdown_snapshot.is_none());
        // Summary retained so UI can show "last proposal said…".
        assert_eq!(entry.proposal_summary.as_deref(), Some("改写整段"));
        // Status stays Pending — user can re-propose without restart.
        assert_eq!(entry.status, wiki_store::InboxStatus::Pending);
        // maintain_action / target_page_slug preserved for re-propose.
        assert_eq!(entry.maintain_action.as_deref(), Some("update_existing"));
        assert_eq!(entry.target_page_slug.as_deref(), Some("relu"));
    }

    #[test]
    fn apply_without_pending_proposal_returns_error() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        // Seed an inbox entry that has NEVER been proposed against.
        let (inbox_id, _raw_id) = seed_raw_with_inbox(&paths, "Body.");

        let err = apply_update_proposal(&paths, inbox_id).unwrap_err();
        match err {
            MaintainerError::InvalidProposal(msg) => {
                assert!(msg.contains("no pending proposal"),
                    "expected 'no pending proposal' in error, got: {msg}");
            }
            other => panic!("expected InvalidProposal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn apply_detects_concurrent_edit_and_refuses() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        wiki_store::write_wiki_page(
            &paths,
            "softmax",
            "Softmax",
            "Summary.",
            "# Softmax\n\nOriginal.",
            None,
        )
        .unwrap();
        let (inbox_id, _raw_id) = seed_raw_with_inbox(&paths, "body");

        let broker = MockBrokerSender {
            canned: canned_merge_response("# Softmax\n\nMerged.", "改写"),
        };
        propose_update(&paths, inbox_id, "softmax", &broker).await.unwrap();

        // Simulate a concurrent external edit: overwrite the page body.
        wiki_store::write_wiki_page(
            &paths,
            "softmax",
            "Softmax",
            "Summary.",
            "# Softmax\n\nExternally edited.",
            None,
        )
        .unwrap();

        // Apply should now fail with a conflict error.
        let err = apply_update_proposal(&paths, inbox_id).unwrap_err();
        match err {
            MaintainerError::Store(msg) => {
                assert!(msg.contains("changed since proposal"), "got: {msg}");
            }
            other => panic!("expected Store conflict error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn propose_update_rejects_malformed_llm_json() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());

        wiki_store::write_wiki_page(
            &paths,
            "loss",
            "Loss",
            "s",
            "# Loss\n\nbody.",
            None,
        )
        .unwrap();
        let (inbox_id, _raw_id) = seed_raw_with_inbox(&paths, "body");

        let broker = MockBrokerSender {
            canned: "this is not JSON at all".to_string(),
        };
        let err = propose_update(&paths, inbox_id, "loss", &broker)
            .await
            .unwrap_err();
        assert!(matches!(err, MaintainerError::BadJson { .. }));

        // Inbox entry must NOT have a pending proposal — propose failed
        // before the persist step.
        let entries = wiki_store::list_inbox_entries(&paths).unwrap();
        let entry = entries.iter().find(|e| e.id == inbox_id).unwrap();
        assert!(entry.proposal_status.is_none());
    }
}

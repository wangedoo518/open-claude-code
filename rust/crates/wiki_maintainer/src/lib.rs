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
    parse_proposal(&raw_json, raw_id)
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
}

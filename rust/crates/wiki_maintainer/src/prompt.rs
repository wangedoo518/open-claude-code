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
3. Quote ≤ 15 consecutive words from the raw source (hard copyright cap).
4. NEVER emit backlinks to non-existent pages.
5. If you cannot produce a confident summary, respond with an object
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
}

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
7. Never use JSON null. Use empty arrays for no conflicts and non-empty
   strings for slug/title/summary/body.
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

// ── Slice E21 — Draft HTML render prompt ────────────────────────
//
// Render a draft markdown document into a single self-contained HTML
// file via one LLM round-trip. Per Thariq's "HTML effectiveness"
// pattern: MD is the canonical editable source, HTML is a regen-
// eratable presentation artifact. The LLM IS the renderer here —
// no template, no CSS framework, the model picks layout and
// typography per target.

/// Slice E21 — system prompt for `render_draft_html`. The target-
/// specific styling sections are non-negotiable contract: handler
/// passes the draft's `target` field into the user message and the
/// LLM picks the matching section.
pub const RENDER_HTML_SYSTEM_PROMPT: &str = r#"You are a presentation renderer. Convert a draft markdown document into a single self-contained HTML file for the user to preview / print / share.

Hard rules (these are non-negotiable):
1. Output ONLY the HTML. No commentary, no markdown fences, no "here is the HTML:" preface. Your entire response must be parseable as a single HTML document.
2. The output must be a complete document: <!DOCTYPE html> through </html>.
3. Use inline <style> tags or style attributes — NO external CSS, NO external JavaScript, NO images from outside the document. The file must work offline.
4. Preserve the visual hierarchy of the source markdown (headings, lists, code blocks, blockquotes, links, emphasis).
5. Stay under 30 KB of HTML output total.
6. Pick reasonable typography, line-height, color contrast — the page must be comfortable to read on a normal monitor.
7. SECURITY: NEVER emit any of the following — they will be stripped server-side and you'll waste tokens:
   - <script> tags of any kind (inline or external)
   - JavaScript event handlers (onclick=, onload=, onerror=, etc.)
   - javascript: URLs in href or src attributes
   - <iframe>, <object>, <embed>, <form> tags
   The output must be 100% static markup + CSS. If the source markdown asks for an interactive element, render it as static text describing the interaction instead.

Target-specific styling:

[xhs] — 小红书 post preview
Render as a phone-frame mockup. Outer page background neutral (light gray). Inner content max-width 375px, centered, with rounded corners + soft shadow that simulates a phone screen. Large hero title (28-32px). Body text 15-16px with generous line-height. Hashtags (#xxx) styled with a soft brand-colored background pill. Mobile-first spacing. The user will screenshot this preview before pasting into the XHS app.

[blog] — Long-form blog post
Single readable column, max-width 720px, centered. Generous line-height (1.7+) and clear section spacing. Code blocks with monospace + light background + small padding. Blockquotes with a left-border accent. If there are 3 or more H2 headings, include a small table of contents at the top.

[wechat] — 微信公众号 article
Max-width 600px. Spacing and font sizing similar to native 公众号 articles. Blockquotes with a left-border accent in a muted color. Add minimal vertical padding between paragraphs (公众号 reads tighter than blog).

[other] — Generic clean document
Max-width 800px. Sans-serif body, simple navigation, minimal styling. Default to a clean "looks like a Google Doc" feel.

If the target value is unrecognised, fall back to the [other] styling.
"#;

/// Slice E21 — input character cap for the draft body before it
/// enters the render prompt. Mirrors the absorb truncation pattern
/// (oversize bodies → clipped). Drafts ≤ 10 KB cover every realistic
/// XHS / blog / 微信 / 长文 length.
pub const RENDER_HTML_MAX_INPUT_BYTES: usize = 10_000;

/// Slice E21 — output token cap. HTML with inline styles is verbose:
/// a single self-contained doc with body content + a styled phone-
/// frame template easily reaches 4-8K tokens. 12K gives headroom so
/// the LLM doesn't truncate mid-`</html>`. Truncation is also
/// defended against post-render via `looks_like_complete_html` in
/// the maintainer crate (review G).
pub const RENDER_HTML_MAX_OUTPUT_TOKENS: u32 = 12_000;

/// Slice E21 — build a render request from a draft's metadata + body.
/// The user message embeds slug / title / target so the LLM can echo
/// or namespace if needed; the body is char-clipped to bound cost.
pub fn build_render_html_request(
    slug: &str,
    title: &str,
    target: &str,
    body: &str,
) -> MessageRequest {
    let truncated_body = if body.len() > RENDER_HTML_MAX_INPUT_BYTES {
        // Char-boundary safe — find the last char boundary at or
        // before the cap so we don't slice mid-codepoint on CJK.
        let mut end = RENDER_HTML_MAX_INPUT_BYTES;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        &body[..end]
    } else {
        body
    };
    let user_text = format!(
        "Render this draft as a single self-contained HTML document.\n\
         \n\
         slug: {slug}\n\
         title: {title}\n\
         target: {target}\n\
         \n\
         --- markdown body ---\n\
         {truncated_body}\n\
         --- end body ---\n\
         \n\
         Output ONLY the HTML. Pick the styling section that matches the target.",
    );
    MessageRequest {
        model: MAINTAINER_MODEL.to_string(),
        max_tokens: RENDER_HTML_MAX_OUTPUT_TOKENS,
        system: Some(RENDER_HTML_SYSTEM_PROMPT.to_string()),
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
            source_domain: None,
            inferred_use_domain: None,
            cross_domain_reason: None,
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
            (
                10_u32,
                "Transformer 论文".to_string(),
                "Body ten.".to_string(),
            ),
            (
                11_u32,
                "Attention survey".to_string(),
                "Body eleven.".to_string(),
            ),
            (
                12_u32,
                "Flash attention".to_string(),
                "Body twelve.".to_string(),
            ),
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

    // ── Slice E21 — render_html prompt tests ──────────────────────

    #[test]
    fn build_render_html_request_picks_correct_target_section_for_xhs() {
        let req = build_render_html_request(
            "post-slug",
            "Title",
            "xhs",
            "# hello\n\nbody\n",
        );
        let system = req.system.expect("system prompt");
        // Target-specific section MUST appear so the LLM picks the
        // right styling — this is the contract we lean on.
        assert!(system.contains("[xhs]"), "missing xhs section in:\n{system}");
        assert!(
            system.contains("phone"),
            "xhs hint should mention phone-frame"
        );
        // Generic rules also present.
        assert!(system.contains("Output ONLY"));
        // Other targets are also defined (so the LLM can fall through).
        assert!(system.contains("[blog]"));
        assert!(system.contains("[wechat]"));
        assert!(system.contains("[other]"));
    }

    #[test]
    fn build_render_html_request_includes_metadata_in_user_message() {
        let req = build_render_html_request(
            "post-slug",
            "My Title",
            "blog",
            "# heading\n\nparagraph text\n",
        );
        assert_eq!(req.messages.len(), 1);
        let user_text: String = req.messages[0]
            .content
            .iter()
            .filter_map(|c| match c {
                InputContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(user_text.contains("# heading"));
        assert!(user_text.contains("My Title"));
        assert!(user_text.contains("blog"));
        assert!(user_text.contains("post-slug"));
    }

    #[test]
    fn build_render_html_request_caps_body_to_max_input_chars() {
        let huge = "x".repeat(20_000);
        let req = build_render_html_request("p", "T", "other", &huge);
        let user_text: String = req.messages[0]
            .content
            .iter()
            .filter_map(|c| match c {
                InputContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        // The user message includes the body PLUS some boilerplate;
        // assert the body portion is capped.
        assert!(
            user_text.len() < 12_000,
            "user message should be truncated; got {} chars",
            user_text.len()
        );
    }

    #[test]
    fn build_render_html_request_does_not_split_codepoints_on_truncation() {
        // CJK character "诶" is 3 bytes in UTF-8. If we naïvely
        // slice at exactly 10000 in the middle of one, the &str
        // slice panics. Build a 12000-char body of mostly CJK.
        let huge = "诶".repeat(4000); // 12000 bytes
        // Should NOT panic.
        let _req = build_render_html_request("p", "T", "blog", &huge);
    }
}

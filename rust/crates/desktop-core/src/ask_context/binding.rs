//! A2 — Explicit source binding for Ask turns.
//!
//! A `SourceRef` pins the conversation to an internal artifact (raw
//! entry, wiki page, inbox item). When a binding is present it
//! overrides the A1 mode classifier — the backend treats the bound
//! source as the highest-priority context slice.
//!
//! Lifecycle
//! ─────────
//!   * `bind_source(session_id, source)` — stores the ref on
//!     `SessionMetadata.source_binding`. Subsequent turns on that
//!     session resolve the source body (raw entry / wiki page body /
//!     inbox → raw) and prepend it to the system prompt.
//!   * `clear_source_binding(session_id)` — drops the binding so the
//!     session reverts to the ambient A1 `ContextMode` behaviour.
//!   * Implicit override — binding a new `SourceRef` replaces any
//!     prior one on the same session; no explicit clear is needed.
//!
//! Wire format
//! ───────────
//!   * URL handoff / HTTP bodies use a single flat key string (see
//!     `parse_binding_key`): `"raw:<id>"`, `"wiki:<slug>"`, or
//!     `"inbox:<id>"`. The handoff component resolves that string to
//!     a full `SourceRef` by calling into `wiki_store` for the title.
//!   * JSON over the session HTTP API uses a tagged enum (serde
//!     `#[serde(tag = "kind", rename_all = "snake_case")]`) so the
//!     frontend can `switch (source.kind) { case "raw": ... }`.
//!
//! Token budgeting
//! ───────────────
//! `token_budget_for_source` mirrors the A1 URL-enrich cap (6000)
//! for raw entries so existing behaviour is preserved; wiki pages get
//! a tighter 4000 budget (tend to be more distilled) and inbox items
//! a 3000 budget (typically small summary chunks). `truncate_source_body`
//! operates on byte length (approx 4 bytes per token) and appends a
//! Chinese truncation marker so the LLM knows it saw a partial view.

use serde::{Deserialize, Serialize};

/// Per-turn pointer to an internal source the user explicitly pinned.
///
/// `title` is required for every variant so the UI chip + wire
/// responses don't have to do a second round trip to derive it. The
/// backend ignores `title` during resolve — the body comes from the
/// id/slug lookup — but echoes it back so the frontend can trust it
/// as the canonical display label.
///
/// `serde(tag = "kind", rename_all = "snake_case")` so the wire form
/// is:
///   `{ "kind": "raw", "id": 42, "title": "..." }`
///   `{ "kind": "wiki", "slug": "foo-bar", "title": "..." }`
///   `{ "kind": "inbox", "id": 17, "title": "..." }`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceRef {
    /// Raw-library entry (landed article, pasted content, etc.).
    Raw { id: u32, title: String },
    /// Wiki concept / topic / people / compare page. The slug is the
    /// stem under `wiki/**/<slug>.md` — `wiki_store::read_wiki_page`
    /// walks the category subdirectories to find it.
    Wiki { slug: String, title: String },
    /// Inbox pending / approved item. Resolved via its `source_raw_id`
    /// back to a raw entry body — inbox items are not themselves
    /// bodies, they're tasks that point at raws.
    Inbox { id: u32, title: String },
}

impl SourceRef {
    /// Short display tag for logs + UI chips. Stable string, three
    /// variants only — never a `format!` over user input.
    #[must_use]
    pub fn display_kind(&self) -> &'static str {
        match self {
            Self::Raw { .. } => "raw",
            Self::Wiki { .. } => "wiki",
            Self::Inbox { .. } => "inbox",
        }
    }

    /// Stable key used in URL handoff + log lines. Inverse of
    /// `parse_binding_key`. Format is `<kind>:<id-or-slug>`.
    ///
    /// Examples: `"raw:123"`, `"wiki:foo-bar"`, `"inbox:42"`.
    #[must_use]
    pub fn binding_key(&self) -> String {
        match self {
            Self::Raw { id, .. } => format!("raw:{id}"),
            Self::Wiki { slug, .. } => format!("wiki:{slug}"),
            Self::Inbox { id, .. } => format!("inbox:{id}"),
        }
    }

    /// Unified accessor so call-sites that only need the display
    /// label don't have to re-match on every variant.
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            Self::Raw { title, .. } | Self::Wiki { title, .. } | Self::Inbox { title, .. } => title,
        }
    }
}

/// Persistent wrapper stored on `SessionMetadata`.
///
/// Cleared by an explicit `clear_source_binding` call or by binding a
/// different source (see `DesktopState::bind_source`). `binding_reason`
/// is optional text surfaced on the UI chip — e.g. `"URL handoff"` or
/// `"rebind from raw library"`.
///
/// `bound_at` is unix milliseconds, matching every other timestamp
/// field on `SessionMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSourceBinding {
    pub source: SourceRef,
    pub bound_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_reason: Option<String>,
}

impl SessionSourceBinding {
    /// Construct a fresh binding with the current unix-ms timestamp
    /// stamped on `bound_at`. Callers that need a specific time (tests,
    /// replays) should build the struct literally.
    #[must_use]
    pub fn new(source: SourceRef, reason: Option<String>) -> Self {
        let bound_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            source,
            bound_at,
            binding_reason: reason,
        }
    }
}

/// Parse a wire-format binding key like `"raw:123"` or `"wiki:foo-slug"`
/// coming from a URL handoff or an HTTP body. Returns `None` on
/// malformed input.
///
/// Returns `(kind, id_or_slug)` — the caller is responsible for:
///   * Validating that `kind` is one of `"raw"`, `"wiki"`, `"inbox"`.
///   * Parsing the payload according to the kind (numeric id for
///     raw/inbox, free-form slug for wiki).
///   * Resolving the title via `wiki_store` before building a full
///     `SourceRef`.
///
/// We intentionally keep this a pure `&str` splitter so the handoff
/// component can validate without pulling in `wiki_store`.
///
/// Rejects: empty input, input with no `:`, input where `kind` is not
/// one of the three known tags, and — for the numeric kinds — input
/// where the payload doesn't parse as a `u32`.
#[must_use]
pub fn parse_binding_key(key: &str) -> Option<(&str, &str)> {
    let (kind, rest) = key.split_once(':')?;
    if rest.is_empty() {
        return None;
    }
    match kind {
        "raw" | "inbox" => {
            // Numeric kinds must have a parseable id. We only validate
            // here — the caller still does the final `u32::from_str`
            // conversion on the returned `&str`.
            if rest.parse::<u32>().is_err() {
                return None;
            }
            Some((kind, rest))
        }
        "wiki" => {
            // Slugs are free-form kebab-case; reject obviously bogus
            // inputs (whitespace, control chars) so we don't crash
            // the filesystem lookup downstream.
            if rest.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return None;
            }
            Some((kind, rest))
        }
        _ => None,
    }
}

/// Token budget per source kind. `Raw` matches the A1 URL-enrich cap
/// (`safe_truncate(body, 6000)` in `maybe_enrich_url`) so a bound raw
/// entry gets the same headroom as an inline URL fetch. Wiki pages
/// are typically more distilled; inbox items are even shorter
/// summary chunks — tighter budgets keep the system prompt focused.
#[must_use]
pub fn token_budget_for_source(source: &SourceRef) -> usize {
    match source {
        SourceRef::Raw { .. } => 6000,
        SourceRef::Wiki { .. } => 4000,
        SourceRef::Inbox { .. } => 3000,
    }
}

/// Truncate `body` to roughly `budget_tokens` tokens using the
/// bytes-per-token ≈ 4 heuristic shared with `ask_context::
/// approximate_tokens_from_bytes`. Appends a Chinese truncation
/// marker so the LLM knows it saw a partial slice rather than
/// silently pruning the tail.
///
/// Respects UTF-8 char boundaries — we never split a multi-byte
/// codepoint, which would surface as a replacement glyph in the
/// chat transcript.
#[must_use]
pub fn truncate_source_body(body: &str, budget_tokens: usize) -> String {
    let max_bytes = budget_tokens.saturating_mul(4);
    if body.len() <= max_bytes {
        return body.to_string();
    }
    // Walk back to the nearest UTF-8 char boundary so we don't cut a
    // multi-byte codepoint (matches `DesktopState::safe_truncate`).
    let mut end = max_bytes;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n[...内容已截断...]", &body[..end])
}

/// Prompt-assembly helper — wraps the bound source body with a clear
/// header the LLM can recognise as "this is the user's pinned source".
///
/// Returns a Chinese-language block because every other A1 prompt
/// fragment (`BOUNDARY_MARKER_*`, `format_enriched_source`) is also
/// Chinese. Mixing languages in the system prompt degrades the
/// LLM's ability to follow the instruction.
///
/// Called by `append_user_message` after `truncate_source_body`; the
/// resulting string is prepended to `system_prompt_text` before any
/// A1 boundary marker or enrich block.
///
/// A4 — Grounded Answer Mode. The body is immediately followed by an
/// instruction block (see `grounding_instruction_block`) that asks the
/// LLM to (1) answer only from the bound source, (2) include at least
/// one `> blockquote` of the original text, (3) be explicit when the
/// source does not cover a point, (4) append a `### 依据片段` section
/// listing the quoted fragments, and (5) not fabricate quotes. The
/// instructions come *after* the body so the LLM reads the content
/// first and the rules second — chat models follow instructions that
/// frame already-seen material more reliably than pre-content rules.
#[must_use]
pub fn format_bound_source(source: &SourceRef, body: &str) -> String {
    format!(
        "\n\n## 绑定来源 · {kind} · {title}\n\n\
         请优先基于下方来源内容回答当前问题。\n\n\
         ---\n\n{body}\n\n---\n\
         {rules}",
        kind = source.display_kind(),
        title = source.title(),
        body = body,
        rules = grounding_instruction_block()
    )
}

/// A4 — Grounded Answer Mode instruction block.
///
/// Appended immediately after the bound-source body (inside
/// `format_bound_source`) or, for A3 auto-bound turns where the
/// enriched body is already in the system prompt via `maybe_enrich_url`,
/// tacked on at the end of `system_prompt_text`. In both cases the LLM
/// reads the material before the rules so the instructions frame what
/// has already been seen (more reliable than pre-content guardrails).
///
/// The five numbered rules cover:
///   1. Grounding — answer only from the bound source above.
///   2. Quote anchoring — at least one `> blockquote` of original text.
///   3. Conservative behaviour — say "当前来源未提及 X" instead of
///      filling in from common knowledge.
///   4. Evidence section — append a `### 依据片段` list of quotes.
///   5. No fabrication — never quote sentences that aren't in the
///      source; mark paraphrases explicitly.
///
/// Returns a `&'static str` so call-sites can `push_str` it without
/// allocating. The text is Chinese to match the rest of the prompt
/// pipeline (`BOUNDARY_MARKER_*`, `format_enriched_source`) — mixing
/// languages in one system prompt degrades instruction-following.
#[must_use]
pub fn grounding_instruction_block() -> &'static str {
    "\n\n## Grounded Mode · 回答规则\n\n\
     1. **仅基于上方「绑定来源」回答** —— 不要从对话历史或训练数据补全细节。\n\
     2. **引用原文** —— 回答中**至少用一次 `> blockquote` 引用来源里的原文片段**，让用户能回溯依据。\n\
     3. **保守表达** —— 如果来源不足以回答，明确说「当前来源未提及 X」；绝不凭常识或印象扩写。\n\
     4. **依据片段** —— 在回答末尾加 `### 依据片段` section，列出你引用过的关键片段（每条一个 `> 引号`）。\n\
     5. **不要编造** —— 不要引用来源里不存在的句子；如果是复述而非原文，请显式标注「来源原文改述：」。\n\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All three variants encode with the `snake_case` `kind` tag the
    /// frontend + URL handoff rely on. A silent rename to PascalCase
    /// or dropping the tag would desync every consumer.
    #[test]
    fn source_ref_serializes_tagged_snake_case() {
        let raw = SourceRef::Raw {
            id: 42,
            title: "Some Raw".into(),
        };
        let raw_json = serde_json::to_value(&raw).unwrap();
        assert_eq!(raw_json["kind"], "raw");
        assert_eq!(raw_json["id"], 42);
        assert_eq!(raw_json["title"], "Some Raw");

        let wiki = SourceRef::Wiki {
            slug: "foo-bar".into(),
            title: "Foo Bar".into(),
        };
        let wiki_json = serde_json::to_value(&wiki).unwrap();
        assert_eq!(wiki_json["kind"], "wiki");
        assert_eq!(wiki_json["slug"], "foo-bar");
        assert_eq!(wiki_json["title"], "Foo Bar");

        let inbox = SourceRef::Inbox {
            id: 7,
            title: "Inbox 7".into(),
        };
        let inbox_json = serde_json::to_value(&inbox).unwrap();
        assert_eq!(inbox_json["kind"], "inbox");
        assert_eq!(inbox_json["id"], 7);

        // Round-trip — decode must match the original value.
        let decoded: SourceRef = serde_json::from_value(raw_json).unwrap();
        assert_eq!(decoded, raw);
    }

    /// `binding_key` format is a cross-worker contract: the URL
    /// handoff component relies on this exact string shape. Pin it.
    #[test]
    fn binding_key_format_is_stable() {
        assert_eq!(
            SourceRef::Raw {
                id: 123,
                title: "x".into()
            }
            .binding_key(),
            "raw:123"
        );
        assert_eq!(
            SourceRef::Wiki {
                slug: "foo-slug".into(),
                title: "y".into()
            }
            .binding_key(),
            "wiki:foo-slug"
        );
        assert_eq!(
            SourceRef::Inbox {
                id: 42,
                title: "z".into()
            }
            .binding_key(),
            "inbox:42"
        );
    }

    /// Happy paths — all three kinds parse into the expected
    /// `(kind, payload)` pair. Numeric kinds accept bare ascii digits;
    /// the wiki variant accepts kebab-case slugs.
    #[test]
    fn parse_binding_key_happy_path() {
        assert_eq!(parse_binding_key("raw:123"), Some(("raw", "123")));
        assert_eq!(parse_binding_key("inbox:42"), Some(("inbox", "42")));
        assert_eq!(
            parse_binding_key("wiki:foo-slug"),
            Some(("wiki", "foo-slug"))
        );
        // Unicode slug passes — `wiki_store::read_wiki_page` does the
        // actual filesystem check, so we don't try to be stricter
        // than the store itself.
        assert_eq!(
            parse_binding_key("wiki:some-page-123"),
            Some(("wiki", "some-page-123"))
        );
    }

    /// Malformed inputs must return None — these cover the common
    /// hostile / sloppy shapes we expect on the URL handoff path.
    #[test]
    fn parse_binding_key_rejects_malformed() {
        // Empty
        assert_eq!(parse_binding_key(""), None);
        // No colon
        assert_eq!(parse_binding_key("raw123"), None);
        // Bare colon
        assert_eq!(parse_binding_key("raw:"), None);
        // Unknown kind
        assert_eq!(parse_binding_key("video:123"), None);
        // Numeric kind with non-numeric id
        assert_eq!(parse_binding_key("raw:abc"), None);
        assert_eq!(parse_binding_key("inbox:xyz"), None);
        // Overflow — larger than u32::MAX
        assert_eq!(parse_binding_key("raw:99999999999999"), None);
        // Wiki slug with whitespace
        assert_eq!(parse_binding_key("wiki:foo bar"), None);
    }

    /// Short bodies pass through unchanged; long bodies are truncated
    /// at a char boundary and carry the truncation marker. The marker
    /// is user-visible in the system prompt.
    #[test]
    fn truncate_source_body_respects_budget() {
        // Under budget — no mutation.
        let short = "hello world".to_string();
        assert_eq!(truncate_source_body(&short, 100), short);

        // Over budget — marker appended.
        let long = "a".repeat(10_000);
        let out = truncate_source_body(&long, 100); // budget = 400 bytes
        assert!(out.len() < long.len());
        assert!(
            out.contains("[...内容已截断...]"),
            "truncation marker missing: {out:?}"
        );
        // Truncated body (before the marker) must not exceed the budget.
        let body_part = out.split("\n\n[...内容已截断...]").next().unwrap();
        assert!(body_part.len() <= 400);

        // UTF-8 boundary safety — truncating a stream of multi-byte
        // chars at a mid-codepoint offset must NOT panic and the
        // resulting body must still be valid UTF-8.
        let utf8 = "中".repeat(1_000); // each char is 3 bytes
        let out = truncate_source_body(&utf8, 100); // budget = 400 bytes
                                                    // `contains` forces a UTF-8 scan; if the slice is broken this
                                                    // panics in the stdlib.
        assert!(out.contains("中"));
    }

    /// Back-compat guard — a legacy `SessionMetadata` JSON that
    /// omits the new `source_binding` field must still decode. We
    /// model this the same way the existing `last_context_basis`
    /// legacy test does: wrap a minimal struct with `#[serde(default)]`.
    #[test]
    fn legacy_session_metadata_without_binding_deserializes() {
        #[derive(Deserialize, Default)]
        struct Shim {
            #[serde(default)]
            source_binding: Option<SessionSourceBinding>,
        }
        // Absent → None via serde default.
        let v: Shim = serde_json::from_str("{}").expect("legacy JSON deserialises");
        assert!(v.source_binding.is_none());

        // Explicit null → also None.
        let v: Shim =
            serde_json::from_str(r#"{"source_binding": null}"#).expect("null deserialises");
        assert!(v.source_binding.is_none());

        // A populated field round-trips.
        let raw = SessionSourceBinding::new(
            SourceRef::Raw {
                id: 5,
                title: "t".into(),
            },
            Some("URL handoff".into()),
        );
        let json = serde_json::to_string(&raw).unwrap();
        let decoded: SessionSourceBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.source, raw.source);
        assert_eq!(decoded.binding_reason, raw.binding_reason);
    }

    /// `token_budget_for_source` returns the three distinct caps. If
    /// anyone tunes these, the frontend's byte-count progress bar
    /// reads from `source_token_hint` and does not depend on knowing
    /// the cap, so the budgets can change independently.
    #[test]
    fn token_budget_per_source_kind() {
        assert_eq!(
            token_budget_for_source(&SourceRef::Raw {
                id: 1,
                title: "t".into()
            }),
            6000
        );
        assert_eq!(
            token_budget_for_source(&SourceRef::Wiki {
                slug: "s".into(),
                title: "t".into()
            }),
            4000
        );
        assert_eq!(
            token_budget_for_source(&SourceRef::Inbox {
                id: 1,
                title: "t".into()
            }),
            3000
        );
    }

    /// The bound-source header must carry the user-visible title +
    /// kind tag. The LLM needs to see these consistently; we pin the
    /// surface text so a refactor doesn't silently change prompt
    /// framing (which would drift model outputs).
    #[test]
    fn format_bound_source_includes_header_and_body() {
        let source = SourceRef::Raw {
            id: 1,
            title: "测试来源".into(),
        };
        let out = format_bound_source(&source, "body-bytes");
        assert!(out.contains("绑定来源"));
        assert!(out.contains("raw"));
        assert!(out.contains("测试来源"));
        assert!(out.contains("body-bytes"));
        assert!(out.contains("请优先基于下方来源内容回答当前问题"));
    }

    /// A4 — the bound-source block must contain every one of the
    /// five Grounded Mode rules (grounding, quote anchoring,
    /// conservative behaviour, evidence section, no fabrication).
    /// If any rule silently drops the LLM's output framing will
    /// regress (no quotes, hallucinated content, missing evidence
    /// footer). Pin each rule's load-bearing marker text so a
    /// careless rewrite can't erode the contract.
    #[test]
    fn format_bound_source_includes_grounding_rules() {
        let source = SourceRef::Raw {
            id: 1,
            title: "测试".into(),
        };
        let out = format_bound_source(&source, "body-bytes");

        // Rule header must appear.
        assert!(
            out.contains("Grounded Mode · 回答规则"),
            "missing Grounded Mode header: {out}"
        );

        // Rule 1: grounding.
        assert!(out.contains("仅基于上方"), "missing grounding rule: {out}");
        // Rule 2: quote anchoring — the word "blockquote" is the
        // most stable handle here (the Chinese text around it can
        // be reworded, but the `> blockquote` markdown idiom is
        // the actual instruction to the LLM).
        assert!(
            out.contains("blockquote"),
            "missing blockquote instruction: {out}"
        );
        // Rule 3: conservative behaviour — the "当前来源未提及"
        // phrase is the pattern the LLM should emit when the
        // source doesn't cover something.
        assert!(
            out.contains("当前来源未提及"),
            "missing conservative-behaviour phrasing: {out}"
        );
        // Rule 4: evidence section — the Chinese heading the LLM
        // should append at the end of its answer.
        assert!(
            out.contains("依据片段"),
            "missing evidence section header: {out}"
        );
        // Rule 5: no fabrication.
        assert!(
            out.contains("不要编造"),
            "missing no-fabrication rule: {out}"
        );
    }

    /// A4 — the standalone `grounding_instruction_block` helper (used
    /// by `append_user_message` on the A3 auto-bound path, where
    /// `format_bound_source` isn't re-run but the rules still need
    /// to reach the system prompt) must emit the same five rule
    /// markers. Keeps A2 and A3 UX symmetric at the prompt layer.
    #[test]
    fn grounding_instruction_block_is_stable() {
        let out = grounding_instruction_block();
        assert!(out.contains("Grounded Mode · 回答规则"));
        assert!(out.contains("仅基于上方"));
        assert!(out.contains("blockquote"));
        assert!(out.contains("当前来源未提及"));
        assert!(out.contains("依据片段"));
        assert!(out.contains("不要编造"));
        // Leading newlines so the block separates cleanly when
        // pushed onto an existing system_prompt_text.
        assert!(
            out.starts_with("\n\n"),
            "block must start with blank line padding: {out:?}"
        );
    }

    /// A4 — the instruction block must appear *after* the body so
    /// the LLM reads the material first and the rules second. Chat
    /// models follow rules that reference already-seen content more
    /// reliably than pre-content guardrails. Pin the ordering by
    /// scanning indices: the body marker must come before the rule
    /// header; neither index may be absent.
    #[test]
    fn format_bound_source_instruction_follows_body() {
        let source = SourceRef::Raw {
            id: 1,
            title: "t".into(),
        };
        let out = format_bound_source(&source, "BODYMARK_42");
        let body_idx = out.find("BODYMARK_42").expect("body present");
        let rule_idx = out
            .find("Grounded Mode · 回答规则")
            .expect("rule header present");
        assert!(
            body_idx < rule_idx,
            "body must appear before rules: body={body_idx}, rules={rule_idx}\nout={out}"
        );
        // And the body's closing `---` fence must also precede the
        // rule header — otherwise the LLM might read the rules as
        // part of the quoted source.
        let closing_fence_idx = out[body_idx..]
            .find("---")
            .map(|i| body_idx + i)
            .expect("closing fence present");
        assert!(
            closing_fence_idx < rule_idx,
            "closing fence must separate body from rules: fence={closing_fence_idx}, rules={rule_idx}"
        );
    }
}

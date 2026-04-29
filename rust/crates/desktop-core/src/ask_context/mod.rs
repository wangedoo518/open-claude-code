//! A1 — Ask context engine / mode detection / prompt packaging.
//!
//! This module centralises the "how should the LLM treat history vs.
//! the newly-provided source" decision that used to be implicit in
//! `append_user_message` + `build_api_request`. Prior behaviour always
//! sent the full session history with every request, which caused
//! topic-drift when the user pasted a brand-new URL mid-session: the
//! LLM could still see every earlier turn and tended to keep answering
//! along the old thread.
//!
//! Three modes are now declared explicitly:
//!   * `FollowUp` (default) — preserve full history, no extra marker.
//!   * `SourceFirst` — trim history to the most recent turn, inject an
//!     explicit "new task" boundary marker, re-prefix the enriched
//!     source so the LLM reads it as "fresh material for this question
//!     only". This is the setting that fixes the drift problem.
//!   * `Combine` — keep full history AND add a boundary marker that
//!     asks the LLM to distinguish history-derived vs. source-derived
//!     conclusions.
//!
//! Frontend sends one of `"follow_up" | "source_first" | "combine"`
//! (or omits the field entirely) on the message-append body; the
//! server maps `None` to the `FollowUp` default so old clients stay
//! on the prior behaviour.
//!
//! The actual prompt packaging lives in `package_history` +
//! `boundary_marker_for` + `enrich_prefix_for`; `build_api_request`
//! in `agentic_loop.rs` and `append_user_message` in `lib.rs` call
//! them so the decision lives in one place.

// A2 — explicit source binding. See `binding.rs` for `SourceRef`
// (the tagged pointer at a raw/wiki/inbox source) and
// `SessionSourceBinding` (the persistent wrapper stored on
// `SessionMetadata`). When a binding is active it overrides the A1
// mode classifier — the bound source becomes the highest-priority
// context slice in the system prompt.
pub mod binding;

pub use binding::{
    format_bound_source, grounding_instruction_block, parse_binding_key, token_budget_for_source,
    truncate_source_body, SessionSourceBinding, SourceRef,
};

use runtime::ConversationMessage;
use serde::{Deserialize, Serialize};

/// User-selected intent about how the new turn should relate to the
/// existing session history.
///
/// `serde(rename_all = "snake_case")` so the HTTP body uses
/// `"follow_up" | "source_first" | "combine"`, matching the frontend
/// string literal it already sends.
///
/// `Default = FollowUp` guarantees:
/// 1. Old JSON payloads without a `mode` field deserialise cleanly.
/// 2. Old persisted `Session` snapshots that never saw this enum
///    still round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    /// Preserve the full session history; no boundary marker added.
    /// This is the legacy behaviour.
    #[default]
    FollowUp,
    /// Trim history to the most recent turn and tell the LLM to
    /// answer *only* from the fresh source. Fixes the "sticks to
    /// old topic after new URL" bug.
    SourceFirst,
    /// Keep history *and* the new source; ask the LLM to label which
    /// conclusions came from which.
    Combine,
}

impl ContextMode {
    /// Returns `true` for modes that require a boundary marker in the
    /// system prompt (`SourceFirst`, `Combine`).
    #[must_use]
    pub fn has_boundary_marker(self) -> bool {
        matches!(self, Self::SourceFirst | Self::Combine)
    }

    /// Returns `true` for modes that rewrite the enrich prefix text
    /// (`SourceFirst`, `Combine` — both use the "新素材" framing).
    #[must_use]
    pub fn rewrites_enrich_prefix(self) -> bool {
        matches!(self, Self::SourceFirst | Self::Combine)
    }
}

/// Snapshot of how the backend packaged the user's turn for the LLM.
/// Broadcast to the frontend over SSE so the UI can render a
/// `ContextBasisLabel` chip explaining which mode actually ran +
/// how much history / source the LLM saw.
///
/// Fields:
///  * `mode` — effective mode used (after `None → FollowUp` default).
///  * `history_turns_included` — number of *historical* messages that
///    survived the trim. For `FollowUp` / `Combine` this equals the
///    count of pre-existing messages before the new user turn. For
///    `SourceFirst` it's capped at 2 (one user + one assistant).
///  * `source_included` — `true` iff enrichment attached a source
///    body to the system prompt. Independent of mode.
///  * `source_token_hint` — rough token estimate for the attached
///    source body (`bytes / 4`). `None` when no source was attached.
///  * `boundary_marker` — `true` iff a boundary marker string was
///    inserted into the system prompt for this turn.
///  * `bound_source` — A2: echo of the `SessionSourceBinding.source`
///    in effect for the turn, if any. Independent of mode — when a
///    binding is active the backend forces `SourceFirst`-ish framing
///    and the frontend uses this field to render a "绑定来源" chip.
///    `None` when no binding is set.
///  * `auto_bound` — A3: `true` when `bound_source` was produced by
///    Fresh-Link auto-binding (a fresh URL enrich inside this very
///    turn) rather than a persistent `SessionSourceBinding`. Used by
///    the UI so the chip can distinguish "pinned by you" (A2) from
///    "auto-pinned for this turn" (A3). Defaults to `false` and is
///    omitted from JSON when `false` so legacy consumers + older
///    persisted snapshots remain untouched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextBasis {
    pub mode: ContextMode,
    pub history_turns_included: usize,
    pub source_included: bool,
    pub source_token_hint: Option<usize>,
    pub boundary_marker: bool,
    /// A2: when the session had an active `SessionSourceBinding`, the
    /// backend stamps the `SourceRef` here so the UI can display the
    /// bound-source chip without another round trip. `None` means no
    /// explicit binding was in effect (the turn used pure A1 mode
    /// semantics). Legacy JSON without this field decodes to `None`
    /// via `#[serde(default)]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_source: Option<binding::SourceRef>,
    /// A3: `true` when `bound_source` was derived from a fresh URL
    /// enrich inside this turn rather than an explicit user binding.
    /// Auto-bound sources are turn-local — they do NOT persist to
    /// `SessionMetadata.source_binding` and naturally expire on the
    /// next turn if no new URL arrives. `false` when `bound_source`
    /// came from `SessionMetadata.source_binding` (A2 explicit
    /// binding) or when no source was bound at all. Legacy JSON
    /// without this field decodes to `false` via `#[serde(default)]`;
    /// the skip-when-false serializer keeps the wire + on-disk
    /// shape untouched for the common case.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_bound: bool,
    /// A4: `true` when the turn's system prompt carried the Grounded
    /// Mode instruction block — the set of five rules (answer only
    /// from bound source, quote original text with `> blockquote`,
    /// be explicit when the source doesn't cover a point, append
    /// `### 依据片段` section, never fabricate quotes).
    ///
    /// The flag is symmetric across A2 (explicit session binding)
    /// and A3 (turn-local auto-bind) — both paths stamp it `true`
    /// when `bound_source.is_some()`. The frontend reads it to render
    /// a "✓ Grounded" badge next to the existing binding chip so
    /// the user can see when the model is operating under the
    /// stricter prompt.
    ///
    /// Legacy JSON without this field decodes to `false` via
    /// `#[serde(default)]`; the skip-when-false serializer keeps
    /// the wire + on-disk shape untouched for pre-A4 snapshots and
    /// for non-binding turns.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub grounding_applied: bool,
    /// R1.1: `Some(true)` when `bound_source` resolved to a body
    /// that `wiki_store::is_full_article` classifies as an article
    /// (i.e. URL fetch / WeChat-article fetch / PDF / DOCX / PPTX
    /// succeeded). `Some(false)` when the bound source resolved but
    /// is a non-article raw — chat text, voice transcript, archived
    /// link without a fetched body. `None` when no source was bound
    /// or when the resolver could not load the raw at all.
    ///
    /// The frontend reads this to render a yellow warning chip
    /// ("只保存了链接 / 原文未抓取") instead of the regular green
    /// "Grounded" chip — and to disable affordances that only make
    /// sense with a real article (pin-as-source-of-truth, etc).
    ///
    /// On the prompt side, when this is `Some(false)` the backend
    /// pushes a sentinel system message instead of the bound-source
    /// body + Grounded Mode rules, so the LLM cannot hallucinate a
    /// summary of an empty body. See
    /// `binding::format_archived_link_sentinel`.
    ///
    /// Legacy JSON without this field decodes to `None` via
    /// `#[serde(default)]`; skip-when-none serializer keeps the
    /// wire + on-disk shape untouched for pre-R1.1 snapshots and
    /// for non-binding turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_source_is_article: Option<bool>,
    /// Purpose Lens values selected for this turn. Empty means the
    /// backend should use the default cross-purpose behaviour.
    ///
    /// The UI sends the same controlled vocabulary used by wiki
    /// frontmatter (`writing`, `building`, `operating`, `learning`,
    /// `personal`, `research`) while still allowing future
    /// organization-specific lowercase slugs. Values are normalized
    /// and validated before they reach this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub purpose_lenses: Vec<String>,
}

impl ContextBasis {
    /// Build a `ContextBasis` from the decisions `append_user_message`
    /// made. Kept as a small helper so call-sites can't forget a field.
    ///
    /// Defaults `bound_source` to `None`; callers that have an active
    /// binding should populate it directly via `with_bound_source`.
    #[must_use]
    pub fn new(
        mode: ContextMode,
        history_turns_included: usize,
        source_bytes: Option<usize>,
    ) -> Self {
        let source_included = source_bytes.is_some();
        Self {
            mode,
            history_turns_included,
            source_included,
            source_token_hint: source_bytes.map(approximate_tokens_from_bytes),
            boundary_marker: mode.has_boundary_marker(),
            bound_source: None,
            auto_bound: false,
            grounding_applied: false,
            bound_source_is_article: None,
            purpose_lenses: Vec::new(),
        }
    }

    /// Attach a `SourceRef` to the basis. Called by
    /// `append_user_message` when a `SessionSourceBinding` is active;
    /// the frontend reads `bound_source` to render the binding chip.
    #[must_use]
    pub fn with_bound_source(mut self, source: Option<binding::SourceRef>) -> Self {
        self.bound_source = source;
        self
    }

    /// A3: mark this basis' `bound_source` as auto-bound (Fresh-Link
    /// auto-binding from a URL enrich this turn) rather than an
    /// explicit session-level pin. Call with `true` from
    /// `append_user_message` when `bound_source` was produced by an
    /// enrich success rather than resolved from
    /// `SessionMetadata.source_binding`. Defaults to `false` on every
    /// freshly-constructed basis.
    #[must_use]
    pub fn with_auto_bound(mut self, auto_bound: bool) -> Self {
        self.auto_bound = auto_bound;
        self
    }

    /// A4: stamp that the Grounded Mode instruction block was
    /// actually injected into this turn's system prompt. Called by
    /// `append_user_message` on both the A2 (session binding) and A3
    /// (auto-bind) paths — symmetric so the frontend can render a
    /// single "✓ Grounded" badge regardless of which binding flavour
    /// produced the bound source. Defaults to `false` on every
    /// freshly-constructed basis; when `false`, the serializer skips
    /// the field so pre-A4 consumers + on-disk snapshots are byte-
    /// identical to before.
    #[must_use]
    pub fn with_grounding_applied(mut self, applied: bool) -> Self {
        self.grounding_applied = applied;
        self
    }

    /// R1.1: stamp whether the resolved bound source is an article
    /// (full document body the LLM can summarize) or a non-article
    /// raw (chat text / voice transcript / archived link without a
    /// fetched body). The frontend reads this to render a warning
    /// chip in the latter case so the user sees "只保存了链接，原文
    /// 未抓取" instead of a misleading "Grounded" badge. `None` means
    /// no source was bound for this turn (or the resolver could not
    /// load the raw, in which case the binding is degraded to a
    /// pre-A2 turn anyway).
    #[must_use]
    pub fn with_bound_source_is_article(mut self, is_article: Option<bool>) -> Self {
        self.bound_source_is_article = is_article;
        self
    }

    /// Stamp the Purpose Lens values that shaped this turn. The caller
    /// passes already-normalized slugs; storing them on ContextBasis
    /// lets the UI explain why the answer was framed as writing,
    /// building, research, etc.
    #[must_use]
    pub fn with_purpose_lenses(mut self, lenses: Vec<String>) -> Self {
        self.purpose_lenses = lenses;
        self
    }
}

/// Rough token hint from byte length. `bytes / 4` approximates
/// per-token cost for mixed ASCII + CJK; the frontend only uses this
/// as a display chip, so precision is not important.
#[must_use]
pub fn approximate_tokens_from_bytes(bytes: usize) -> usize {
    bytes / 4
}

/// Normalize user-supplied Purpose Lens slugs before they are used in
/// prompts or echoed to the UI. We intentionally keep this stricter
/// than YAML frontmatter parsing because this path is an HTTP request
/// body that can be hit by external tools.
#[must_use]
pub fn normalize_purpose_lenses<I, S>(raw: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let lens = item.as_ref().trim().to_ascii_lowercase();
        if lens.is_empty() || lens.len() > 32 {
            continue;
        }
        if !lens
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        {
            continue;
        }
        if !out.iter().any(|existing| existing == &lens) {
            out.push(lens);
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

/// Prompt block for Ask Purpose mode. This is intentionally short:
/// the lens should shape source selection, output form, and caveats
/// without overwhelming the normal system prompt or changing the
/// user's visible message history.
#[must_use]
pub fn purpose_instruction_block(lenses: &[String]) -> Option<String> {
    if lenses.is_empty() {
        return None;
    }
    let joined = lenses.join(", ");
    Some(format!(
        "\n\n---\n# Purpose Lens\n\n本轮回答限定到以下目的：{joined}。\n请优先选择与这些 purpose 相关的知识、模板和表达方式；如果材料不足，先说明缺口，再给出该目的下可直接使用的下一步。"
    ))
}

/// `SourceFirst` boundary marker. Injected into the system prompt
/// *before* the enriched source body so the LLM reads the reset
/// instruction first.
pub const BOUNDARY_MARKER_SOURCE_FIRST: &str = "\n\n---\n# 新任务开始\n\n请**仅基于下方来源**回答用户当前问题，**忽略此前对话历史**（它们作为背景参考，但不应主导答案）。如果来源信息不足，直接说明\"资料中未提及\"，不要凭印象续写。\n\n";

/// `Combine` boundary marker. Keeps history but asks the LLM to
/// distinguish history-derived vs. source-derived conclusions.
pub const BOUNDARY_MARKER_COMBINE: &str = "\n\n---\n# 结合历史与新素材\n\n用户希望综合之前的对话与下方新素材共同回答。请明确区分：哪些结论来自历史讨论、哪些来自新素材。\n\n";

/// Return the boundary marker string for a given mode, or `None`
/// if the mode doesn't need one (`FollowUp`).
#[must_use]
pub fn boundary_marker_for(mode: ContextMode) -> Option<&'static str> {
    match mode {
        ContextMode::FollowUp => None,
        ContextMode::SourceFirst => Some(BOUNDARY_MARKER_SOURCE_FIRST),
        ContextMode::Combine => Some(BOUNDARY_MARKER_COMBINE),
    }
}

/// Format the enrich prefix that wraps the fetched source body before
/// it's appended to the system prompt.
///
/// `FollowUp` keeps the legacy wording (`请基于以下文章内容...`).
/// `SourceFirst` + `Combine` both use the new `新素材：...` framing.
///
/// `title` and `body` are the already-truncated article title + body.
#[must_use]
pub fn format_enriched_source(
    mode: ContextMode,
    title: &str,
    body: &str,
    original_message: &str,
) -> String {
    if mode.rewrites_enrich_prefix() {
        format!(
            "新素材：以下是用户刚提供的来源，请基于它回答。\n\n{title}\n\n{body}\n\n---\n用户原始消息：{original_message}"
        )
    } else {
        // Legacy FollowUp prefix — preserved verbatim so the FollowUp
        // behaviour is bit-for-bit identical to the pre-A1 code.
        format!(
            "请基于以下文章内容回答我的问题。\n\n标题：{title}\n\n{body}\n\n---\n用户原始消息：{original_message}"
        )
    }
}

/// Package the session history for an LLM request based on the
/// effective context mode.
///
/// * `FollowUp` / `Combine` return the input slice unchanged.
/// * `SourceFirst` truncates to the last 2 messages (one user +
///   one assistant at most). The new user message that just got
///   pushed is *already in the slice*, so we keep it plus at most
///   one prior turn.
///
/// Returns a `Vec<ConversationMessage>` that the caller can feed
/// directly into `build_api_request`'s serialisation loop.
#[must_use]
pub fn package_history(
    mode: ContextMode,
    messages: &[ConversationMessage],
) -> Vec<ConversationMessage> {
    match mode {
        ContextMode::FollowUp | ContextMode::Combine => messages.to_vec(),
        ContextMode::SourceFirst => {
            // Keep at most the final 2 messages: the current user
            // turn (which must always survive — it's the actual
            // question) plus the single immediately-prior assistant
            // turn, which can serve as a lightweight background hint
            // without dragging earlier topics back into play.
            if messages.len() <= 2 {
                messages.to_vec()
            } else {
                messages[messages.len() - 2..].to_vec()
            }
        }
    }
}

/// How many *historical* messages survived `package_history`. Counts
/// every message except the trailing user turn (which is the brand-
/// new question, not history). Used to populate `ContextBasis`.
#[must_use]
pub fn history_turns_after_packaging(mode: ContextMode, messages: &[ConversationMessage]) -> usize {
    let packaged = package_history(mode, messages);
    // The current user message is always the last entry; subtract 1
    // so the frontend chip reads "N 轮历史" not "N+1 including self".
    packaged.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{ContentBlock, ConversationMessage, MessageRole};

    fn user_msg(text: &str) -> ConversationMessage {
        ConversationMessage::user_text(text.to_string())
    }

    fn assistant_msg(text: &str) -> ConversationMessage {
        ConversationMessage {
            role: MessageRole::Assistant,
            blocks: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            usage: None,
        }
    }

    /// Helper: build a synthetic 10-turn (20 message) conversation +
    /// one fresh user turn on top. Matches the brief's 10-turn
    /// scenario.
    fn ten_turn_history_with_fresh_user() -> Vec<ConversationMessage> {
        let mut msgs = Vec::new();
        for i in 0..10 {
            msgs.push(user_msg(&format!("user turn {i}")));
            msgs.push(assistant_msg(&format!("assistant turn {i}")));
        }
        msgs.push(user_msg("new fresh question"));
        msgs
    }

    /// Spec test 1 — FollowUp preserves the full history.
    #[test]
    fn test_follow_up_preserves_full_history() {
        let msgs = ten_turn_history_with_fresh_user();
        let packaged = package_history(ContextMode::FollowUp, &msgs);
        assert_eq!(
            packaged.len(),
            msgs.len(),
            "FollowUp must not trim; expected {} got {}",
            msgs.len(),
            packaged.len()
        );
        assert_eq!(packaged, msgs, "FollowUp must preserve message order");
    }

    /// Spec test 2 — SourceFirst truncates history to ≤2 prior msgs.
    #[test]
    fn test_source_first_truncates_history() {
        let msgs = ten_turn_history_with_fresh_user();
        assert!(msgs.len() >= 20, "setup sanity");
        let packaged = package_history(ContextMode::SourceFirst, &msgs);
        assert!(
            packaged.len() <= 2,
            "SourceFirst must trim to ≤2 msgs, got {}",
            packaged.len()
        );
        // The fresh user turn must always survive.
        let last = packaged.last().expect("packaged non-empty");
        assert_eq!(last.role, MessageRole::User);
        assert!(matches!(
            &last.blocks[0],
            ContentBlock::Text { text } if text == "new fresh question"
        ));
    }

    /// Spec test 3 — SourceFirst emits the "新任务开始" boundary
    /// marker. This is what the backend injects into system_prompt
    /// before the source body.
    #[test]
    fn test_source_first_adds_task_boundary() {
        let marker = boundary_marker_for(ContextMode::SourceFirst).expect("SourceFirst has marker");
        assert!(
            marker.contains("新任务开始"),
            "SourceFirst marker must include '新任务开始': got {marker:?}"
        );
        assert!(
            marker.contains("忽略此前对话历史"),
            "SourceFirst marker must instruct ignoring history: got {marker:?}"
        );
        // And a follow-up sanity check: FollowUp yields no marker.
        assert!(
            boundary_marker_for(ContextMode::FollowUp).is_none(),
            "FollowUp must not emit any marker"
        );
    }

    /// Spec test 4 — Combine keeps history and adds marker.
    #[test]
    fn test_combine_keeps_history_and_adds_marker() {
        let msgs = ten_turn_history_with_fresh_user();
        let packaged = package_history(ContextMode::Combine, &msgs);
        assert_eq!(
            packaged.len(),
            msgs.len(),
            "Combine must preserve full history"
        );
        let marker = boundary_marker_for(ContextMode::Combine).expect("Combine has marker");
        assert!(
            marker.contains("结合历史与新素材"),
            "Combine marker must include '结合历史与新素材': got {marker:?}"
        );
    }

    /// Spec test 5 — legacy JSON without the mode field deserialises
    /// back to FollowUp. Guards session-on-disk back-compat.
    #[test]
    fn test_context_mode_default_deserializes_for_legacy() {
        // A wrapping struct models an old persisted record that
        // added `mode` later; the default `FollowUp` must kick in.
        #[derive(Deserialize, Default)]
        struct Legacy {
            #[serde(default)]
            mode: ContextMode,
        }
        let v: Legacy = serde_json::from_str("{}").expect("legacy JSON deserialises");
        assert_eq!(v.mode, ContextMode::FollowUp);

        // Explicit null should *not* parse (serde default kicks in
        // only when the field is absent; null goes to the variant
        // parser and errors). Confirm this is the contract.
        let v2: Result<Legacy, _> = serde_json::from_str("{\"mode\": null}");
        assert!(
            v2.is_err(),
            "explicit null is not the default path (absence is)"
        );

        // Positive round-trip: each variant tag must encode as the
        // snake_case string the frontend sends.
        for (mode, tag) in [
            (ContextMode::FollowUp, "\"follow_up\""),
            (ContextMode::SourceFirst, "\"source_first\""),
            (ContextMode::Combine, "\"combine\""),
        ] {
            let encoded = serde_json::to_string(&mode).unwrap();
            assert_eq!(encoded, tag);
            let decoded: ContextMode = serde_json::from_str(tag).unwrap();
            assert_eq!(decoded, mode);
        }
    }

    /// Extra: `history_turns_after_packaging` — frontend chip label.
    #[test]
    fn test_history_turns_count_subtracts_current_user_turn() {
        let msgs = ten_turn_history_with_fresh_user(); // 21 messages
        assert_eq!(
            history_turns_after_packaging(ContextMode::FollowUp, &msgs),
            20,
            "FollowUp: 20 historical + 1 fresh current"
        );
        assert_eq!(
            history_turns_after_packaging(ContextMode::Combine, &msgs),
            20
        );
        // SourceFirst: 2 kept (1 prior assistant + current user) → 1
        // historical from the frontend's POV.
        assert_eq!(
            history_turns_after_packaging(ContextMode::SourceFirst, &msgs),
            1
        );
    }

    /// Extra: enrich prefix formatting routes by mode.
    #[test]
    fn test_format_enriched_source_routes_by_mode() {
        let follow = format_enriched_source(ContextMode::FollowUp, "T", "body-bytes", "orig");
        assert!(
            follow.starts_with("请基于以下文章内容回答我的问题。"),
            "FollowUp must keep legacy prefix"
        );

        let sf = format_enriched_source(ContextMode::SourceFirst, "T", "body-bytes", "orig");
        assert!(
            sf.starts_with("新素材：以下是用户刚提供的来源"),
            "SourceFirst must use 新素材 prefix"
        );

        let cb = format_enriched_source(ContextMode::Combine, "T", "body-bytes", "orig");
        assert!(
            cb.starts_with("新素材：以下是用户刚提供的来源"),
            "Combine must also use 新素材 prefix"
        );
    }

    #[test]
    fn purpose_lenses_normalize_and_dedupe_for_prompt_safety() {
        let lenses = normalize_purpose_lenses([
            " Research ",
            "research",
            "personal",
            "bad space",
            "../prompt",
            "building",
        ]);
        assert_eq!(lenses, vec!["research", "personal", "building"]);
    }

    #[test]
    fn purpose_instruction_block_names_selected_lenses() {
        let lenses = vec!["research".to_string(), "building".to_string()];
        let block = purpose_instruction_block(&lenses).expect("purpose block");
        assert!(block.contains("Purpose Lens"));
        assert!(block.contains("research, building"));
        assert!(block.contains("可直接使用的下一步"));
    }

    /// Extra: ContextBasis builder fills fields consistently.
    #[test]
    fn test_context_basis_fields() {
        let basis_no_src = ContextBasis::new(ContextMode::FollowUp, 4, None);
        assert_eq!(basis_no_src.mode, ContextMode::FollowUp);
        assert_eq!(basis_no_src.history_turns_included, 4);
        assert!(!basis_no_src.source_included);
        assert_eq!(basis_no_src.source_token_hint, None);
        assert!(!basis_no_src.boundary_marker);
        // A2: `new()` defaults `bound_source` to `None`; the binding
        // is attached separately by `append_user_message` when
        // `SessionSourceBinding` is active.
        assert!(basis_no_src.bound_source.is_none());
        // A3: `new()` defaults `auto_bound` to `false`; the flag is
        // flipped on via `with_auto_bound(true)` when Fresh-Link
        // auto-binding fires.
        assert!(!basis_no_src.auto_bound);
        // A4: `new()` defaults `grounding_applied` to `false`; the
        // flag is flipped on via `with_grounding_applied(true)`
        // from `append_user_message` when a bound source is
        // resolved (A2 or A3 path).
        assert!(!basis_no_src.grounding_applied);

        let basis_sf = ContextBasis::new(ContextMode::SourceFirst, 1, Some(4000));
        assert_eq!(basis_sf.mode, ContextMode::SourceFirst);
        assert!(basis_sf.source_included);
        assert_eq!(basis_sf.source_token_hint, Some(1000));
        assert!(basis_sf.boundary_marker);

        let basis_cb = ContextBasis::new(ContextMode::Combine, 7, Some(8));
        assert!(basis_cb.boundary_marker);
        assert_eq!(basis_cb.source_token_hint, Some(2));
    }

    /// A2: `with_bound_source` attaches the SourceRef so the frontend
    /// chip can render without a second round trip. Round-tripping a
    /// basis with an attached source preserves the field.
    #[test]
    fn test_context_basis_with_bound_source_round_trips() {
        let source = binding::SourceRef::Raw {
            id: 7,
            title: "my-raw".into(),
        };
        let basis = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400))
            .with_bound_source(Some(source.clone()));
        assert_eq!(basis.bound_source, Some(source));

        let json = serde_json::to_string(&basis).expect("serialize");
        assert!(
            json.contains("\"bound_source\""),
            "bound_source must serialize when Some: {json}"
        );
        let decoded: ContextBasis = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, basis);

        // None → field is omitted (skip_serializing_if).
        let plain = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400));
        let json = serde_json::to_string(&plain).unwrap();
        assert!(
            !json.contains("bound_source"),
            "None must be omitted by skip_serializing_if: {json}"
        );
    }

    /// A3: `auto_bound = true` must show up in the JSON so the
    /// frontend can distinguish a turn-local Fresh-Link auto-bind
    /// from an explicit session-level binding (A2). `with_auto_bound`
    /// flips the field on via a chained builder.
    #[test]
    fn test_context_basis_auto_bound_serializes_when_true() {
        let source = binding::SourceRef::Raw {
            id: 11,
            title: "fresh-link".into(),
        };
        let basis = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400))
            .with_bound_source(Some(source.clone()))
            .with_auto_bound(true);
        assert!(basis.auto_bound, "with_auto_bound(true) must flip the flag");

        let json = serde_json::to_string(&basis).expect("serialize");
        assert!(
            json.contains("\"auto_bound\":true"),
            "auto_bound must serialize as true when set: {json}"
        );

        // Round-trip — decoded basis preserves the flag.
        let decoded: ContextBasis = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, basis);
        assert!(decoded.auto_bound);
        assert_eq!(decoded.bound_source, Some(source));
    }

    /// A3: default `auto_bound = false` must be omitted from the JSON
    /// (skip_serializing_if) so the common case keeps the on-disk +
    /// on-wire footprint identical to pre-A3 snapshots. A2 session
    /// bindings also go through the default path (auto_bound stays
    /// false) and must not accidentally emit the field.
    #[test]
    fn test_context_basis_auto_bound_omitted_when_false() {
        // Plain basis — no binding, no auto-bind.
        let plain = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400));
        let json = serde_json::to_string(&plain).expect("serialize");
        assert!(
            !json.contains("auto_bound"),
            "default auto_bound=false must be skipped: {json}"
        );

        // A2-style basis — explicit binding, auto_bound stays false
        // because the source came from SessionMetadata, not from an
        // enrich inside this turn.
        let source = binding::SourceRef::Raw {
            id: 22,
            title: "pinned".into(),
        };
        let a2_basis = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400))
            .with_bound_source(Some(source));
        let a2_json = serde_json::to_string(&a2_basis).expect("serialize");
        assert!(
            !a2_json.contains("auto_bound"),
            "A2 explicit-binding basis must not emit auto_bound: {a2_json}"
        );
        // bound_source should still show up — only auto_bound is skipped.
        assert!(
            a2_json.contains("bound_source"),
            "bound_source must still serialize for A2: {a2_json}"
        );

        // Explicitly setting `with_auto_bound(false)` is also omitted.
        let off = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400)).with_auto_bound(false);
        let off_json = serde_json::to_string(&off).expect("serialize");
        assert!(
            !off_json.contains("auto_bound"),
            "explicit false must also skip: {off_json}"
        );
    }

    /// A4: `grounding_applied = true` must show up in the JSON so
    /// the frontend can render a "✓ Grounded" badge alongside the
    /// binding chip. Default `false` is omitted via
    /// `skip_serializing_if`, so a bound-source turn without the
    /// Grounded Mode instruction block (shouldn't happen in practice,
    /// but kept explicit in the contract) stays byte-identical to
    /// pre-A4 snapshots.
    #[test]
    fn test_context_basis_grounding_applied_serializes_when_true() {
        // Default basis — grounding_applied stays false, field is
        // omitted by skip_serializing_if.
        let plain = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400));
        let json = serde_json::to_string(&plain).expect("serialize");
        assert!(
            !json.contains("grounding_applied"),
            "default grounding_applied=false must be skipped: {json}"
        );

        // A2-flavoured basis with grounding applied — explicit
        // binding + Grounded Mode rules in the system prompt.
        let source = binding::SourceRef::Raw {
            id: 9,
            title: "grounded".into(),
        };
        let basis = ContextBasis::new(ContextMode::SourceFirst, 1, Some(400))
            .with_bound_source(Some(source.clone()))
            .with_grounding_applied(true);
        assert!(
            basis.grounding_applied,
            "with_grounding_applied(true) must flip the flag"
        );

        let json = serde_json::to_string(&basis).expect("serialize");
        assert!(
            json.contains("\"grounding_applied\":true"),
            "grounding_applied must serialize as true when set: {json}"
        );

        // Round-trip preserves the flag alongside bound_source.
        let decoded: ContextBasis = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, basis);
        assert!(decoded.grounding_applied);
        assert_eq!(decoded.bound_source, Some(source));

        // Explicit false must also be skipped — guards against a
        // future refactor that drops skip_serializing_if.
        let off =
            ContextBasis::new(ContextMode::SourceFirst, 1, Some(400)).with_grounding_applied(false);
        let off_json = serde_json::to_string(&off).expect("serialize");
        assert!(
            !off_json.contains("grounding_applied"),
            "explicit false must also skip: {off_json}"
        );
    }

    /// A4 back-compat: legacy JSON written before A4 (no
    /// `grounding_applied` field) must decode cleanly to
    /// `grounding_applied = false` via serde default. Covers both
    /// pre-A2 snapshots (no bound_source) and A2/A3 snapshots that
    /// had a bound_source but no grounding marker.
    #[test]
    fn test_context_basis_legacy_without_grounding_applied_deserializes() {
        // Pre-A2 legacy JSON — just the A1 fields.
        let legacy = r#"{
            "mode": "source_first",
            "history_turns_included": 1,
            "source_included": true,
            "source_token_hint": 100,
            "boundary_marker": true
        }"#;
        let decoded: ContextBasis = serde_json::from_str(legacy).expect("legacy JSON deserialises");
        assert!(
            !decoded.grounding_applied,
            "legacy grounding_applied defaults to false"
        );

        // A2 legacy JSON — has bound_source, no grounding_applied.
        let a2_legacy = r#"{
            "mode": "source_first",
            "history_turns_included": 1,
            "source_included": true,
            "source_token_hint": 100,
            "boundary_marker": true,
            "bound_source": { "kind": "wiki", "slug": "foo", "title": "Foo" }
        }"#;
        let decoded: ContextBasis =
            serde_json::from_str(a2_legacy).expect("A2 legacy deserialises");
        assert!(decoded.bound_source.is_some());
        assert!(
            !decoded.grounding_applied,
            "A2 legacy must default grounding_applied to false"
        );

        // A3 legacy JSON — has bound_source + auto_bound, no
        // grounding_applied (fresh-link turns pre-A4).
        let a3_legacy = r#"{
            "mode": "source_first",
            "history_turns_included": 1,
            "source_included": true,
            "source_token_hint": 100,
            "boundary_marker": true,
            "bound_source": { "kind": "raw", "id": 3, "title": "t" },
            "auto_bound": true
        }"#;
        let decoded: ContextBasis =
            serde_json::from_str(a3_legacy).expect("A3 legacy deserialises");
        assert!(decoded.auto_bound);
        assert!(
            !decoded.grounding_applied,
            "A3 legacy must default grounding_applied to false"
        );
    }

    /// A3 back-compat: legacy JSON written before A3 (no `auto_bound`
    /// field) must still decode cleanly to `auto_bound = false` via
    /// serde default. Guards persisted `last_context_basis` snapshots
    /// in `SessionMetadata` on disk.
    #[test]
    fn test_context_basis_legacy_without_auto_bound_deserializes() {
        // Minimal legacy JSON: only the pre-A3 fields. No bound_source,
        // no auto_bound.
        let legacy = r#"{
            "mode": "source_first",
            "history_turns_included": 1,
            "source_included": true,
            "source_token_hint": 100,
            "boundary_marker": true
        }"#;
        let decoded: ContextBasis = serde_json::from_str(legacy).expect("legacy JSON deserialises");
        assert_eq!(decoded.mode, ContextMode::SourceFirst);
        assert!(decoded.boundary_marker);
        assert!(
            decoded.bound_source.is_none(),
            "legacy bound_source defaults to None"
        );
        assert!(
            !decoded.auto_bound,
            "legacy auto_bound defaults to false via serde(default)"
        );

        // Legacy A2 JSON (has bound_source but no auto_bound) also
        // decodes — auto_bound stays false.
        let a2_legacy = r#"{
            "mode": "source_first",
            "history_turns_included": 1,
            "source_included": true,
            "source_token_hint": 100,
            "boundary_marker": true,
            "bound_source": { "kind": "raw", "id": 7, "title": "t" }
        }"#;
        let decoded: ContextBasis =
            serde_json::from_str(a2_legacy).expect("A2 legacy deserialises");
        assert!(decoded.bound_source.is_some());
        assert!(
            !decoded.auto_bound,
            "A2 legacy must default auto_bound to false"
        );
    }
}

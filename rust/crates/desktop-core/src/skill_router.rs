//! `SkillRouter` — parse and dispatch `/skill` commands typed into Chat.
//!
//! Canonical spec: `docs/design/technical-design.md §4.5.1` (L1931+).
//!
//! The router lexes a user-typed line like `/absorb 1 2 3` into a
//! [`SkillCommand`] enum, then dispatches to the right backend path and
//! returns a [`SkillResult`] that the HTTP layer can translate into a
//! 202 Accepted / SSE stream / error response.
//!
//! ## Scope of this commit (Sprint 1-B.1 · step 1)
//!
//! Step 1 lands the **router skeleton only**: parse input, generate a
//! canonical `{kind}-{unix_ts}-{4hex}` task id, and return
//! [`SkillResult::TaskStarted`] / [`SkillResult::StreamStarted`] /
//! [`SkillResult::ParseError`]. **No `TaskManager` hookup yet** — step 2
//! of the same sprint introduces [`crate::absorb_task::TaskManager`]
//! and replaces the manual id generation in `route()` with
//! `state.task_manager.register(...)`. Until then, the router simply
//! mints a one-shot id so the HTTP shape is already correct when step 2
//! lands.
//!
//! ## Why `Arc<DesktopState>` lives on the struct already
//!
//! The spec's signature stores `state: Arc<DesktopState>` on the router
//! so `route()` can reach into session state, provider registry,
//! task manager, etc. Step 1 doesn't dereference it yet, but keeping the
//! field now avoids a visible signature change in step 2. A `#[allow(
//! dead_code)]` on the field suppresses the interim unused-field lint.

use std::sync::Arc;

use crate::DesktopState;

/// Parsed SKILL command. Produced by [`SkillRouter::parse_command`].
///
/// Only the four canonical commands from `§4.5.1` are recognized;
/// everything else either passes through the router (returning `None`
/// from [`SkillRouter::route`]) or surfaces as a
/// [`SkillResult::ParseError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillCommand {
    /// `/absorb` — batch-absorb raw entries into wiki pages.
    /// Empty args means "absorb all un-absorbed raw entries".
    Absorb { entry_ids: Option<Vec<u32>> },
    /// `/query <question>` — Wiki-grounded Q&A (SSE stream).
    Query { question: String },
    /// `/cleanup` — run the patrol-based cleanup audit.
    Cleanup,
    /// `/patrol` — run a full patrol pass.
    Patrol,
}

/// Result of running a parsed SKILL command through `route()`.
///
/// The HTTP handler translates these into:
///   * [`Self::TaskStarted`]  → `202 Accepted` + `{task_id, status: "started"}`
///   * [`Self::StreamStarted`] → SSE stream handshake
///   * [`Self::ParseError`]   → `400 Bad Request` + `{error, message}`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillResult {
    /// Async task was spawned; progress will arrive via the SSE
    /// `absorb_progress` / `cleanup_progress` / `patrol_progress`
    /// events (see `technical-design.md §2.1`).
    TaskStarted { task_id: String },
    /// Streaming query response started; chunks arrive on the SSE
    /// `query_chunk` events + one terminal `query_done` / `query_error`.
    StreamStarted { task_id: String },
    /// Input matched `is_skill_command` but the arguments were invalid,
    /// or the command name was unknown. HTTP layer should return 400.
    ParseError { message: String },
}

/// Parse + dispatch `/skill` commands. See module-level docs.
pub struct SkillRouter {
    /// Held for step 2's `TaskManager` hookup. Step 1 does not read it;
    /// keeping the field now prevents a visible signature change later.
    #[allow(dead_code)]
    state: Arc<DesktopState>,
}

impl SkillRouter {
    /// Construct a router bound to the given [`DesktopState`].
    ///
    /// The `Arc` lets the router live alongside any number of HTTP
    /// handlers; none of them own state exclusively.
    #[must_use]
    pub fn new(state: Arc<DesktopState>) -> Self {
        Self { state }
    }

    /// Cheap gate used by upstream code to decide whether a user message
    /// is a SKILL command at all. Returns `true` iff the trimmed input
    /// starts with `/` and the first whitespace-separated token is one
    /// of the four canonical SKILL names.
    ///
    /// Intentionally static: callers hit this before constructing a
    /// `SkillRouter`, typically to skip the async [`Self::route`] path
    /// for plain chat messages.
    #[must_use]
    pub fn is_skill_command(input: &str) -> bool {
        let trimmed = input.trim_start();
        if !trimmed.starts_with('/') {
            return false;
        }
        let head = trimmed.split_whitespace().next().unwrap_or("");
        matches!(head, "/absorb" | "/query" | "/cleanup" | "/patrol")
    }

    /// Parse + dispatch. Returns `None` for non-SKILL input (the caller
    /// then forwards the message to the regular chat path).
    ///
    /// Step 1 stub: for `Absorb` / `Cleanup` / `Patrol` this mints a
    /// fresh task id and returns [`SkillResult::TaskStarted`]. Step 2
    /// replaces the manual id with `state.task_manager.register(...)`
    /// so one absorb can't race another (409 semantics).
    ///
    /// Kept `async` because the step 2 hookup adds `.await` on
    /// `state.task_manager.register(...)`; flipping the signature to
    /// sync now would cause a breaking API change when step 2 lands.
    #[allow(clippy::unused_async)]
    pub async fn route(&self, input: &str, _session_id: &str) -> Option<SkillResult> {
        let trimmed = input.trim();
        if !Self::is_skill_command(trimmed) {
            return None;
        }

        let command = match Self::parse_command(trimmed) {
            Ok(c) => c,
            Err(msg) => return Some(SkillResult::ParseError { message: msg }),
        };

        match command {
            SkillCommand::Absorb { entry_ids: _ } => Some(SkillResult::TaskStarted {
                task_id: generate_task_id("absorb"),
            }),
            SkillCommand::Query { question: _ } => Some(SkillResult::StreamStarted {
                task_id: generate_task_id("query"),
            }),
            SkillCommand::Cleanup => Some(SkillResult::TaskStarted {
                task_id: generate_task_id("cleanup"),
            }),
            SkillCommand::Patrol => Some(SkillResult::TaskStarted {
                task_id: generate_task_id("patrol"),
            }),
        }
    }

    /// Lex a trimmed line into a [`SkillCommand`]. The caller is
    /// expected to have already confirmed via [`Self::is_skill_command`]
    /// that `input` starts with one of the four canonical names; if
    /// not, this returns `Err(...)` instead of panicking.
    pub(crate) fn parse_command(input: &str) -> Result<SkillCommand, String> {
        let trimmed = input.trim_start();
        let (head, rest) = match trimmed.split_once(char::is_whitespace) {
            Some((h, r)) => (h, r.trim()),
            None => (trimmed, ""),
        };
        match head {
            "/absorb" => {
                if rest.is_empty() {
                    return Ok(SkillCommand::Absorb { entry_ids: None });
                }
                // Accept space- or comma-separated u32 ids.
                let mut ids = Vec::new();
                for tok in rest.split(|c: char| c == ',' || c.is_whitespace()) {
                    let tok = tok.trim();
                    if tok.is_empty() {
                        continue;
                    }
                    ids.push(
                        tok.parse::<u32>()
                            .map_err(|_| format!("无法解析 entry_id: {tok}"))?,
                    );
                }
                Ok(SkillCommand::Absorb {
                    entry_ids: if ids.is_empty() { None } else { Some(ids) },
                })
            }
            "/query" => {
                if rest.is_empty() {
                    Err("/query 需要问题文本".to_string())
                } else {
                    Ok(SkillCommand::Query {
                        question: rest.to_string(),
                    })
                }
            }
            "/cleanup" => Ok(SkillCommand::Cleanup),
            "/patrol" => Ok(SkillCommand::Patrol),
            other => Err(format!("未知 SKILL 命令: {other}")),
        }
    }
}

/// Format: `{kind}-{unix_secs}-{4hex}`. Matches the task id shape
/// used by `absorb_handler` in `desktop-server/src/lib.rs:6175` and
/// the §2.1 response spec.
fn generate_task_id(kind: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = t.as_secs();
    let nanos = t.subsec_nanos();
    format!("{kind}-{secs}-{:04x}", nanos & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_skill_command ──────────────────────────────────────────

    #[test]
    fn is_skill_command_recognizes_canonical_commands() {
        assert!(SkillRouter::is_skill_command("/absorb"));
        assert!(SkillRouter::is_skill_command("/absorb 1 2 3"));
        assert!(SkillRouter::is_skill_command("/query what is foo"));
        assert!(SkillRouter::is_skill_command("/cleanup"));
        assert!(SkillRouter::is_skill_command("/patrol"));
        // Leading whitespace is tolerated.
        assert!(SkillRouter::is_skill_command("  /absorb  "));
    }

    #[test]
    fn is_skill_command_rejects_non_slash_and_unknown() {
        assert!(!SkillRouter::is_skill_command("absorb"));
        assert!(!SkillRouter::is_skill_command("/unknown"));
        assert!(!SkillRouter::is_skill_command(""));
        assert!(!SkillRouter::is_skill_command("   "));
        // Slash must be the first non-whitespace character.
        assert!(!SkillRouter::is_skill_command("hello /absorb"));
    }

    // ── parse_command · SkillCommand::Absorb ──────────────────────

    #[test]
    fn parse_absorb_without_ids_returns_none() {
        let cmd = SkillRouter::parse_command("/absorb").unwrap();
        assert_eq!(cmd, SkillCommand::Absorb { entry_ids: None });
    }

    #[test]
    fn parse_absorb_space_separated_ids() {
        let cmd = SkillRouter::parse_command("/absorb 1 2 3").unwrap();
        assert_eq!(
            cmd,
            SkillCommand::Absorb {
                entry_ids: Some(vec![1, 2, 3])
            }
        );
    }

    #[test]
    fn parse_absorb_comma_separated_ids() {
        let cmd = SkillRouter::parse_command("/absorb 10,20,30").unwrap();
        assert_eq!(
            cmd,
            SkillCommand::Absorb {
                entry_ids: Some(vec![10, 20, 30])
            }
        );
    }

    #[test]
    fn parse_absorb_mixed_separators() {
        let cmd = SkillRouter::parse_command("/absorb 1, 2 3,4").unwrap();
        assert_eq!(
            cmd,
            SkillCommand::Absorb {
                entry_ids: Some(vec![1, 2, 3, 4])
            }
        );
    }

    #[test]
    fn parse_absorb_rejects_non_numeric_token() {
        let err = SkillRouter::parse_command("/absorb abc").unwrap_err();
        assert!(err.contains("abc"), "error must mention bad token: {err}");
    }

    // ── parse_command · SkillCommand::Query ───────────────────────

    #[test]
    fn parse_query_with_question() {
        let cmd = SkillRouter::parse_command("/query what is RAG").unwrap();
        assert_eq!(
            cmd,
            SkillCommand::Query {
                question: "what is RAG".to_string()
            }
        );
    }

    #[test]
    fn parse_query_preserves_inner_whitespace() {
        let cmd = SkillRouter::parse_command("/query a  b  c").unwrap();
        let SkillCommand::Query { question } = cmd else {
            panic!("expected Query");
        };
        assert_eq!(question, "a  b  c");
    }

    #[test]
    fn parse_query_without_question_is_error() {
        assert!(SkillRouter::parse_command("/query").is_err());
        assert!(SkillRouter::parse_command("/query   ").is_err());
    }

    // ── parse_command · SkillCommand::{Cleanup,Patrol} ────────────

    #[test]
    fn parse_cleanup_and_patrol_take_no_args() {
        assert_eq!(
            SkillRouter::parse_command("/cleanup").unwrap(),
            SkillCommand::Cleanup
        );
        assert_eq!(
            SkillRouter::parse_command("/patrol").unwrap(),
            SkillCommand::Patrol
        );
    }

    // ── parse_command · unknown ───────────────────────────────────

    #[test]
    fn parse_unknown_command_is_error() {
        let err = SkillRouter::parse_command("/foobar").unwrap_err();
        assert!(
            err.contains("未知 SKILL") || err.contains("foobar"),
            "error must identify bad command: {err}"
        );
    }

    // ── generate_task_id ──────────────────────────────────────────

    #[test]
    fn task_id_matches_canonical_shape() {
        let id = generate_task_id("absorb");
        assert!(id.starts_with("absorb-"));
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "absorb");
        assert!(
            parts[1].parse::<u64>().is_ok(),
            "ts part must be numeric: {}",
            parts[1]
        );
        assert_eq!(
            parts[2].len(),
            4,
            "hex suffix must be 4 chars: {}",
            parts[2]
        );
        assert!(parts[2].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn task_id_prefix_matches_kind() {
        assert!(generate_task_id("cleanup").starts_with("cleanup-"));
        assert!(generate_task_id("query").starts_with("query-"));
        assert!(generate_task_id("patrol").starts_with("patrol-"));
    }

    // ── route() · end-to-end ──────────────────────────────────────
    //
    // These tests construct a minimal `DesktopState` via `new()` (Mock
    // backend) so the router has somewhere to live. Step 1's route()
    // path never touches the state, so the Mock is sufficient.

    #[tokio::test]
    async fn route_returns_none_for_plain_chat() {
        let state = Arc::new(DesktopState::new());
        let router = SkillRouter::new(state);
        assert!(router.route("hello world", "session-1").await.is_none());
        assert!(router.route("", "session-1").await.is_none());
    }

    #[tokio::test]
    async fn route_returns_task_started_for_absorb() {
        let state = Arc::new(DesktopState::new());
        let router = SkillRouter::new(state);
        match router.route("/absorb", "session-1").await {
            Some(SkillResult::TaskStarted { task_id }) => {
                assert!(task_id.starts_with("absorb-"));
            }
            other => panic!("expected TaskStarted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_returns_stream_started_for_query() {
        let state = Arc::new(DesktopState::new());
        let router = SkillRouter::new(state);
        match router
            .route("/query what is transformer", "session-1")
            .await
        {
            Some(SkillResult::StreamStarted { task_id }) => {
                assert!(task_id.starts_with("query-"));
            }
            other => panic!("expected StreamStarted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_returns_task_started_for_cleanup_and_patrol() {
        let state = Arc::new(DesktopState::new());
        let router = SkillRouter::new(state);
        for cmd in &["/cleanup", "/patrol"] {
            match router.route(cmd, "session-1").await {
                Some(SkillResult::TaskStarted { task_id }) => {
                    let prefix = cmd.trim_start_matches('/');
                    assert!(
                        task_id.starts_with(&format!("{prefix}-")),
                        "{cmd} → {task_id}"
                    );
                }
                other => panic!("{cmd} expected TaskStarted, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn route_returns_parse_error_for_bad_absorb_args() {
        let state = Arc::new(DesktopState::new());
        let router = SkillRouter::new(state);
        match router.route("/absorb notanumber", "session-1").await {
            Some(SkillResult::ParseError { message }) => {
                assert!(
                    message.contains("notanumber"),
                    "error must identify bad token: {message}"
                );
            }
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_returns_parse_error_for_empty_query() {
        let state = Arc::new(DesktopState::new());
        let router = SkillRouter::new(state);
        match router.route("/query", "session-1").await {
            Some(SkillResult::ParseError { .. }) => {}
            other => panic!("expected ParseError, got {other:?}"),
        }
    }
}

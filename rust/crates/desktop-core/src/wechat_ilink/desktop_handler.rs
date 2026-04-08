//! Bridge between inbound WeChat messages and the desktop `agentic_loop`.
//!
//! Replaces `EchoHandler` from Phase 2a. For each WeChat message:
//!
//!   1. Look up (or create) a desktop session dedicated to the WeChat user
//!   2. Subscribe to the session's broadcast channel BEFORE appending so we
//!      don't miss the assistant reply
//!   3. Call `DesktopState::append_user_message` to trigger the agentic loop
//!   4. Drain broadcast events until we see a `Snapshot` whose `turn_state`
//!      is `Idle` (signals turn completion)
//!   5. Concatenate all assistant text emitted between subscribe and idle
//!   6. Reply via `IlinkClient::send_message`
//!
//! Each inbound message is processed in its own tokio task so the long-poll
//! loop in `monitor.rs` is never blocked. Concurrent messages from the SAME
//! WeChat user are serialized by the desktop session's own busy-state guard
//! (`SessionBusy` is reported back to the user).
//!
//! Per-user session isolation is achieved by maintaining an
//! `openid → desktop_session_id` mapping persisted under
//! `wechat_ilink::account::openid_sessions_file_path`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, Mutex};
use tokio::time::timeout;

use super::account;
use super::client::IlinkClient;
use super::handlers::{build_text_reply, extract_first_text};
use super::monitor::{MessageHandler, MonitorError};
use super::types::WeixinMessage;

use crate::{
    CreateDesktopSessionRequest, DesktopContentBlock, DesktopConversationMessage,
    DesktopMessageRole, DesktopSessionDetail, DesktopSessionEvent, DesktopState,
    DesktopStateError, DesktopTurnState,
};

/// Maximum time we'll wait for an agentic turn to complete before giving up
/// and replying with an error. Must accommodate slow tools (Bash, Edit on
/// large files) plus LLM latency.
const TURN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Hard cap on assistant reply length sent to WeChat. Single iLink messages
/// have a body limit (~4 KB observed in practice); we cap at 3500 chars to
/// leave headroom for the JSON envelope and any future framing. Messages
/// longer than this are truncated with a "[truncated]" marker — Phase 3
/// will replace this with proper splitting into multiple sends.
const MAX_REPLY_CHARS: usize = 3500;

/// `MessageHandler` that bridges WeChat messages to the desktop `DesktopState`.
///
/// Cheap to clone — the `DesktopState` and the `Arc<Mutex<…>>` mapping cache
/// are both reference-counted internally.
#[derive(Clone)]
pub struct DesktopAgentHandler {
    state: DesktopState,
    /// Normalized account id of the bot this handler is bound to (e.g.
    /// `e0f2ee56e64d-im-bot`).
    account_id: String,
    /// Default project path for newly-created sessions. Defaults to the
    /// process cwd at server startup; can be overridden via env var
    /// `WECHAT_DEFAULT_PROJECT_PATH`.
    default_project_path: String,
    /// In-memory `openid → desktop_session_id` cache, hydrated from disk
    /// on construction. Wrapped in a Mutex so concurrent message handlers
    /// for different users serialize their reads-and-writes safely.
    mapping: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

impl DesktopAgentHandler {
    /// Build a new handler. Loads any persisted `openid → session` mapping
    /// from disk so existing conversations resume after a server restart.
    pub fn new(
        state: DesktopState,
        account_id: impl Into<String>,
        default_project_path: impl Into<String>,
    ) -> Result<Self, account::AccountError> {
        let account_id = account_id.into();
        let mapping = account::load_openid_sessions(&account_id)?;
        eprintln!(
            "[wechat agent] loaded {} persisted openid → session mappings",
            mapping.len()
        );
        Ok(Self {
            state,
            account_id,
            default_project_path: default_project_path.into(),
            mapping: Arc::new(Mutex::new(mapping)),
        })
    }

    /// Find an existing desktop session for `openid`, or create a fresh one
    /// if none exists. Returns the session id.
    async fn get_or_create_session(
        &self,
        openid: &str,
    ) -> Result<String, MonitorError> {
        // Fast path: in-memory cache hit
        {
            let map = self.mapping.lock().await;
            if let Some(existing) = map.get(openid) {
                // Verify the session still exists in DesktopState (it may
                // have been deleted from the desktop UI).
                if self.state.get_session(existing).await.is_ok() {
                    return Ok(existing.clone());
                }
            }
        }

        // Cache miss or stale entry — create a new session.
        let title = format!("WeChat · {}", short_openid(openid));
        let request = CreateDesktopSessionRequest {
            title: Some(title),
            project_name: None,
            project_path: Some(self.default_project_path.clone()),
        };
        let detail = self.state.create_session(request).await;
        let session_id = detail.id.clone();

        eprintln!(
            "[wechat agent] created session {session_id} for openid={openid}"
        );

        // Persist + cache.
        let mut map = self.mapping.lock().await;
        map.insert(openid.to_string(), session_id.clone());
        if let Err(e) =
            account::upsert_openid_session(&self.account_id, openid, &session_id)
        {
            eprintln!("[wechat agent] failed to persist mapping: {e}");
        }

        Ok(session_id)
    }

    /// Run a full turn for a single inbound user message and return the
    /// concatenated assistant text reply, or an error string if anything
    /// goes wrong (busy session, timeout, etc.).
    async fn run_turn(
        &self,
        session_id: &str,
        user_text: &str,
    ) -> Result<String, String> {
        // Subscribe FIRST so we don't miss the snapshot/messages emitted
        // while append_user_message is executing.
        let (_initial_snapshot, mut receiver) = match self.state.subscribe(session_id).await {
            Ok(pair) => pair,
            Err(e) => return Err(format!("subscribe failed: {e}")),
        };

        // Trigger the turn. If the session is already busy with a previous
        // turn, surface that to the user immediately rather than blocking.
        match self
            .state
            .append_user_message(session_id, user_text.to_string())
            .await
        {
            Ok(_) => {}
            Err(DesktopStateError::SessionBusy(_)) => {
                return Err("⏳ 上一条消息还在处理中，请稍后再试".to_string());
            }
            Err(e) => return Err(format!("append failed: {e}")),
        }

        // Drain events until we see Snapshot { turn_state: Idle }, which
        // means finalize_agentic_turn has fired. Collect any Message
        // events with role=assistant in the meantime.
        let mut collected_text = Vec::new();
        let started_at = Instant::now();

        loop {
            if started_at.elapsed() > TURN_TIMEOUT {
                return Err("⏳ 处理超时（10 分钟），请稍后重试".to_string());
            }

            // Cap each recv at the remaining budget so we don't overshoot.
            let remaining = TURN_TIMEOUT
                .checked_sub(started_at.elapsed())
                .unwrap_or(Duration::from_secs(1));

            let event = match timeout(remaining, receiver.recv()).await {
                Ok(Ok(event)) => event,
                Ok(Err(broadcast::error::RecvError::Lagged(skipped))) => {
                    eprintln!(
                        "[wechat agent] broadcast lagged, skipped {skipped} events"
                    );
                    continue;
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    return Err("session event stream closed unexpectedly".to_string());
                }
                Err(_) => {
                    return Err("⏳ 处理超时（10 分钟），请稍后重试".to_string());
                }
            };

            match event {
                DesktopSessionEvent::Message {
                    session_id: evt_sid,
                    message,
                } if evt_sid == session_id => {
                    if let Some(text) = extract_assistant_text(&message) {
                        collected_text.push(text);
                    }
                }
                DesktopSessionEvent::Snapshot { session } if session.id == session_id => {
                    if session.turn_state == DesktopTurnState::Idle {
                        // Turn finished. Check whether assistant produced anything.
                        if collected_text.is_empty() {
                            // No text emitted — likely the agent only ran tools
                            // and ended without a closing message, OR the loop
                            // errored. Try to read the latest assistant message
                            // from the session itself as a fallback.
                            if let Some(text) = latest_assistant_text(&session) {
                                return Ok(text);
                            }
                            return Err(
                                "(agent did not produce a text reply)".to_string()
                            );
                        }
                        return Ok(collected_text.join("\n"));
                    }
                }
                _ => {}
            }
        }
    }
}

#[async_trait::async_trait]
impl MessageHandler for DesktopAgentHandler {
    async fn on_message(
        &self,
        client: &IlinkClient,
        message: WeixinMessage,
    ) -> Result<(), MonitorError> {
        // Validate inbound shape upfront so the spawned task can assume
        // these fields are present.
        let from_user_id = match message.from_user_id.clone() {
            Some(id) if !id.is_empty() => id,
            _ => {
                eprintln!("[wechat agent] missing from_user_id, dropping");
                return Ok(());
            }
        };
        let context_token = match message.context_token.clone() {
            Some(t) if !t.is_empty() => t,
            _ => {
                eprintln!("[wechat agent] missing context_token, dropping");
                return Ok(());
            }
        };
        let user_text = match extract_first_text(&message) {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                // Non-text message (image/voice/file). For Phase 2b we don't
                // support these — reply with a hint and move on.
                let reply = build_text_reply(
                    &from_user_id,
                    &context_token,
                    "（暂不支持非文本消息，请发送文字）",
                );
                if let Err(e) = client.send_message(reply).await {
                    eprintln!("[wechat agent] reply send failed: {e}");
                }
                return Ok(());
            }
        };

        // Spawn the actual work in a background task so the long-poll loop
        // in monitor.rs returns to fetch the next message immediately.
        // This is critical: an agentic turn can take several minutes; the
        // monitor loop must NOT serialize on it.
        let handler = self.clone();
        let client = client.clone();
        tokio::spawn(async move {
            // Find or create a session for this WeChat user.
            let session_id = match handler.get_or_create_session(&from_user_id).await {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("[wechat agent] get_or_create_session failed: {e}");
                    let reply = build_text_reply(
                        &from_user_id,
                        &context_token,
                        &format!("⚠️ 创建会话失败: {e}"),
                    );
                    let _ = client.send_message(reply).await;
                    return;
                }
            };

            eprintln!(
                "[wechat agent] turn start: openid={from_user_id} session={session_id} text={:?}",
                truncate_for_log(&user_text)
            );

            let reply_text = match handler.run_turn(&session_id, &user_text).await {
                Ok(text) => text,
                Err(err_msg) => err_msg,
            };

            let truncated = if reply_text.chars().count() > MAX_REPLY_CHARS {
                let prefix: String = reply_text.chars().take(MAX_REPLY_CHARS).collect();
                format!("{prefix}\n\n[truncated]")
            } else {
                reply_text
            };

            let reply = build_text_reply(&from_user_id, &context_token, &truncated);
            if let Err(e) = client.send_message(reply).await {
                eprintln!("[wechat agent] reply send failed: {e}");
            } else {
                eprintln!(
                    "[wechat agent] turn end: openid={from_user_id} session={session_id}"
                );
            }
        });

        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Pull all text blocks out of an assistant `DesktopConversationMessage`.
/// Returns `None` if the message has no role/text content we want to
/// forward to WeChat (e.g. tool-call or tool-result rows).
fn extract_assistant_text(message: &DesktopConversationMessage) -> Option<String> {
    if message.role != DesktopMessageRole::Assistant {
        return None;
    }
    let mut parts = Vec::new();
    for block in &message.blocks {
        if let DesktopContentBlock::Text { text } = block {
            if !text.trim().is_empty() {
                parts.push(text.clone());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Fallback: read the most recent assistant message from a session detail
/// snapshot. Used when the broadcast missed events but the snapshot still
/// has the latest state.
fn latest_assistant_text(session: &DesktopSessionDetail) -> Option<String> {
    let messages = &session.session.messages;
    for msg in messages.iter().rev() {
        if msg.role == DesktopMessageRole::Assistant {
            let text: String = msg
                .blocks
                .iter()
                .filter_map(|b| {
                    if let DesktopContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Trim a long openid down to a recognizable suffix for use in session titles.
fn short_openid(openid: &str) -> String {
    let cleaned = openid.split('@').next().unwrap_or(openid);
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() <= 8 {
        cleaned.to_string()
    } else {
        chars[chars.len() - 8..].iter().collect()
    }
}

fn truncate_for_log(s: &str) -> String {
    let chars: Vec<char> = s.chars().take(80).collect();
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_openid_truncates_long_value() {
        assert_eq!(
            short_openid("o9cq80959YB4VrX7Fb2WF3YkFHPE@im.wechat"),
            "F3YkFHPE"
        );
    }

    #[test]
    fn short_openid_preserves_short_value() {
        assert_eq!(short_openid("alice@im.wechat"), "alice");
    }

    #[test]
    fn short_openid_handles_no_at() {
        assert_eq!(short_openid("xyz12345678"), "12345678");
    }

    #[test]
    fn truncate_for_log_caps_at_80() {
        let long: String = "a".repeat(200);
        let truncated = truncate_for_log(&long);
        assert_eq!(truncated.chars().count(), 80);
    }

    #[test]
    fn truncate_for_log_preserves_short() {
        assert_eq!(truncate_for_log("hi"), "hi");
    }
}

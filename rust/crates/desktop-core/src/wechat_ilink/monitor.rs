//! Long-poll loop that drives the WeChat iLink integration.
//!
//! Direct port of `~/.openclaw/extensions/openclaw-weixin/src/monitor/monitor.ts`.
//! Runs as a tokio background task and stays alive for the lifetime of the
//! desktop server. Each iteration:
//!
//!   1. Calls `IlinkClient::get_updates` with the persisted cursor
//!   2. Saves the new cursor to disk before processing messages (so a crash
//!      mid-handler doesn't replay the same batch)
//!   3. For each message, invokes the registered `MessageHandler`
//!   4. Backs off on consecutive failures, pauses on session-expired
//!
//! The handler trait is intentionally narrow so we can swap implementations
//! cheaply between Phase 2a (echo) and Phase 2b (real DesktopState bridge).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::account;
use super::client::{IlinkClient, IlinkError, SESSION_EXPIRED_ERRCODE};
use super::types::WeixinMessage;

/// Maximum consecutive failures before sleeping for `BACKOFF_DELAY`.
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
/// Long sleep applied after `MAX_CONSECUTIVE_FAILURES` is reached.
const BACKOFF_DELAY: Duration = Duration::from_secs(30);
/// Short retry delay between individual transient failures.
const RETRY_DELAY: Duration = Duration::from_secs(2);
/// Sleep applied when the server reports the bot session has expired.
/// The reference implementation uses 10 minutes; we mirror that.
const SESSION_EXPIRED_PAUSE: Duration = Duration::from_secs(10 * 60);

/// Trait implemented by the WeChat-side message handler.
///
/// `monitor` calls `on_message` for every inbound `WeixinMessage`. The
/// handler decides what to do (echo, dispatch to agent, log, ...) and is
/// responsible for sending any reply via `client`.
///
/// Implementations must be `Send + Sync` so they can live inside a
/// shared `Arc` across the long-poll task.
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Called once per inbound message. The `client` is borrowed by reference
    /// so the handler can call `send_message` / `send_typing` without taking
    /// ownership of the client.
    ///
    /// Errors are logged by the monitor but do not stop the loop.
    async fn on_message(
        &self,
        client: &IlinkClient,
        message: WeixinMessage,
    ) -> Result<(), MonitorError>;
}

/// Errors raised by the long-poll loop and message handlers.
#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error(transparent)]
    Ilink(#[from] IlinkError),
    #[error(transparent)]
    Account(#[from] account::AccountError),
    #[error("handler failed: {0}")]
    Handler(String),
}

/// Health snapshot the monitor publishes via a `watch::Receiver` so other
/// parts of the application (HTTP status endpoints, frontend UI) can read
/// the connection state without holding the loop's internal lock.
///
/// M5: the `last_ingest_unix_ms`, `processed_msg_count`, and
/// `dedupe_hit_count` fields are filled in by the handler (not the
/// monitor loop) via the health endpoint layer — the monitor always
/// publishes them as `None`/`0`. `serde(default)` is declared so any
/// future persistent snapshot can round-trip older payloads missing
/// the new fields without breaking.
#[derive(Debug, Clone, Default)]
pub struct MonitorStatus {
    pub running: bool,
    pub last_poll_unix_ms: Option<i64>,
    pub last_inbound_unix_ms: Option<i64>,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    /// Time (unix ms) of the most recent successful ingest. Populated
    /// by the HTTP health handler from the dedupe store's per-channel
    /// stats, not by the monitor loop itself.
    pub last_ingest_unix_ms: Option<i64>,
    /// Lifetime count of messages that cleared dedupe and reached the
    /// ingest path. Backed by the process-global [`crate::wechat_ilink::DedupeStore`].
    pub processed_msg_count: u64,
    /// Lifetime count of inbound events the dedupe layer short-circuited.
    pub dedupe_hit_count: u64,
}

/// Configuration for a monitor instance.
pub struct MonitorConfig {
    /// Account id (normalized form, e.g. `e0f2ee56e64d-im-bot`).
    pub account_id: String,
    pub client: IlinkClient,
    pub handler: Arc<dyn MessageHandler>,
    pub cancel: CancellationToken,
}

/// Run the long-poll loop until `cancel` is triggered.
///
/// Designed to be invoked from `tokio::spawn` and lived as a background task.
/// Returns when the cancellation token fires.
pub async fn run_monitor(config: MonitorConfig, status_tx: watch::Sender<MonitorStatus>) {
    let MonitorConfig {
        account_id,
        client,
        handler,
        cancel,
    } = config;

    let _ = status_tx.send(MonitorStatus {
        running: true,
        ..Default::default()
    });

    // Restore cursor from disk so we don't replay messages across restarts.
    let mut cursor = match account::load_sync_buf(&account_id) {
        Ok(buf) => buf,
        Err(e) => {
            eprintln!("[wechat monitor] failed to load sync buf: {e}; starting fresh");
            String::new()
        }
    };

    if cursor.is_empty() {
        eprintln!("[wechat monitor] no previous cursor; starting fresh");
    } else {
        eprintln!(
            "[wechat monitor] resuming from cursor ({} bytes)",
            cursor.len()
        );
    }

    let mut consecutive_failures: u32 = 0;
    let mut next_timeout: Option<Duration> = None;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Race the long-poll against the cancellation token so we can shut
        // down promptly without waiting for the 35s timeout.
        let updates = tokio::select! {
            _ = cancel.cancelled() => break,
            updates = client.get_updates(&cursor, next_timeout) => updates,
        };

        match updates {
            Ok(resp) => {
                // Refresh server-suggested timeout for next iteration.
                if let Some(ms) = resp.longpolling_timeout_ms {
                    if ms > 0 {
                        next_timeout = Some(Duration::from_millis(ms as u64));
                    }
                }

                // Surface API-layer errors (`ret`/`errcode`).
                let api_err = matches!(resp.ret, Some(n) if n != 0)
                    || matches!(resp.errcode, Some(n) if n != 0);
                if api_err {
                    let is_session_expired = matches!(
                        resp.errcode,
                        Some(n) if n == SESSION_EXPIRED_ERRCODE
                    ) || matches!(
                        resp.ret,
                        Some(n) if n == SESSION_EXPIRED_ERRCODE
                    );

                    if is_session_expired {
                        eprintln!(
                            "[wechat monitor] session expired (errcode {}), pausing for {} min",
                            SESSION_EXPIRED_ERRCODE,
                            SESSION_EXPIRED_PAUSE.as_secs() / 60
                        );
                        let _ = status_tx.send(MonitorStatus {
                            running: true,
                            consecutive_failures: 0,
                            last_error: Some("session expired".to_string()),
                            ..Default::default()
                        });
                        sleep_or_cancel(SESSION_EXPIRED_PAUSE, &cancel).await;
                        continue;
                    }

                    consecutive_failures += 1;
                    eprintln!(
                        "[wechat monitor] getUpdates api err: ret={:?} errcode={:?} errmsg={:?} ({}/{})",
                        resp.ret,
                        resp.errcode,
                        resp.errmsg,
                        consecutive_failures,
                        MAX_CONSECUTIVE_FAILURES
                    );
                    let _ = status_tx.send(MonitorStatus {
                        running: true,
                        consecutive_failures,
                        last_error: resp
                            .errmsg
                            .clone()
                            .or_else(|| Some(format!("ret={:?}", resp.ret))),
                        ..Default::default()
                    });

                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        eprintln!(
                            "[wechat monitor] {MAX_CONSECUTIVE_FAILURES} consecutive failures, backing off {}s",
                            BACKOFF_DELAY.as_secs()
                        );
                        consecutive_failures = 0;
                        sleep_or_cancel(BACKOFF_DELAY, &cancel).await;
                    } else {
                        sleep_or_cancel(RETRY_DELAY, &cancel).await;
                    }
                    continue;
                }

                consecutive_failures = 0;
                let now = unix_ms();
                let _ = status_tx.send(MonitorStatus {
                    running: true,
                    last_poll_unix_ms: Some(now),
                    consecutive_failures: 0,
                    ..Default::default()
                });

                // Persist the new cursor BEFORE processing messages so a
                // crash mid-handler doesn't cause us to replay them.
                if let Some(new_cursor) = resp.get_updates_buf {
                    if !new_cursor.is_empty() && new_cursor != cursor {
                        if let Err(e) = account::save_sync_buf(&account_id, &new_cursor) {
                            eprintln!("[wechat monitor] failed to persist cursor: {e}");
                        }
                        cursor = new_cursor;
                    }
                }

                let messages = resp.msgs.unwrap_or_default();
                for msg in messages {
                    let from = msg.from_user_id.clone().unwrap_or_default();
                    let summary = describe_message(&msg);
                    eprintln!("[wechat monitor] inbound from={from} summary={summary}");

                    let now = unix_ms();
                    let _ = status_tx.send(MonitorStatus {
                        running: true,
                        last_poll_unix_ms: Some(now),
                        last_inbound_unix_ms: Some(now),
                        consecutive_failures: 0,
                        ..Default::default()
                    });

                    // Update the persisted context_token cache so future
                    // outbound messages from us can find the right thread.
                    if let (Some(uid), Some(ctx)) =
                        (msg.from_user_id.as_deref(), msg.context_token.as_deref())
                    {
                        let _ = account::upsert_context_token(&account_id, uid, ctx);
                    }

                    // Hand off to the handler. We deliberately swallow
                    // handler errors here so a single bad message doesn't
                    // tank the long-poll loop.
                    if let Err(e) = handler.on_message(&client, msg).await {
                        eprintln!("[wechat monitor] handler error: {e}");
                    }
                }
            }
            Err(e) => {
                if cancel.is_cancelled() {
                    break;
                }
                consecutive_failures += 1;
                eprintln!(
                    "[wechat monitor] getUpdates network err: {e} ({}/{})",
                    consecutive_failures, MAX_CONSECUTIVE_FAILURES
                );
                let _ = status_tx.send(MonitorStatus {
                    running: true,
                    consecutive_failures,
                    last_error: Some(e.to_string()),
                    ..Default::default()
                });
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!(
                        "[wechat monitor] {MAX_CONSECUTIVE_FAILURES} consecutive failures, backing off {}s",
                        BACKOFF_DELAY.as_secs()
                    );
                    consecutive_failures = 0;
                    sleep_or_cancel(BACKOFF_DELAY, &cancel).await;
                } else {
                    sleep_or_cancel(RETRY_DELAY, &cancel).await;
                }
            }
        }
    }

    eprintln!("[wechat monitor] stopped (cancelled)");
    let _ = status_tx.send(MonitorStatus {
        running: false,
        ..Default::default()
    });
}

/// Sleep for `duration`, but return early if the cancellation token fires.
async fn sleep_or_cancel(duration: Duration, cancel: &CancellationToken) {
    tokio::select! {
        _ = tokio::time::sleep(duration) => {}
        _ = cancel.cancelled() => {}
    }
}

fn unix_ms() -> i64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Best-effort one-line summary of a message for logging.
fn describe_message(msg: &WeixinMessage) -> String {
    let Some(items) = msg.item_list.as_ref() else {
        return "<empty>".to_string();
    };
    let mut parts = Vec::new();
    for item in items {
        match item.r#type {
            Some(t) if t == super::types::message_item_type::TEXT => {
                let text = item
                    .text_item
                    .as_ref()
                    .and_then(|t| t.text.as_deref())
                    .unwrap_or("");
                let snip: String = text.chars().take(60).collect();
                parts.push(format!("text(\"{snip}\")"));
            }
            Some(t) if t == super::types::message_item_type::IMAGE => parts.push("image".into()),
            Some(t) if t == super::types::message_item_type::VOICE => {
                let transcript = item
                    .voice_item
                    .as_ref()
                    .and_then(|v| v.text.as_deref())
                    .unwrap_or("");
                let snip: String = transcript.chars().take(40).collect();
                parts.push(format!("voice(\"{snip}\")"));
            }
            Some(t) if t == super::types::message_item_type::FILE => parts.push("file".into()),
            Some(t) if t == super::types::message_item_type::VIDEO => parts.push("video".into()),
            other => parts.push(format!("type={other:?}")),
        }
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::super::types::{message_item_type, MessageItem, TextItem, WeixinMessage};
    use super::*;

    #[test]
    fn describe_text_message() {
        let msg = WeixinMessage {
            item_list: Some(vec![MessageItem {
                r#type: Some(message_item_type::TEXT),
                text_item: Some(TextItem {
                    text: Some("hello world".to_string()),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert_eq!(describe_message(&msg), "text(\"hello world\")");
    }

    #[test]
    fn describe_text_truncates_long() {
        let long: String = "a".repeat(120);
        let msg = WeixinMessage {
            item_list: Some(vec![MessageItem {
                r#type: Some(message_item_type::TEXT),
                text_item: Some(TextItem { text: Some(long) }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let summary = describe_message(&msg);
        // 60 chars + the wrapper text(""…")
        assert!(summary.len() < 80);
        assert!(summary.starts_with("text(\""));
    }

    #[test]
    fn describe_empty_message() {
        let msg = WeixinMessage::default();
        assert_eq!(describe_message(&msg), "<empty>");
    }
}

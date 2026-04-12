//! Dual-mode message monitor for the WeChat Customer Service channel.
//!
//! Primary: callback-triggered sync_msg (instant, with event token)
//! Fallback: periodic polling sync_msg (rate-limited, without token)

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use super::callback::CallbackEvent;
use super::client::KefuClient;
use crate::wechat_ilink::MonitorStatus;

/// Fallback polling interval when no callback event arrives.
/// Must be long (30s+) to avoid errcode=45009 rate limiting
/// when calling sync_msg without the callback token.
const POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
const BACKOFF_DELAY: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(2);
const SYNC_MSG_LIMIT: u32 = 100;

/// Message handler trait for kefu messages.
#[async_trait::async_trait]
pub trait KefuMessageHandler: Send + Sync {
    async fn on_message(
        &self,
        client: &KefuClient,
        msg: &serde_json::Value,
        open_kfid: &str,
    );
}

/// Configuration for the kefu monitor.
pub struct KefuMonitorConfig {
    pub client: KefuClient,
    pub open_kfid: String,
    pub handler: Arc<dyn KefuMessageHandler>,
    pub cancel: CancellationToken,
    pub callback_rx: mpsc::Receiver<CallbackEvent>,
}

/// Run the dual-mode kefu monitor until cancelled.
pub async fn run_kefu_monitor(
    config: KefuMonitorConfig,
    status_tx: watch::Sender<MonitorStatus>,
) {
    let KefuMonitorConfig {
        client,
        open_kfid,
        handler,
        cancel,
        mut callback_rx,
    } = config;

    let _ = status_tx.send(MonitorStatus {
        running: true,
        ..Default::default()
    });

    let mut cursor = super::account::load_cursor().unwrap_or_default();
    if cursor.is_empty() {
        eprintln!("[kefu monitor] no previous cursor; starting fresh");
    } else {
        eprintln!(
            "[kefu monitor] resuming from cursor ({} bytes)",
            cursor.len()
        );
    }

    let mut consecutive_failures: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Wait for callback event OR poll timeout
        let event_token = tokio::select! {
            _ = cancel.cancelled() => break,
            event = callback_rx.recv() => {
                match event {
                    Some(CallbackEvent::MsgReceive { token }) => {
                        eprintln!("[kefu monitor] callback: msg_receive");
                        Some(token)
                    }
                    Some(CallbackEvent::EnterSession { .. }) => {
                        eprintln!("[kefu monitor] callback: enter_session");
                        None
                    }
                    Some(CallbackEvent::Other(t)) => {
                        eprintln!("[kefu monitor] callback: {t}");
                        continue;
                    }
                    None => {
                        eprintln!("[kefu monitor] callback channel closed");
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(POLL_INTERVAL) => None,
        };

        match client
            .sync_msg(&cursor, event_token.as_deref(), SYNC_MSG_LIMIT)
            .await
        {
            Ok(resp) => {
                consecutive_failures = 0;
                let now = unix_ms();
                let _ = status_tx.send(MonitorStatus {
                    running: true,
                    last_poll_unix_ms: Some(now),
                    consecutive_failures: 0,
                    ..Default::default()
                });

                // Persist cursor (crash-safe)
                if let Some(next) = &resp.next_cursor {
                    if !next.is_empty() && *next != cursor {
                        if let Err(e) = super::account::save_cursor(next) {
                            eprintln!("[kefu monitor] cursor persist failed: {e}");
                        }
                        cursor = next.clone();
                    }
                }

                for msg in resp.msg_list {
                    let now = unix_ms();
                    let _ = status_tx.send(MonitorStatus {
                        running: true,
                        last_poll_unix_ms: Some(now),
                        last_inbound_unix_ms: Some(now),
                        consecutive_failures: 0,
                        ..Default::default()
                    });

                    let msgtype = msg.get("msgtype").and_then(|v| v.as_str()).unwrap_or("?");
                    let msgid = msg.get("msgid").and_then(|v| v.as_str()).unwrap_or("?");
                    eprintln!("[kefu monitor] inbound msgid={msgid} type={msgtype}");

                    handler.on_message(&client, &msg, &open_kfid).await;
                }
            }
            Err(e) => {
                if cancel.is_cancelled() {
                    break;
                }
                consecutive_failures += 1;
                eprintln!(
                    "[kefu monitor] sync_msg error: {e} ({consecutive_failures}/{MAX_CONSECUTIVE_FAILURES})"
                );
                let _ = status_tx.send(MonitorStatus {
                    running: true,
                    consecutive_failures,
                    last_error: Some(e.to_string()),
                    ..Default::default()
                });
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    consecutive_failures = 0;
                    sleep_or_cancel(BACKOFF_DELAY, &cancel).await;
                } else {
                    sleep_or_cancel(RETRY_DELAY, &cancel).await;
                }
            }
        }
    }

    eprintln!("[kefu monitor] stopped");
    let _ = status_tx.send(MonitorStatus {
        running: false,
        ..Default::default()
    });
}

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

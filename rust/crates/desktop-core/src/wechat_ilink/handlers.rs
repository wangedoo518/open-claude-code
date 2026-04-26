//! Concrete `MessageHandler` implementations for the WeChat iLink monitor.
//!
//! Phase 2a ships a single handler — `EchoHandler` — which mirrors any
//! inbound text back to the sender. It's only used for protocol-level
//! verification ("can we receive a message and reply to it?") and will
//! be removed in Phase 2b once the real DesktopState bridge lands.

use super::client::IlinkClient;
use super::monitor::{MessageHandler, MonitorError};
use super::types::{
    message_item_type, message_state, message_type, MessageItem, TextItem, WeixinMessage,
};

/// Reply with the user's own message verbatim, prefixed with "echo: ".
///
/// This is the simplest possible handler — any user message round-trips
/// through the iLink protocol so we can verify get_updates → send_message
/// works end-to-end before plugging in the real agent.
pub struct EchoHandler;

#[async_trait::async_trait]
impl MessageHandler for EchoHandler {
    async fn on_message(
        &self,
        client: &IlinkClient,
        message: WeixinMessage,
    ) -> Result<(), MonitorError> {
        // Pull the first text item out of the inbound message. We ignore
        // images / voice / files for echo (they'd need media handling).
        let text = extract_first_text(&message);
        let from_user_id = match message.from_user_id.as_ref() {
            Some(id) if !id.is_empty() => id.clone(),
            _ => {
                eprintln!("[echo] inbound message missing from_user_id, skipping");
                return Ok(());
            }
        };
        let context_token = match message.context_token.as_ref() {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                eprintln!("[echo] inbound message missing context_token, cannot reply");
                return Ok(());
            }
        };

        let reply_text = match text {
            Some(t) if !t.trim().is_empty() => format!("echo: {t}"),
            _ => "echo: (received non-text message)".to_string(),
        };

        let reply = build_text_reply(&from_user_id, &context_token, &reply_text);
        client
            .send_message(reply)
            .await
            .map_err(MonitorError::Ilink)?;
        Ok(())
    }
}

/// Extract the concatenated text content of all `TEXT` items in a message.
/// Returns `None` if the message contains no text items at all.
pub(crate) fn extract_first_text(message: &WeixinMessage) -> Option<String> {
    let items = message.item_list.as_ref()?;
    let mut parts = Vec::new();
    for item in items {
        if item.r#type == Some(message_item_type::TEXT) {
            if let Some(text) = item.text_item.as_ref().and_then(|t| t.text.as_deref()) {
                parts.push(text.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Build a `WeixinMessage` envelope for an outbound text reply. The caller
/// is responsible for supplying the original message's `context_token` —
/// reusing a stale token would break the conversation thread.
pub(crate) fn build_text_reply(to_user_id: &str, context_token: &str, text: &str) -> WeixinMessage {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    WeixinMessage {
        to_user_id: Some(to_user_id.to_string()),
        client_id: Some(format!("warwolf-{}-{}", now_ms, random_hex_suffix())),
        message_type: Some(message_type::BOT),
        message_state: Some(message_state::FINISH),
        context_token: Some(context_token.to_string()),
        item_list: Some(vec![MessageItem {
            r#type: Some(message_item_type::TEXT),
            text_item: Some(TextItem {
                text: Some(text.to_string()),
            }),
            ..Default::default()
        }]),
        ..Default::default()
    }
}

fn random_hex_suffix() -> String {
    use rand::RngCore as _;
    let mut buf = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::super::types::{
        message_item_type, message_state, message_type, MessageItem, TextItem, WeixinMessage,
    };
    use super::*;

    #[test]
    fn extract_first_text_simple() {
        let msg = WeixinMessage {
            item_list: Some(vec![MessageItem {
                r#type: Some(message_item_type::TEXT),
                text_item: Some(TextItem {
                    text: Some("hello".to_string()),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert_eq!(extract_first_text(&msg).as_deref(), Some("hello"));
    }

    #[test]
    fn extract_first_text_concatenates_multiple() {
        let msg = WeixinMessage {
            item_list: Some(vec![
                MessageItem {
                    r#type: Some(message_item_type::TEXT),
                    text_item: Some(TextItem {
                        text: Some("line 1".to_string()),
                    }),
                    ..Default::default()
                },
                MessageItem {
                    r#type: Some(message_item_type::TEXT),
                    text_item: Some(TextItem {
                        text: Some("line 2".to_string()),
                    }),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        assert_eq!(extract_first_text(&msg).as_deref(), Some("line 1\nline 2"));
    }

    #[test]
    fn extract_first_text_skips_non_text() {
        let msg = WeixinMessage {
            item_list: Some(vec![MessageItem {
                r#type: Some(message_item_type::IMAGE),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert_eq!(extract_first_text(&msg), None);
    }

    #[test]
    fn extract_first_text_empty_returns_none() {
        let msg = WeixinMessage::default();
        assert_eq!(extract_first_text(&msg), None);
    }

    #[test]
    fn build_text_reply_sets_required_fields() {
        let reply = build_text_reply("user@im.wechat", "ctx-tok", "hi back");
        assert_eq!(reply.to_user_id.as_deref(), Some("user@im.wechat"));
        assert_eq!(reply.context_token.as_deref(), Some("ctx-tok"));
        assert_eq!(reply.message_type, Some(message_type::BOT));
        assert_eq!(reply.message_state, Some(message_state::FINISH));
        assert!(reply
            .client_id
            .as_deref()
            .map(|s| s.starts_with("warwolf-"))
            .unwrap_or(false));
        let items = reply.item_list.as_ref().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].r#type, Some(message_item_type::TEXT));
        assert_eq!(
            items[0].text_item.as_ref().and_then(|t| t.text.as_deref()),
            Some("hi back")
        );
    }

    #[test]
    fn random_hex_suffix_uniqueness() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(random_hex_suffix());
        }
        assert!(seen.len() >= 49);
    }
}

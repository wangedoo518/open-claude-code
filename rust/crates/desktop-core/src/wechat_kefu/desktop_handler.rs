//! Bridge from WeChat Customer Service messages to the ClaudeWiki turn pipeline.
//!
//! Pattern mirrors `wechat_ilink/desktop_handler.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::client::KefuClient;
use super::monitor::KefuMessageHandler;
use crate::wechat_ilink::markdown_split::split_markdown_for_wechat;
use crate::{
    CreateDesktopSessionRequest, DesktopContentBlock, DesktopConversationMessage,
    DesktopMessageRole, DesktopSessionEvent, DesktopState, DesktopTurnState,
};

const DEFAULT_MAX_CHARS: usize = 3000;
const TURN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const INTER_CHUNK_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

/// Handler that bridges kefu messages into ClaudeWiki desktop sessions.
pub struct KefuDesktopHandler {
    state: DesktopState,
    default_project_path: String,
    mapping: Arc<Mutex<HashMap<String, String>>>,
    wiki_paths: Option<wiki_store::WikiPaths>,
}

impl KefuDesktopHandler {
    pub fn new(
        state: DesktopState,
        default_project_path: impl Into<String>,
    ) -> Self {
        let mapping = super::account::load_session_map().unwrap_or_default();
        eprintln!(
            "[kefu handler] loaded {} persisted session mappings",
            mapping.len()
        );
        Self {
            state,
            default_project_path: default_project_path.into(),
            mapping: Arc::new(Mutex::new(mapping)),
            wiki_paths: None,
        }
    }

    pub fn with_wiki_paths(mut self, paths: wiki_store::WikiPaths) -> Self {
        self.wiki_paths = Some(paths);
        self
    }

    async fn get_or_create_session(
        &self,
        external_userid: &str,
    ) -> Result<String, String> {
        let session_key = format!("kefu:{external_userid}");

        // Fast path: cache hit
        {
            let map = self.mapping.lock().await;
            if let Some(existing) = map.get(&session_key) {
                if self.state.get_session(existing).await.is_ok() {
                    return Ok(existing.clone());
                }
            }
        }

        // Create new session
        let short_id: String = external_userid.chars().take(12).collect();
        let title = format!("客服 · {short_id}");
        let request = CreateDesktopSessionRequest {
            title: Some(title),
            project_name: None,
            project_path: Some(self.default_project_path.clone()),
        };
        let detail = self.state.create_session(request).await;
        let session_id = detail.id.clone();

        let mut map = self.mapping.lock().await;
        map.insert(session_key, session_id.clone());
        let _ = super::account::upsert_session(external_userid, &session_id);
        eprintln!("[kefu handler] created session {session_id} for {external_userid}");
        Ok(session_id)
    }

    async fn run_turn(
        &self,
        session_id: &str,
        user_text: &str,
    ) -> Result<String, String> {
        let (_snapshot, mut rx) = self
            .state
            .subscribe(session_id)
            .await
            .map_err(|e| format!("subscribe failed: {e:?}"))?;

        if let Err(e) = self
            .state
            .append_user_message(session_id, user_text.to_string())
            .await
        {
            match e {
                crate::DesktopStateError::SessionBusy(_) => {
                    return Err("上一条消息还在处理中，请稍后再试".to_string());
                }
                other => return Err(format!("append failed: {other:?}")),
            }
        }

        let deadline = tokio::time::Instant::now() + TURN_TIMEOUT;
        let mut collected = Vec::new();

        loop {
            let remaining =
                deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            let event = tokio::select! {
                e = rx.recv() => match e {
                    Ok(e) => e,
                    Err(_) => break,
                },
                _ = tokio::time::sleep(remaining) => break,
            };

            match &event {
                DesktopSessionEvent::Message {
                    session_id: sid,
                    message,
                } if sid == session_id => {
                    if let Some(text) = extract_assistant_text(message) {
                        collected.push(text);
                    }
                }
                DesktopSessionEvent::Snapshot { session }
                    if session.id == session_id =>
                {
                    if session.turn_state == DesktopTurnState::Idle {
                        if collected.is_empty() {
                            if let Some(text) =
                                latest_assistant_text(session)
                            {
                                return Ok(text);
                            }
                        }
                        break;
                    }
                }
                _ => {}
            }
        }

        if collected.is_empty() {
            return Err("no assistant reply received".to_string());
        }
        Ok(collected.join("\n"))
    }
}

#[async_trait::async_trait]
impl KefuMessageHandler for KefuDesktopHandler {
    async fn on_message(
        &self,
        client: &KefuClient,
        msg: &serde_json::Value,
        open_kfid: &str,
    ) {
        // Only process customer messages (origin=3)
        let origin = msg.get("origin").and_then(|v| v.as_u64()).unwrap_or(0);
        if origin != 3 {
            return;
        }

        let external_userid = match msg.get("external_userid").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => {
                eprintln!("[kefu handler] missing external_userid");
                return;
            }
        };

        let msgtype = msg.get("msgtype").and_then(|v| v.as_str()).unwrap_or("unknown");

        // Extract text content
        let user_text = match msgtype {
            "text" => msg
                .get("text")
                .and_then(|t| t.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string(),
            "link" => {
                // Extract link title + desc as text input
                let title = msg.get("link").and_then(|l| l.get("title")).and_then(|t| t.as_str()).unwrap_or("");
                let desc = msg.get("link").and_then(|l| l.get("desc")).and_then(|d| d.as_str()).unwrap_or("");
                let url = msg.get("link").and_then(|l| l.get("url")).and_then(|u| u.as_str()).unwrap_or("");
                format!("{title}\n{desc}\n{url}")
            }
            "event" => return,
            other => {
                eprintln!("[kefu handler] unsupported msgtype={other}");
                let _ = client
                    .send_text(
                        &external_userid,
                        open_kfid,
                        &format!("（暂不支持{other}类型消息，请发送文字或链接）"),
                    )
                    .await;
                return;
            }
        };

        if user_text.trim().is_empty() {
            return;
        }

        eprintln!(
            "[kefu handler] text from {} ({} chars)",
            &external_userid[..8.min(external_userid.len())],
            user_text.len()
        );

        // Wiki ingest (best-effort)
        if let Some(paths) = &self.wiki_paths {
            let paths = paths.clone();
            let text = user_text.clone();
            let uid = external_userid.clone();
            tokio::task::spawn_blocking(move || {
                ingest_kefu_text_to_wiki(&paths, &uid, &text);
            });
        }

        // Get or create session
        let session_id = match self.get_or_create_session(&external_userid).await {
            Ok(id) => id,
            Err(e) => {
                eprintln!("[kefu handler] session error: {e}");
                let _ = client.send_text(&external_userid, open_kfid, &e).await;
                return;
            }
        };

        // Execute turn
        match self.run_turn(&session_id, &user_text).await {
            Ok(reply) => {
                let chunks = split_markdown_for_wechat(&reply, DEFAULT_MAX_CHARS);
                for (i, chunk) in chunks.iter().enumerate() {
                    if i > 0 {
                        tokio::time::sleep(INTER_CHUNK_DELAY).await;
                    }
                    if let Err(e) = client
                        .send_text(&external_userid, open_kfid, chunk)
                        .await
                    {
                        eprintln!("[kefu handler] send_text error: {e}");
                        break;
                    }
                }
                eprintln!(
                    "[kefu handler] replied ({} chars, {} chunks)",
                    reply.len(),
                    chunks.len()
                );
            }
            Err(e) => {
                eprintln!("[kefu handler] turn error: {e}");
                let _ = client.send_text(&external_userid, open_kfid, &e).await;
            }
        }
    }
}

fn extract_assistant_text(msg: &DesktopConversationMessage) -> Option<String> {
    if msg.role != DesktopMessageRole::Assistant {
        return None;
    }
    let mut parts = Vec::new();
    for block in &msg.blocks {
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

fn latest_assistant_text(session: &crate::DesktopSessionDetail) -> Option<String> {
    for msg in session.session.messages.iter().rev() {
        if msg.role == DesktopMessageRole::Assistant {
            if let Some(text) = extract_assistant_text(msg) {
                return Some(text);
            }
        }
    }
    None
}

fn ingest_kefu_text_to_wiki(
    paths: &wiki_store::WikiPaths,
    external_userid: &str,
    user_text: &str,
) {
    let short_id = &external_userid[..8.min(external_userid.len())];
    let slug = format!("kefu-{short_id}");
    let frontmatter = wiki_store::RawFrontmatter::for_paste("kefu-text", None);
    let entry = match wiki_store::write_raw_entry(
        paths,
        "kefu-text",
        &slug,
        user_text,
        &frontmatter,
    ) {
        Ok(entry) => entry,
        Err(err) => {
            eprintln!("[kefu handler] wiki write_raw_entry failed: {err}");
            return;
        }
    };
    let origin = format!("WeChat kefu user `{short_id}`");
    if let Err(err) = wiki_store::append_new_raw_task(paths, &entry, &origin) {
        eprintln!("[kefu handler] inbox append failed: {err}");
    }
}

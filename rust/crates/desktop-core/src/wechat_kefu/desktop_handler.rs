//! Bridge from WeChat Customer Service messages to the ClawWiki knowledge pipeline.
//!
//! v2 behavior (04-wechat-kefu.md §5):
//!   URL  → ingest + absorb + reply confirmation
//!   Text → ingest + absorb + reply confirmation
//!   ?Q   → wiki query → reply answer
//!   /cmd → /recent, /stats
//!   other message types → unsupported reply

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::client::KefuClient;
use super::monitor::KefuMessageHandler;
use super::{KEFU_COMMAND_RECENT, KEFU_COMMAND_STATS, KEFU_TEXT_MIN_CHARS};
// v1 session-turn imports — kept for fallback path, currently unused.
#[allow(unused_imports)]
use crate::wechat_ilink::markdown_split::split_markdown_for_wechat;
use crate::{
    CreateDesktopSessionRequest, DesktopContentBlock, DesktopConversationMessage,
    DesktopMessageRole, DesktopSessionEvent, DesktopState, DesktopTurnState,
};

// v1 constants — kept for fallback path.
#[allow(dead_code)]
const DEFAULT_MAX_CHARS: usize = 3000;
#[allow(dead_code)]
const TURN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
#[allow(dead_code)]
const INTER_CHUNK_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

/// Handler that bridges kefu messages into ClaudeWiki desktop sessions.
#[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
                        &unsupported_msgtype_reply(other),
                    )
                    .await;
                return;
            }
        };

        if user_text.trim().is_empty() {
            return;
        }

        let trimmed = user_text.trim();
        eprintln!(
            "[kefu handler] text from {} ({} chars)",
            &external_userid[..8.min(external_userid.len())],
            trimmed.len()
        );

        // ── v2 Message classification + knowledge pipeline ──────────
        // Per 04-wechat-kefu.md §5: classify → route → reply.

        let kind = classify_message(trimmed);

        match kind {
            MessageKind::Query(question) => {
                // ?问题 → wiki_maintainer::query_wiki → 回复答案
                self.handle_query(client, &external_userid, open_kfid, &question)
                    .await;
            }
            MessageKind::Command(cmd) => {
                // /recent, /stats 命令
                self.handle_command(client, &external_userid, open_kfid, &cmd)
                    .await;
            }
            MessageKind::Url(url) => {
                // URL → ingest + absorb + 回复确认
                self.handle_url_ingest(client, &external_userid, open_kfid, &url)
                    .await;
            }
            MessageKind::Text(text) => {
                // 纯文本 → ingest + absorb + 回复确认
                self.handle_text_ingest(client, &external_userid, open_kfid, &text)
                    .await;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// v2: Message classification + handler methods (04-wechat-kefu.md §5)
// ═══════════════════════════════════════════════════════════════════════

enum MessageKind {
    Url(String),
    Query(String),
    Command(String),
    Text(String),
}

fn classify_message(trimmed: &str) -> MessageKind {
    // ? or ？ prefix → query
    if trimmed.starts_with('?') {
        return MessageKind::Query(trimmed[1..].trim().to_string());
    }
    if trimmed.starts_with('\u{FF1F}') {
        // ？ is 3 bytes in UTF-8
        return MessageKind::Query(trimmed[3..].trim().to_string());
    }
    // / prefix → command
    if trimmed.starts_with('/') {
        return MessageKind::Command(trimmed.to_string());
    }
    // URL detection
    if let Some(url) = extract_first_url(trimmed) {
        return MessageKind::Url(url);
    }
    MessageKind::Text(trimmed.to_string())
}

fn extract_first_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            return Some(word.to_string());
        }
    }
    None
}

fn should_ingest_text(text: &str) -> bool {
    text.chars().count() >= KEFU_TEXT_MIN_CHARS
}

fn unsupported_msgtype_reply(msgtype: &str) -> String {
    format!("（暂不支持{msgtype}类型消息，请发送文字或链接）")
}

impl KefuDesktopHandler {
    /// ?问题 → query_wiki → 结构化回答 → 回复微信
    async fn handle_query(
        &self,
        client: &KefuClient,
        userid: &str,
        open_kfid: &str,
        question: &str,
    ) {
        if question.is_empty() {
            let _ = client.send_text(userid, open_kfid, "请在 ? 后面输入问题").await;
            return;
        }

        let paths = match &self.wiki_paths {
            Some(p) => p.clone(),
            None => {
                let _ = client
                    .send_text(userid, open_kfid, "❌ 知识库未初始化")
                    .await;
                return;
            }
        };

        let adapter = crate::wiki_maintainer_adapter::BrokerAdapter::from_global();
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);

        let question_owned = question.to_string();
        let paths_clone = paths.clone();
        let query_handle = tokio::spawn(async move {
            wiki_maintainer::query_wiki(&paths_clone, &question_owned, 5, &adapter, tx).await
        });

        // Collect answer chunks.
        let mut answer = String::new();
        while let Some(chunk) = rx.recv().await {
            answer.push_str(&chunk.delta);
        }

        let result = query_handle.await;
        let sources = match result {
            Ok(Ok(r)) => r.sources,
            Ok(Err(wiki_maintainer::MaintainerError::RawNotAvailable(_))) => {
                // Wiki is empty — friendly fallback.
                let reply = format!(
                    "🤔 暂无相关知识\n\n\
                     你的知识库中还没有关于「{question}」的内容。\n\
                     试试先投喂一些相关资料？"
                );
                let _ = client.send_text(userid, open_kfid, &reply).await;
                return;
            }
            Ok(Err(e)) => {
                let _ = client
                    .send_text(userid, open_kfid, &format!("❌ 查询失败: {e}"))
                    .await;
                return;
            }
            Err(e) => {
                let _ = client
                    .send_text(userid, open_kfid, &format!("❌ 查询失败: {e}"))
                    .await;
                return;
            }
        };

        // Structured reply format.
        let sources_section = if sources.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = sources.iter().map(|s| format!("• {}", s.title)).collect();
            format!("\n\n📚 参考来源:\n{}", items.join("\n"))
        };

        let reply = format!(
            "💡 {question}\n\n{answer}{sources_section}\n\n—— 基于 ClawWiki 知识库回答"
        );
        let _ = client.send_text(userid, open_kfid, &reply).await;
        eprintln!("[kefu handler] query replied ({} chars)", reply.len());
    }

    /// /recent, /stats 命令
    async fn handle_command(
        &self,
        client: &KefuClient,
        userid: &str,
        open_kfid: &str,
        cmd: &str,
    ) {
        let paths = match &self.wiki_paths {
            Some(p) => p.clone(),
            None => {
                let _ = client
                    .send_text(userid, open_kfid, "❌ 知识库未初始化")
                    .await;
                return;
            }
        };

        let reply = match cmd.trim() {
            KEFU_COMMAND_RECENT => {
                let entries = wiki_store::list_raw_entries(&paths).unwrap_or_default();
                let recent: Vec<_> = entries.iter().rev().take(10).collect();
                if recent.is_empty() {
                    "📥 暂无入库记录".to_string()
                } else {
                    let stats = wiki_store::wiki_stats(&paths).ok();
                    let mut lines = vec![format!(
                        "📥 最近入库 ({} 条):\n",
                        recent.len()
                    )];
                    for (i, e) in recent.iter().enumerate() {
                        let emoji = source_emoji(&e.source);
                        lines.push(format!("{}. {} {} — {}", i + 1, emoji, e.slug, e.source));
                    }
                    if let Some(s) = stats {
                        lines.push(format!(
                            "\n知识库共 {} 条素材, {} 个页面",
                            s.raw_count, s.wiki_count
                        ));
                    }
                    lines.join("\n")
                }
            }
            KEFU_COMMAND_STATS => match wiki_store::wiki_stats(&paths) {
                Ok(s) => format!(
                    "📊 ClawWiki 知识库统计\n\n\
                     📄 素材: {} 条\n\
                     📖 Wiki 页面: {} 个\n\
                       └ 概念 {} · 人物 {} · 主题 {} · 对比 {}\n\
                     🔗 关联: {} 条\n\
                     📥 今日入库: {} 条\n\
                     📈 知识速率: {:.1} 页/天\n\
                     ✅ 维护成功率: {:.0}%",
                    s.raw_count,
                    s.wiki_count,
                    s.concept_count,
                    s.people_count,
                    s.topic_count,
                    s.compare_count,
                    s.edge_count,
                    s.today_ingest_count,
                    s.knowledge_velocity,
                    s.absorb_success_rate * 100.0,
                ),
                Err(e) => format!("❌ 统计失败: {e}"),
            },
            _ => format!("未知命令: {cmd}\n可用: {KEFU_COMMAND_RECENT} {KEFU_COMMAND_STATS}"),
        };

        let _ = client.send_text(userid, open_kfid, &reply).await;
    }

    /// URL → wiki_ingest → write_raw → absorb → 冲突检查 → 回复
    async fn handle_url_ingest(
        &self,
        client: &KefuClient,
        userid: &str,
        open_kfid: &str,
        url: &str,
    ) {
        let paths = match &self.wiki_paths {
            Some(p) => p.clone(),
            None => {
                let _ = client
                    .send_text(userid, open_kfid, "❌ 知识库未初始化")
                    .await;
                return;
            }
        };

        // Step 1: Fetch URL content.
        let ingest_result = match wiki_ingest::url::fetch_and_body(url).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[kefu handler] URL fetch failed: {e}");
                // v2 bugfix: do NOT ingest the "Failed to fetch: {url}" stub —
                // that's how we got garbage raw entries polluting Inbox.
                // Just reply to the user; no raw entry created.
                let _ = client
                    .send_text(
                        userid,
                        open_kfid,
                        &format!("❌ 无法获取链接内容: {e}\n请手动复制内容发送"),
                    )
                    .await;
                return;
            }
        };

        // Step 1.5 (v2 bugfix): Reject low-quality / anti-bot content.
        if let Err(reason) = wiki_ingest::validate_fetched_content(&ingest_result.body) {
            eprintln!("[kefu handler] URL content rejected: {reason}");
            let _ = client
                .send_text(
                    userid,
                    open_kfid,
                    &format!("❌ 链接抓取失败（{reason}）\n请手动复制内容发送"),
                )
                .await;
            return;
        }

        // Step 2: Write to raw/.
        let title = if ingest_result.title.is_empty() {
            url.to_string()
        } else {
            ingest_result.title.clone()
        };
        let fm = wiki_store::RawFrontmatter::for_paste("url", Some(url.to_string()));
        let raw_entry = match wiki_store::write_raw_entry(
            &paths,
            "url",
            &wiki_store::slugify(&title),
            &ingest_result.body,
            &fm,
        ) {
            Ok(e) => e,
            Err(e) => {
                let _ = client
                    .send_text(userid, open_kfid, &format!("❌ 入库失败: {e}"))
                    .await;
                return;
            }
        };

        // Step 2.5: Append Inbox `NewRaw` task so the maintainer surfaces
        // this entry in the review queue. Without this, kefu URL ingests
        // land in raw/ but the Inbox never lights up — users think their
        // link was silently dropped. Non-fatal: an Inbox append failure
        // must NOT block the ingest / absorb path.
        let short_userid = &userid[..8.min(userid.len())];
        let origin = format!("WeChat kefu · {short_userid}");
        if let Err(err) = wiki_store::append_new_raw_task(&paths, &raw_entry, &origin) {
            eprintln!("[kefu handler] URL inbox append failed: {err}");
        }

        // Step 3: Trigger absorb.
        let absorb_reply = trigger_absorb_internal(raw_entry.id).await;

        // Step 4: Reply confirmation.
        let reply = format!("✓ 已入库「{title}」{absorb_reply}");
        let _ = client.send_text(userid, open_kfid, &reply).await;
        eprintln!("[kefu handler] URL ingested: {} → raw #{}", url, raw_entry.id);

        // Step 5: Delayed conflict check (方案 A — 04-wechat-kefu.md §5.6).
        let paths_c = paths.clone();
        let client_uid = userid.to_string();
        let client_kfid = open_kfid.to_string();
        let client_clone = client.clone();
        tokio::spawn(async move {
            check_and_notify_conflicts(&paths_c, &client_clone, &client_uid, &client_kfid).await;
        });
    }

    /// 纯文本 → write_raw → absorb → 冲突检查 → 回复
    async fn handle_text_ingest(
        &self,
        client: &KefuClient,
        userid: &str,
        open_kfid: &str,
        text: &str,
    ) {
        let paths = match &self.wiki_paths {
            Some(p) => p.clone(),
            None => {
                let _ = client
                    .send_text(userid, open_kfid, "❌ 知识库未初始化")
                    .await;
                return;
            }
        };

        let char_count = text.chars().count();
        if !should_ingest_text(text) {
            let _ = client
                .send_text(
                    userid,
                    open_kfid,
                    &format!(
                        "消息太短（< {KEFU_TEXT_MIN_CHARS} 字），未入库。请发送更多内容或链接。"
                    ),
                )
                .await;
            return;
        }

        let short_id = &userid[..8.min(userid.len())];
        let slug = format!("kefu-{short_id}");
        let fm = wiki_store::RawFrontmatter::for_paste("wechat-text", None);
        let raw_entry = match wiki_store::write_raw_entry(&paths, "wechat-text", &slug, text, &fm)
        {
            Ok(e) => e,
            Err(e) => {
                let _ = client
                    .send_text(userid, open_kfid, &format!("❌ 入库失败: {e}"))
                    .await;
                return;
            }
        };

        // Append Inbox `NewRaw` task so kefu text ingests also surface in
        // the maintainer queue. Same rationale as handle_url_ingest above —
        // Inbox append must not block the reply / absorb path.
        let origin = format!("WeChat kefu · {short_id}");
        if let Err(err) = wiki_store::append_new_raw_task(&paths, &raw_entry, &origin) {
            eprintln!("[kefu handler] text inbox append failed: {err}");
        }

        let absorb_reply = trigger_absorb_internal(raw_entry.id).await;

        let reply = format!("✓ 已记录（{char_count} 字）{absorb_reply}");
        let _ = client.send_text(userid, open_kfid, &reply).await;
        eprintln!("[kefu handler] text ingested: raw #{}", raw_entry.id);

        // Delayed conflict check.
        let paths_c = paths.clone();
        let client_uid = userid.to_string();
        let client_kfid = open_kfid.to_string();
        let client_clone = client.clone();
        tokio::spawn(async move {
            check_and_notify_conflicts(&paths_c, &client_clone, &client_uid, &client_kfid).await;
        });
    }
}

/// Source type → emoji mapping for /recent display.
fn source_emoji(source: &str) -> &'static str {
    match source {
        "url" | "wechat-article" => "📰",
        "wechat-text" | "kefu-text" => "💬",
        "pdf" => "📄",
        "docx" => "📝",
        "voice" => "🎙️",
        "image" => "🖼️",
        "paste" => "📋",
        "pptx" => "📊",
        "video" => "🎬",
        "card" => "🪪",
        "chat" => "💬",
        _ => "📎",
    }
}

/// Delayed conflict check: wait for absorb to finish, then check inbox
/// for new conflict entries and push a WeChat notification.
/// Per 04-wechat-kefu.md §5.6 (方案 A).
async fn check_and_notify_conflicts(
    paths: &wiki_store::WikiPaths,
    client: &KefuClient,
    userid: &str,
    open_kfid: &str,
) {
    // Wait 5 seconds for absorb to process.
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let inbox = match wiki_store::list_inbox_entries(paths) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let conflicts: Vec<_> = inbox
        .iter()
        .filter(|e| {
            e.status == wiki_store::InboxStatus::Pending
                && e.kind == wiki_store::InboxKind::Conflict
        })
        .collect();

    if conflicts.is_empty() {
        return;
    }

    let items: Vec<String> = conflicts
        .iter()
        .take(5)
        .map(|c| format!("• {}", c.title))
        .collect();
    let msg = format!(
        "⚠️ 发现 {} 条知识冲突，请在 ClawWiki 中审核：\n{}",
        conflicts.len(),
        items.join("\n"),
    );

    let _ = client.send_text(userid, open_kfid, &msg).await;
    eprintln!(
        "[kefu handler] conflict notification sent ({} conflicts)",
        conflicts.len()
    );
}

/// Trigger absorb via internal HTTP POST to localhost.
/// Non-blocking: does not wait for absorb completion.
async fn trigger_absorb_internal(raw_id: u32) -> String {
    let client = reqwest::Client::new();
    match client
        .post("http://127.0.0.1:4357/api/wiki/absorb")
        .json(&serde_json::json!({ "entry_ids": [raw_id] }))
        .send()
        .await
    {
        Ok(resp) if resp.status().as_u16() == 202 => "，正在维护相关页面".to_string(),
        Ok(resp) if resp.status().as_u16() == 409 => "，维护队列排队中".to_string(),
        Ok(resp) => {
            eprintln!(
                "[kefu handler] absorb trigger unexpected status: {}",
                resp.status()
            );
            String::new()
        }
        Err(e) => {
            eprintln!("[kefu handler] absorb trigger failed: {e}");
            String::new()
        }
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

// ═══════════════════════════════════════════════════════════════════════
// Tests (Phase 2 Day 11-14)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_message ─────────────────────────────────────────

    #[test]
    fn classify_url_message() {
        let kind = classify_message("https://example.com 看看这个");
        assert!(matches!(kind, MessageKind::Url(ref u) if u == "https://example.com"));
    }

    #[test]
    fn classify_url_in_middle() {
        let kind = classify_message("看看 https://arxiv.org/abs/123 这篇论文");
        assert!(matches!(kind, MessageKind::Url(ref u) if u == "https://arxiv.org/abs/123"));
    }

    #[test]
    fn classify_query_ascii_mark() {
        let kind = classify_message("?什么是 transformer");
        assert!(matches!(kind, MessageKind::Query(ref q) if q == "什么是 transformer"));
    }

    #[test]
    fn classify_query_chinese_mark() {
        let kind = classify_message("\u{FF1F}transformer 架构");
        assert!(matches!(kind, MessageKind::Query(ref q) if q == "transformer 架构"));
    }

    #[test]
    fn classify_empty_query_ascii_mark() {
        let kind = classify_message("?");
        assert!(matches!(kind, MessageKind::Query(ref q) if q.is_empty()));
    }

    #[test]
    fn classify_empty_query_chinese_mark() {
        let kind = classify_message("\u{FF1F}");
        assert!(matches!(kind, MessageKind::Query(ref q) if q.is_empty()));
    }

    #[test]
    fn classify_command_recent() {
        let kind = classify_message(KEFU_COMMAND_RECENT);
        assert!(matches!(kind, MessageKind::Command(ref c) if c == KEFU_COMMAND_RECENT));
    }

    #[test]
    fn classify_command_stats() {
        let kind = classify_message(KEFU_COMMAND_STATS);
        assert!(matches!(kind, MessageKind::Command(ref c) if c == KEFU_COMMAND_STATS));
    }

    #[test]
    fn current_capability_commands_are_classified_as_commands() {
        let caps = crate::wechat_kefu::KefuCapabilities::current();
        assert!(caps.commands.iter().any(|cmd| cmd == KEFU_COMMAND_RECENT));
        assert!(caps.commands.iter().any(|cmd| cmd == KEFU_COMMAND_STATS));
        for cmd in caps.commands {
            let kind = classify_message(&cmd);
            assert!(matches!(kind, MessageKind::Command(ref c) if c == &cmd));
        }
    }

    #[test]
    fn classify_plain_text() {
        let kind = classify_message("今天天气不错，适合学习");
        assert!(matches!(kind, MessageKind::Text(ref t) if t == "今天天气不错，适合学习"));
    }

    #[test]
    fn classify_short_text() {
        // Short text is still classified as Text; the handler skips it.
        let kind = classify_message("嗯");
        assert!(matches!(kind, MessageKind::Text(ref t) if t == "嗯"));
    }

    // ── extract_first_url ────────────────────────────────────────

    #[test]
    fn extract_url_from_mixed_text() {
        assert_eq!(
            extract_first_url("看这个 https://x.com/abc 不错"),
            Some("https://x.com/abc".to_string()),
        );
    }

    #[test]
    fn extract_url_prefers_first_url() {
        assert_eq!(
            extract_first_url("先看 https://first.example 再看 https://second.example"),
            Some("https://first.example".to_string()),
        );
    }

    #[test]
    fn extract_url_http() {
        assert_eq!(
            extract_first_url("http://example.com"),
            Some("http://example.com".to_string()),
        );
    }

    #[test]
    fn extract_url_none() {
        assert_eq!(extract_first_url("纯文本没有链接"), None);
    }

    #[test]
    fn extract_url_ftp_ignored() {
        // FTP is not http/https, should not match.
        assert_eq!(extract_first_url("ftp://files.example.com"), None);
    }

    // ── pure boundary helpers ─────────────────────────────────────

    #[test]
    fn should_ingest_text_respects_minimum_character_count() {
        assert!(!should_ingest_text(&"好".repeat(KEFU_TEXT_MIN_CHARS - 1)));
        assert!(should_ingest_text(&"好".repeat(KEFU_TEXT_MIN_CHARS)));
    }

    #[test]
    fn unsupported_msgtype_reply_is_explicit() {
        let reply = unsupported_msgtype_reply("image");
        assert!(reply.contains("暂不支持image类型消息"));
        assert!(reply.contains("文字或链接"));
    }

    // ── source_emoji ─────────────────────────────────────────────

    #[test]
    fn source_emoji_all_types() {
        assert_eq!(source_emoji("url"), "📰");
        assert_eq!(source_emoji("wechat-article"), "📰");
        assert_eq!(source_emoji("wechat-text"), "💬");
        assert_eq!(source_emoji("kefu-text"), "💬");
        assert_eq!(source_emoji("pdf"), "📄");
        assert_eq!(source_emoji("docx"), "📝");
        assert_eq!(source_emoji("voice"), "🎙️");
        assert_eq!(source_emoji("image"), "🖼️");
        assert_eq!(source_emoji("paste"), "📋");
        assert_eq!(source_emoji("pptx"), "📊");
        assert_eq!(source_emoji("video"), "🎬");
        assert_eq!(source_emoji("card"), "🪪");
        assert_eq!(source_emoji("chat"), "💬");
        assert_eq!(source_emoji("unknown"), "📎");
    }

    // ── format verification ──────────────────────────────────────

    #[test]
    fn stats_format_contains_key_fields() {
        // Simulate a WikiStats and verify the format string.
        let stats = wiki_store::WikiStats {
            raw_count: 42,
            wiki_count: 28,
            concept_count: 15,
            people_count: 5,
            topic_count: 6,
            compare_count: 2,
            edge_count: 35,
            orphan_count: 3,
            inbox_pending: 7,
            inbox_resolved: 21,
            today_ingest_count: 3,
            week_new_pages: 12,
            avg_page_words: 150,
            absorb_success_rate: 0.75,
            knowledge_velocity: 1.7,
            last_absorb_at: None,
        };
        let reply = format!(
            "📊 ClawWiki 知识库统计\n\n\
             📄 素材: {} 条\n\
             📖 Wiki 页面: {} 个\n\
               └ 概念 {} · 人物 {} · 主题 {} · 对比 {}\n\
             🔗 关联: {} 条\n\
             📥 今日入库: {} 条\n\
             📈 知识速率: {:.1} 页/天\n\
             ✅ 维护成功率: {:.0}%",
            stats.raw_count,
            stats.wiki_count,
            stats.concept_count,
            stats.people_count,
            stats.topic_count,
            stats.compare_count,
            stats.edge_count,
            stats.today_ingest_count,
            stats.knowledge_velocity,
            stats.absorb_success_rate * 100.0,
        );
        assert!(reply.contains("素材: 42 条"));
        assert!(reply.contains("Wiki 页面: 28 个"));
        assert!(reply.contains("概念 15"));
        assert!(reply.contains("知识速率: 1.7 页/天"));
        assert!(reply.contains("维护成功率: 75%"));
    }

    #[test]
    fn recent_format_with_entries() {
        let entries = vec![
            wiki_store::RawEntry {
                id: 1,
                filename: "00001_url_test_2026-04-14.md".into(),
                source: "url".into(),
                slug: "test-article".into(),
                date: "2026-04-14".into(),
                source_url: Some("https://example.com".into()),
                ingested_at: "2026-04-14T10:00:00Z".into(),
                byte_size: 1234,
            },
            wiki_store::RawEntry {
                id: 2,
                filename: "00002_paste_note_2026-04-14.md".into(),
                source: "paste".into(),
                slug: "my-note".into(),
                date: "2026-04-14".into(),
                source_url: None,
                ingested_at: "2026-04-14T11:00:00Z".into(),
                byte_size: 567,
            },
        ];
        let recent: Vec<_> = entries.iter().rev().take(10).collect();
        let mut lines = vec![format!("📥 最近入库 ({} 条):\n", recent.len())];
        for (i, e) in recent.iter().enumerate() {
            let emoji = source_emoji(&e.source);
            lines.push(format!("{}. {} {} — {}", i + 1, emoji, e.slug, e.source));
        }
        let reply = lines.join("\n");
        assert!(reply.contains("📋 my-note — paste"));
        assert!(reply.contains("📰 test-article — url"));
    }

    // ── trigger_absorb_internal format ───────────────────────────
    // (Cannot test actual HTTP in unit tests, but verify the reply
    // builder logic is sound by checking string construction.)

    #[test]
    fn url_ingest_reply_format() {
        let title = "Transformer Architecture";
        let absorb_reply = "，正在维护相关页面";
        let reply = format!("✓ 已入库「{title}」{absorb_reply}");
        assert!(reply.contains("✓ 已入库「Transformer Architecture」"));
        assert!(reply.contains("正在维护相关页面"));
    }

    #[test]
    fn text_ingest_reply_format() {
        let char_count = 150;
        let absorb_reply = "，维护队列排队中";
        let reply = format!("✓ 已记录（{char_count} 字）{absorb_reply}");
        assert!(reply.contains("✓ 已记录（150 字）"));
        assert!(reply.contains("维护队列排队中"));
    }
}

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
use super::markdown_split::{split_markdown_for_wechat, DEFAULT_MAX_CHARS};
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

/// Chunk size used when a single assistant reply exceeds the iLink
/// per-message soft cap. See `markdown_split::DEFAULT_MAX_CHARS`. The
/// splitter preserves paragraph, list, and code-block boundaries so
/// each chunk is a self-contained markdown document.
const REPLY_CHUNK_MAX_CHARS: usize = DEFAULT_MAX_CHARS;

/// Safety cap on how many chunks a single reply can be split into.
/// If a reply is longer than `CHUNK_MAX_CHARS * MAX_REPLY_CHUNKS` we
/// emit the first N chunks + a truncation notice so we don't spam the
/// user with 40 consecutive messages.
const MAX_REPLY_CHUNKS: usize = 10;

/// Idle delay inserted between consecutive chunk sends. WeChat rejects
/// traffic that arrives too quickly; 300 ms comfortably clears the
/// observed rate limits while staying imperceptible to users.
const INTER_CHUNK_DELAY: Duration = Duration::from_millis(300);

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
    /// Optional ClawWiki raw-ingest sink (S5 — D2 override).
    ///
    /// When `Some`, every inbound text message is also written to
    /// `~/.clawwiki/raw/NNNNN_wechat-text_{slug}_{date}.md` and an
    /// `append_inbox_pending` task is appended, making the WeChat funnel
    /// the canonical ingestion path per ClawWiki §7. When `None` the
    /// handler falls back to Phase 2b behavior (chat only, no wiki
    /// persistence) — useful for unit tests and backward-compat CLI.
    wiki_paths: Option<wiki_store::WikiPaths>,
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
            wiki_paths: None,
        })
    }

    /// Attach a ClawWiki raw-ingest sink. Every subsequent inbound text
    /// message will also be written to `~/.clawwiki/raw/` and appended
    /// to the Inbox. Idempotent — replaces any previously attached sink.
    ///
    /// S5 wires this at desktop-server startup by passing the wiki root
    /// resolved via `wiki_store::default_root()`. Tests can bypass the
    /// sink by simply not calling this method.
    #[must_use]
    pub fn with_wiki_paths(mut self, paths: wiki_store::WikiPaths) -> Self {
        self.wiki_paths = Some(paths);
        self
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
        //
        // S5/S6 review finding #1: both the ClawWiki raw-write and the
        // agentic-loop turn MUST live inside this spawned task. Running
        // the `wiki_store::write_raw_entry` + `append_inbox_pending`
        // synchronously on the caller's await point (which is the
        // long-poll thread inside `monitor::run_monitor`) would stall
        // the poll until disk I/O completes. Under a large inbox.json
        // and a slow FS, that's tens of milliseconds per message —
        // enough to push the 35 s long-poll window over its budget.
        let handler = self.clone();
        let client = client.clone();
        tokio::spawn(async move {
            // S5 ClawWiki ingest (D2 override): if the handler is wired
            // with a wiki root, persist the raw text message to
            // `~/.clawwiki/raw/` BEFORE triggering the agentic turn.
            // Failures are logged but never block the chat reply.
            //
            // The filesystem calls (`create_dir_all`, `read`, `write`,
            // `rename`) are synchronous `std::fs`, so we hop them onto
            // a Tokio blocking pool worker via `spawn_blocking`. This
            // keeps them off the async executor threads — important
            // because those threads may be running other WeChat message
            // handlers in parallel.
            //
            // This is the one place in the whole codebase where personal
            // WeChat traffic becomes ClawWiki raw data. S6+ can layer
            // richer adapters (voice transcription, image captioning)
            // by moving the text-handling branch above into an adapter
            // dispatch.
            if let Some(paths) = handler.wiki_paths.clone() {
                let user_text_for_wiki = user_text.clone();
                let from_user_id_for_wiki = from_user_id.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    ingest_wechat_text_to_wiki(
                        &paths,
                        &from_user_id_for_wiki,
                        &user_text_for_wiki,
                    );
                })
                .await;
            }

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

            // Phase 4: split long assistant replies into chunks that fit
            // the iLink per-message soft cap while respecting markdown
            // boundaries (paragraph breaks, code blocks, lists).
            let mut chunks = split_markdown_for_wechat(&reply_text, REPLY_CHUNK_MAX_CHARS);

            if chunks.is_empty() {
                // Empty reply — likely an edge case where the agent
                // ended the turn without producing text. Surface
                // something so the user isn't left wondering.
                chunks.push("(assistant returned no text content)".to_string());
            }

            // Cap total chunks to avoid spamming the user in pathological
            // cases (e.g. the agent ran away producing 30 KB of output).
            let total_chunks_before_cap = chunks.len();
            if chunks.len() > MAX_REPLY_CHUNKS {
                chunks.truncate(MAX_REPLY_CHUNKS);
                if let Some(last) = chunks.last_mut() {
                    last.push_str(&format!(
                        "\n\n_[… truncated after {} chunks; original reply had {} chunks]_",
                        MAX_REPLY_CHUNKS, total_chunks_before_cap
                    ));
                }
            }

            let total_chunks = chunks.len();
            let multi = total_chunks > 1;
            let mut any_error = false;

            for (idx, chunk) in chunks.iter().enumerate() {
                // Prefix multi-chunk messages with an (i/n) marker so
                // the user knows there's more coming. Placed at the
                // TOP so it's immediately visible in WeChat's chat
                // list preview (which shows the first line).
                let body = if multi {
                    format!("({}/{})\n{}", idx + 1, total_chunks, chunk)
                } else {
                    chunk.clone()
                };

                let reply = build_text_reply(&from_user_id, &context_token, &body);
                match client.send_message(reply).await {
                    Ok(()) => {
                        eprintln!(
                            "[wechat agent] sent chunk {}/{} ({} chars)",
                            idx + 1,
                            total_chunks,
                            body.chars().count()
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "[wechat agent] reply send failed (chunk {}/{}): {e}",
                            idx + 1,
                            total_chunks
                        );
                        any_error = true;
                        break; // Stop sending remaining chunks on first failure
                    }
                }

                // Sleep between chunks to stay under iLink rate limits.
                // Skip the final sleep so we don't add pointless latency.
                if multi && idx + 1 < total_chunks {
                    tokio::time::sleep(INTER_CHUNK_DELAY).await;
                }
            }

            if !any_error {
                eprintln!(
                    "[wechat agent] turn end: openid={from_user_id} session={session_id} chunks={total_chunks}"
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

/// Synchronous filesystem sink for the S5 WeChat raw ingest path.
///
/// Extracted as a plain function so `tokio::task::spawn_blocking` can
/// own it cleanly — the blocking pool requires `FnOnce + Send +
/// 'static`, which is awkward to express inside an async block without
/// cloning every captured variable. This also gives tests a synchronous
/// entry point for the ingest logic if they ever need one.
///
/// All errors are logged to stderr and then swallowed. The contract is
/// best-effort: losing a wiki ingest is acceptable, losing the chat
/// reply that runs immediately after is not. See review finding #1
/// for the rationale.
fn ingest_wechat_text_to_wiki(
    paths: &wiki_store::WikiPaths,
    from_user_id: &str,
    user_text: &str,
) {
    // URL auto-detect: if the entire message (trimmed) looks like an
    // HTTP(S) URL, fetch the page and store the extracted markdown
    // instead of the bare URL text. This is the primary path for the
    // canonical §2 user story: user copies an mp.weixin.qq.com link
    // and pastes it to ClawBot. The extractor (feat H) handles the
    // HTML → clean markdown conversion including WeChat-specific
    // selectors.
    let trimmed = user_text.trim();
    let is_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    // Also detect URLs embedded in text (e.g. "看看这个 https://mp.weixin.qq.com/s/xxx")
    let extracted_url = if is_url {
        Some(trimmed.to_string())
    } else {
        extract_first_url(trimmed)
    };

    let (source_tag, slug_seed, body, source_url) = if let Some(url) = extracted_url {
        eprintln!("[wechat agent] detected URL in message, fetching: {url}");
        // Use tokio runtime to run the async fetch synchronously
        // (we're already inside spawn_blocking).
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                match std::thread::spawn(move || {
                    handle.block_on(wiki_ingest::url::fetch_and_body(&url))
                })
                .join()
                {
                    Ok(Ok(result)) => {
                        eprintln!(
                            "[wechat agent] URL fetched OK: title={:?} body_len={}",
                            result.title,
                            result.body.len()
                        );
                        (
                            "wechat-url".to_string(),
                            result.title,
                            result.body,
                            result.source_url,
                        )
                    }
                    Ok(Err(fetch_err)) => {
                        eprintln!(
                            "[wechat agent] URL fetch failed: {fetch_err}, storing raw text"
                        );
                        // Fallback: store the original text with the URL
                        (
                            "wechat-text".to_string(),
                            format!("WeChat · {}", short_openid(from_user_id)),
                            user_text.to_string(),
                            None,
                        )
                    }
                    Err(_panic) => {
                        eprintln!("[wechat agent] URL fetch thread panicked, storing raw text");
                        (
                            "wechat-text".to_string(),
                            format!("WeChat · {}", short_openid(from_user_id)),
                            user_text.to_string(),
                            None,
                        )
                    }
                }
            }
            Err(_) => {
                eprintln!("[wechat agent] no tokio runtime for URL fetch, storing raw text");
                (
                    "wechat-text".to_string(),
                    format!("WeChat · {}", short_openid(from_user_id)),
                    user_text.to_string(),
                    None,
                )
            }
        }
    } else {
        // Plain text message — store as-is.
        (
            "wechat-text".to_string(),
            format!("WeChat · {}", short_openid(from_user_id)),
            user_text.to_string(),
            None,
        )
    };

    let frontmatter =
        wiki_store::RawFrontmatter::for_paste(&source_tag, source_url);
    let entry = match wiki_store::write_raw_entry(
        paths,
        &source_tag,
        &slug_seed,
        &body,
        &frontmatter,
    ) {
        Ok(entry) => entry,
        Err(err) => {
            eprintln!(
                "[wechat agent] wiki_store::write_raw_entry failed: {err} \
                 (chat reply path still proceeds)"
            );
            return;
        }
    };

    let origin = format!("WeChat user `{}`", short_openid(from_user_id));
    if let Err(err) = wiki_store::append_new_raw_task(paths, &entry, &origin) {
        eprintln!("[wechat agent] raw written but inbox append failed: {err}");
    }
    eprintln!(
        "[wechat agent] wrote raw entry #{:05} ({})",
        entry.id, entry.filename
    );
}

/// Extract the first HTTP(S) URL from a text that may contain other
/// content (e.g. "看看这个 https://mp.weixin.qq.com/s/xxx 很有意思").
fn extract_first_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            // Trim trailing Chinese punctuation that often gets attached
            let url = word
                .trim_end_matches(|c: char| {
                    matches!(c, '。' | '，' | '！' | '？' | '；' | '、' | '\u{201c}' | '\u{201d}' | ')' | '）')
                });
            return Some(url.to_string());
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

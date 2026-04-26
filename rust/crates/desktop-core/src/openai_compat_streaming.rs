//! OpenAI-compatible streaming turn executor (A5.1).
//!
//! Before A5.1, when the user picked an OpenAI-compat provider in
//! Settings (Moonshot / Kimi / DeepSeek / Qwen / …), the Ask turn
//! was routed through `execute_live_turn` → vendored `api` crate's
//! synchronous `ConversationRuntime::run_turn`, which blocks until
//! the full reply is available and never emits
//! `DesktopSessionEvent::TextDelta`. Users saw "等一会儿、整段落下"
//! instead of real streaming.
//!
//! This module is the real streaming path for OpenAI-compat. It:
//!
//!   1. POSTs `{base_url}/chat/completions` with `stream: true` and
//!      the standard OpenAI ChatCompletions body.
//!   2. Parses the returned `text/event-stream` line-by-line.
//!   3. For every `choices[0].delta.content` chunk, broadcasts a
//!      `DesktopSessionEvent::TextDelta` to the session's event bus.
//!   4. Accumulates the full assistant text, then returns it as a
//!      single `ConversationMessage` for the caller to persist.
//!
//! Cancellation: the caller passes a `CancellationToken`. Any chunk
//! await is raced against it so `Cancel login` / `Stop` aborts the
//! HTTP read immediately instead of waiting for the 300s timeout.
//!
//! Scope:
//!  - Text-only. Tool-use is intentionally out of scope for MVP —
//!    OpenAI's tool-call streaming has per-provider quirks and the
//!    Ask page works as a pure chat surface on OpenAI-compat
//!    providers today (see `lib.rs::is_openai_compat_override`).
//!  - No session state is touched here. The caller owns the session
//!    store update + `Message` / `Snapshot` broadcasts.

use std::sync::Arc;
use std::time::{Duration, Instant};

use runtime::{ContentBlock, ConversationMessage, MessageRole};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::DesktopSessionEvent;

/// Minimal OpenAI ChatCompletions message shape.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChatMessage {
    pub role: String,
    pub content: String,
}

/// Config for a single streaming turn.
pub struct StreamingTurnConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub messages: Vec<OpenAiChatMessage>,
    /// Optional system prompt; prepended as a `{role:"system"}` entry.
    pub system_prompt: Option<String>,
    /// P1-1: Throttled (5s) callback invoked during stream to bump session
    /// updated_at. Prevents frontend isStale false-positives on long
    /// single responses (>30s Opus thinking, upstream slow-downs).
    /// Not required for correctness; finalize still sets updated_at.
    pub on_stream_tick: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Run one streaming OpenAI-compat ChatCompletions turn.
///
/// Broadcasts `TextDelta` per chunk. Returns the final accumulated
/// assistant message (as a `ConversationMessage`) plus the upstream
/// `model` label (if the server echoed it back) on success.
pub async fn run_streaming_turn(
    http_client: &reqwest::Client,
    config: StreamingTurnConfig,
    event_sender: &broadcast::Sender<DesktopSessionEvent>,
    session_id: &str,
    cancel_token: &CancellationToken,
) -> Result<(ConversationMessage, Option<String>), String> {
    use futures_util::StreamExt;

    // ── Build request body ─────────────────────────────────────────
    let mut messages: Vec<serde_json::Value> = Vec::new();
    if let Some(system) = config.system_prompt.as_ref() {
        if !system.trim().is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system,
            }));
        }
    }
    for m in &config.messages {
        messages.push(serde_json::json!({
            "role": m.role,
            "content": m.content,
        }));
    }
    let request_body = serde_json::json!({
        "model": config.model,
        "messages": messages,
        "stream": true,
    });

    // Endpoint: {base_url}/chat/completions. The base_url from
    // providers.json already includes /v1 (e.g. api.moonshot.cn/v1).
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

    let send_future = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .json(&request_body)
        .timeout(Duration::from_secs(300))
        .send();

    let response = tokio::select! {
        biased;
        _ = cancel_token.cancelled() => {
            return Err("cancelled by user".to_string());
        }
        r = send_future => {
            r.map_err(|e| format!("HTTP request failed: {e}"))?
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_string());
        return Err(format!("upstream returned {status}: {body}"));
    }

    // ── Parse SSE stream ───────────────────────────────────────────
    let mut accumulated = String::new();
    let mut upstream_model: Option<String> = None;
    let mut delta_count: usize = 0;
    // Track whether we observed a terminal event ([DONE] sentinel).
    // Socket FIN without a prior terminal event means the upstream
    // dropped us mid-response (TCP drop, provider 499, proxy hiccup)
    // and must be surfaced as `Err` so callers don't persist a
    // half-built assistant message as a finished reply.
    let mut saw_terminal = false;

    // P1-1: throttle stream-tick invocations to once per 5s. Seed so
    // the first delta fires a tick immediately — this keeps updated_at
    // fresh from the very start of a long response.
    let mut last_tick_at: Instant = Instant::now() - Duration::from_secs(5);

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();

    'outer: loop {
        let chunk_result = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                return Err("cancelled by user".to_string());
            }
            next = stream.next() => match next {
                Some(r) => r,
                None => {
                    // Stream source exhausted. Clean termination only
                    // if we already observed the terminal sentinel.
                    if saw_terminal {
                        break 'outer;
                    } else {
                        return Err(
                            "stream truncated before terminal event".to_string(),
                        );
                    }
                }
            }
        };
        let chunk = chunk_result.map_err(|e| format!("SSE stream error: {e}"))?;
        buffer.extend_from_slice(&chunk);

        while let Some(line) = drain_next_line(&mut buffer) {
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                saw_terminal = true;
                break;
            }
            let event: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // OpenAI echoes the model back on every chunk; capture
            // once for the caller so the session's `model_label`
            // reflects the *actual* model the upstream served.
            if upstream_model.is_none() {
                if let Some(m) = event.get("model").and_then(|v| v.as_str()) {
                    upstream_model = Some(m.to_string());
                }
            }

            let Some(choices) = event.get("choices").and_then(|v| v.as_array()) else {
                continue;
            };
            let Some(first) = choices.first() else {
                continue;
            };
            let Some(delta) = first.get("delta") else {
                continue;
            };
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                if content.is_empty() {
                    continue;
                }
                accumulated.push_str(content);
                delta_count += 1;
                let _ = event_sender.send(DesktopSessionEvent::TextDelta {
                    session_id: session_id.to_string(),
                    content: content.to_string(),
                });
                // P1-1: throttled tick — bumps session.updated_at at
                // most every 5s while the stream produces text deltas.
                if let Some(cb) = &config.on_stream_tick {
                    if last_tick_at.elapsed() >= Duration::from_secs(5) {
                        cb();
                        last_tick_at = Instant::now();
                    }
                }
            }
        }

        // Once we've seen the terminal sentinel, exit immediately
        // rather than awaiting an optional socket FIN. Mirrors the
        // "semantic end-of-stream wins" design from parse_sse_stream
        // in agentic_loop.rs and keeps happy-path latency unchanged.
        if saw_terminal {
            break 'outer;
        }
    }

    eprintln!(
        "[openai-compat-stream] session={session_id} model={:?} deltas={} len={}",
        upstream_model,
        delta_count,
        accumulated.len()
    );

    let message = ConversationMessage {
        role: MessageRole::Assistant,
        blocks: vec![ContentBlock::Text { text: accumulated }],
        usage: None,
    };

    Ok((message, upstream_model))
}

fn drain_next_line(buffer: &mut Vec<u8>) -> Option<String> {
    let newline_pos = buffer.iter().position(|&b| b == b'\n')?;
    let line_bytes: Vec<u8> = buffer.drain(..=newline_pos).collect();
    let line_slice = &line_bytes[..line_bytes.len() - 1];
    let line_slice = if line_slice.last() == Some(&b'\r') {
        &line_slice[..line_slice.len() - 1]
    } else {
        line_slice
    };
    Some(String::from_utf8_lossy(line_slice).into_owned())
}

/// Flatten a `RuntimeSession`'s history into OpenAI-style
/// `{role, content}` messages. Tool blocks are dropped (MVP path)
/// and each message's text blocks are concatenated with newlines.
pub fn messages_from_runtime_session(session: &runtime::Session) -> Vec<OpenAiChatMessage> {
    session
        .messages
        .iter()
        .filter_map(|m| {
            let role = match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                // `MessageRole::Tool` (native tool-use turn) is dropped
                // because the Ask page strips tools on openai_compat
                // providers. If an upstream ever adds tool turns here
                // they'd serialize as opaque text anyway.
                MessageRole::Tool => return None,
            };
            let content = m
                .blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    // Skip tool blocks — OpenAI-compat providers don't
                    // share Anthropic's tool_use / tool_result schema
                    // and the Ask page strips tools on these providers
                    // anyway.
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some(OpenAiChatMessage {
                role: role.to_string(),
                content,
            })
        })
        .filter(|m| !m.content.trim().is_empty())
        .collect()
}

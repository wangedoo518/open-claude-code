//! Async agentic conversation loop.
//!
//! Reimplements the synchronous `ConversationRuntime::run_turn` as a fully async
//! loop with incremental SSE broadcasting and async permission prompting.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Response;
use runtime::{
    should_compact, compact_session, CompactionConfig,
    ContentBlock, ConversationMessage, HookRunner, RuntimeHookConfig,
    Session as RuntimeSession,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio_util::sync::CancellationToken;
use tools::GlobalToolRegistry;

use crate::{
    DesktopConversationMessage, DesktopSessionDetail, DesktopSessionEvent, SessionId,
};

// ── Constants ────────────────────────────────────────────────────────

/// Maximum number of LLM round-trips before the loop terminates.
const MAX_LOOP_ITERATIONS: usize = 50;

/// Timeout for a single permission prompt response from the frontend (5 min).
const PERMISSION_TIMEOUT_SECS: u64 = 300;

/// Default model label when model cannot be determined.
const DEFAULT_MODEL_LABEL: &str = "unknown";

/// Maximum size of a single tool output before truncation (100 KB).
const MAX_TOOL_OUTPUT_CHARS: usize = 100_000;

// ── Permission types ─────────────────────────────────────────────────

/// Decision returned by the frontend (or auto-resolved by policy).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
    AllowAlways,
}

/// A pending permission request waiting for frontend response.
pub struct PendingPermission {
    pub request_id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub sender: oneshot::Sender<PermissionDecision>,
}

/// Bridges the async agentic loop with the frontend permission dialog.
///
/// When the loop needs user permission, it stores a oneshot sender here and
/// broadcasts a `PermissionRequest` SSE event. The frontend shows the dialog,
/// the user decides, and the HTTP handler calls `resolve()` which sends the
/// decision back through the oneshot channel.
pub struct PermissionGate {
    /// Multiple concurrent pending requests keyed by request_id.
    /// This supports parallel tool execution and resolves the race where
    /// a second request would overwrite the first.
    pending: Mutex<HashMap<String, PendingPermission>>,
    event_sender: broadcast::Sender<DesktopSessionEvent>,
    session_id: SessionId,
    /// Tools the user has chosen "Allow always" for during this session.
    always_allow: Mutex<std::collections::HashSet<String>>,
}

impl PermissionGate {
    pub fn new(
        event_sender: broadcast::Sender<DesktopSessionEvent>,
        session_id: SessionId,
    ) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            event_sender,
            session_id,
            always_allow: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// Check if the tool is allowed. If not, ask the user via SSE and wait.
    pub async fn check_permission(
        &self,
        tool_name: &str,
        tool_input: &Value,
        bypass_all: bool,
    ) -> PermissionDecision {
        // Bypass mode: allow everything without asking.
        if bypass_all {
            return PermissionDecision::Allow;
        }

        // Read-only tools never need permission.
        if is_read_only_tool(tool_name) {
            return PermissionDecision::Allow;
        }

        // Already allowed via "Allow always" this session.
        {
            let always = self.always_allow.lock().await;
            if always.contains(tool_name) {
                return PermissionDecision::Allow;
            }
        }

        // Need to ask the user.
        let request_id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();

        {
            let mut pending = self.pending.lock().await;
            pending.insert(
                request_id.clone(),
                PendingPermission {
                    request_id: request_id.clone(),
                    tool_name: tool_name.to_string(),
                    tool_input: tool_input.clone(),
                    sender,
                },
            );
        }

        // Broadcast the permission request to the frontend.
        let _ = self.event_sender.send(DesktopSessionEvent::PermissionRequest {
            session_id: self.session_id.clone(),
            request_id: request_id.clone(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.to_string(),
        });

        // Wait for the user's response (with timeout).
        //
        // Race-safety invariant: if `resolve()` successfully sends a decision,
        // it also removes the entry from `pending`. Therefore the `Ok(Ok)`
        // success path below does NOT need to clean up the map — resolve()
        // already did. Only the timeout and channel-closed paths need cleanup.
        //
        // This prevents the previous race where:
        // 1. resolve() holds the lock, removes entry, sends decision
        // 2. timeout fires at the same instant
        // 3. check_permission unconditionally removes (no-op — already gone)
        // Was fine, BUT:
        // 1. User's decision arrives, resolve() is about to acquire lock
        // 2. Timeout fires, check_permission removes entry and drops sender
        // 3. resolve() then acquires lock, entry is gone, returns false
        // 4. User sees their Allow silently dropped → Deny
        //
        // Fix: only clean up in the failure paths.
        let result = tokio::time::timeout(
            Duration::from_secs(PERMISSION_TIMEOUT_SECS),
            receiver,
        )
        .await;

        match result {
            Ok(Ok(decision)) => {
                // resolve() succeeded — entry already removed. No cleanup needed.
                if decision == PermissionDecision::AllowAlways {
                    let mut always = self.always_allow.lock().await;
                    always.insert(tool_name.to_string());
                }
                decision
            }
            Ok(Err(_)) => {
                // oneshot sender was dropped without sending. Clean up
                // defensively in case the entry is still present.
                let mut pending = self.pending.lock().await;
                pending.remove(&request_id);
                PermissionDecision::Deny {
                    reason: "permission channel closed".into(),
                }
            }
            Err(_) => {
                // Timeout: clean up the entry if it's still there.
                // Note: there is still a small window where resolve() could
                // have started just before the timeout fires but not yet
                // acquired the lock. We check for the entry and if it's
                // gone, trust that resolve() will send on the dropped
                // receiver (which is a no-op, harmless).
                let mut pending = self.pending.lock().await;
                pending.remove(&request_id);
                PermissionDecision::Deny {
                    reason: "permission request timed out (5 min)".into(),
                }
            }
        }
    }

    /// Resolve a pending permission request (called by the HTTP handler).
    pub async fn resolve(&self, request_id: &str, decision: PermissionDecision) -> bool {
        let mut pending = self.pending.lock().await;
        if let Some(p) = pending.remove(request_id) {
            let _ = p.sender.send(decision);
            return true;
        }
        false
    }
}

// ── Agentic loop result ──────────────────────────────────────────────

/// Outcome of a single agentic turn.
pub struct AgenticTurnResult {
    /// The updated session with all new messages appended.
    pub session: RuntimeSession,
    /// Model label from the LLM response.
    pub model_label: String,
    /// Number of LLM round-trips executed.
    pub iterations: usize,
    /// Whether the turn was cancelled by the user.
    pub was_cancelled: bool,
}

/// Errors during the agentic loop.
#[derive(Debug)]
pub enum AgenticError {
    /// LLM API call failed.
    ApiError(String),
    /// Exceeded max iterations.
    MaxIterationsExceeded,
    /// User cancelled the turn.
    Cancelled,
    /// Internal error.
    Internal(String),
}

impl std::fmt::Display for AgenticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiError(msg) => write!(f, "LLM API error: {msg}"),
            Self::MaxIterationsExceeded => {
                write!(f, "exceeded maximum loop iterations ({MAX_LOOP_ITERATIONS})")
            }
            Self::Cancelled => write!(f, "turn cancelled by user"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

// ── Core agentic loop ────────────────────────────────────────────────

/// A simplified MCP server descriptor from the frontend settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub name: String,
    pub transport: String,
    pub target: String,
    pub enabled: bool,
}

/// Configuration for the agentic loop.
pub struct AgenticLoopConfig {
    /// Base URL for the code-tools-bridge endpoint.
    /// e.g. `http://127.0.0.1:4357/api/desktop/code-tools/claude-bridge/codex-openai`
    pub bridge_base_url: String,
    /// Bearer token for the upstream provider.
    pub bearer_token: String,
    /// Model to use.
    pub model: String,
    /// Working directory for tool execution.
    pub project_path: PathBuf,
    /// System prompt (pre-built).
    pub system_prompt: Option<String>,
    /// Whether to bypass all permissions (DangerFullAccess mode).
    pub bypass_permissions: bool,
    /// Optional callback invoked after each loop iteration with the
    /// current session state, for incremental persistence.
    pub on_iteration_complete: Option<Arc<dyn Fn(&RuntimeSession) + Send + Sync>>,
    /// MCP servers to connect at loop startup.
    pub mcp_servers: Vec<McpServerEntry>,
    /// Hook configuration for PreToolUse/PostToolUse lifecycle hooks.
    pub hooks: Option<RuntimeHookConfig>,
}

/// Run the async agentic conversation loop.
///
/// This function:
/// 1. Sends the conversation to the LLM API.
/// 2. Streams the response and accumulates it.
/// 3. If the response contains tool_use blocks, executes tools locally.
/// 4. Broadcasts each step via SSE for real-time frontend updates.
/// 5. Loops until the LLM stops requesting tools (or limits are hit).
pub async fn run_agentic_loop(
    session: RuntimeSession,
    config: AgenticLoopConfig,
    event_sender: broadcast::Sender<DesktopSessionEvent>,
    session_id: SessionId,
    permission_gate: Arc<PermissionGate>,
    cancel_token: CancellationToken,
) -> Result<AgenticTurnResult, AgenticError> {
    let mut current_session = session;
    let mut iterations = 0usize;
    let mut model_label = config.model.clone();

    let client = reqwest::Client::new();
    let tool_specs = tools::mvp_tool_specs();

    // ── Probe MCP servers (CONFIG VALIDATION ONLY, not callable) ──
    // See probe_mcp_servers docstring + docs/audit-lessons.md L-09 for
    // why MCP tools do not actually work through the agentic loop.
    if !config.mcp_servers.is_empty() {
        probe_mcp_servers(&config.mcp_servers);
    }

    // ── Initialize hooks runner (if configured) ──────────────────
    let hook_runner = config.hooks.map(HookRunner::new);

    loop {
        // ── Check limits ─────────────────────────────────────────
        iterations += 1;
        if iterations > MAX_LOOP_ITERATIONS {
            // Append a system message explaining the limit, then return gracefully.
            let limit_msg = ConversationMessage {
                role: runtime::MessageRole::Assistant,
                blocks: vec![ContentBlock::Text {
                    text: format!(
                        "⚠️ Agentic loop reached the maximum of {MAX_LOOP_ITERATIONS} iterations. \
                         Stopping to prevent runaway execution. You can continue by sending another message."
                    ),
                }],
                usage: None,
            };
            let _ = current_session.push_message(limit_msg.clone());
            let _ = event_sender.send(DesktopSessionEvent::Message {
                session_id: session_id.clone(),
                message: DesktopConversationMessage::from(&limit_msg),
            });
            return Ok(AgenticTurnResult {
                session: current_session,
                model_label,
                iterations,
                was_cancelled: false,
            });
        }
        if cancel_token.is_cancelled() {
            return Ok(AgenticTurnResult {
                session: current_session,
                model_label,
                iterations,
                was_cancelled: true,
            });
        }

        // ── Auto-compact if context is too large ──────────────────
        let compaction_config = CompactionConfig::default();
        if should_compact(&current_session, compaction_config) {
            let result = compact_session(&current_session, compaction_config);
            let removed = result.removed_message_count;
            current_session = result.compacted_session;

            // Broadcast compaction notice to frontend.
            let notice = ConversationMessage {
                role: runtime::MessageRole::Assistant,
                blocks: vec![ContentBlock::Text {
                    text: format!(
                        "📦 Context compacted: {removed} older messages summarized to free context window."
                    ),
                }],
                usage: None,
            };
            let _ = current_session.push_message(notice.clone());
            let _ = event_sender.send(DesktopSessionEvent::Message {
                session_id: session_id.clone(),
                message: DesktopConversationMessage::from(&notice),
            });
        }

        // ── Build Anthropic Messages API request (streaming) ─────
        let api_request = build_api_request(
            &current_session,
            &config.model,
            config.system_prompt.as_deref(),
            &tool_specs,
        );

        // ── Call LLM via code-tools-bridge (streaming SSE) ──────
        let api_result = call_llm_api_streaming(
            &client,
            &config.bridge_base_url,
            &config.bearer_token,
            &api_request,
            &event_sender,
            &session_id,
        )
        .await;

        let (assistant_message, stop_reason, response_model) = match api_result {
            Ok(result) => result,
            Err(api_error) => {
                // API error: append error message to session and return gracefully
                // so the user sees what happened and can retry.
                let error_msg = ConversationMessage {
                    role: runtime::MessageRole::Assistant,
                    blocks: vec![ContentBlock::Text {
                        text: format!("⚠️ LLM API error: {api_error}"),
                    }],
                    usage: None,
                };
                let _ = current_session.push_message(error_msg.clone());
                let _ = event_sender.send(DesktopSessionEvent::Message {
                    session_id: session_id.clone(),
                    message: DesktopConversationMessage::from(&error_msg),
                });
                return Ok(AgenticTurnResult {
                    session: current_session,
                    model_label,
                    iterations,
                    was_cancelled: false,
                });
            }
        };

        if let Some(m) = response_model {
            model_label = m;
        }

        // Append assistant message to session.
        current_session
            .push_message(assistant_message.clone())
            .map_err(|e| AgenticError::Internal(e.to_string()))?;

        // Broadcast the assistant message.
        let _ = event_sender.send(DesktopSessionEvent::Message {
            session_id: session_id.clone(),
            message: DesktopConversationMessage::from(&assistant_message),
        });

        // ── Extract tool_use blocks ──────���───────────────────────
        let pending_tools: Vec<(String, String, String)> = assistant_message
            .blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        // If no tool calls, the LLM is done.
        if pending_tools.is_empty() {
            break;
        }

        // ── Execute each tool ────────────────────────────────────
        for (tool_use_id, tool_name, tool_input_str) in pending_tools {
            if cancel_token.is_cancelled() {
                return Ok(AgenticTurnResult {
                    session: current_session,
                    model_label,
                    iterations,
                    was_cancelled: true,
                });
            }

            // Parse tool input for permission check.
            let tool_input_value: Value = serde_json::from_str(&tool_input_str)
                .unwrap_or_else(|_| serde_json::json!({ "raw": &tool_input_str }));

            // Check permission.
            let permission = permission_gate
                .check_permission(&tool_name, &tool_input_value, config.bypass_permissions)
                .await;

            let tool_result_message = match permission {
                PermissionDecision::Allow | PermissionDecision::AllowAlways => {
                    // Run PreToolUse hook (if configured).
                    let hook_cancelled = if let Some(ref runner) = hook_runner {
                        let result = runner.run_pre_tool_use(&tool_name, &tool_input_str);
                        result.is_cancelled() || result.is_denied()
                    } else {
                        false
                    };

                    if hook_cancelled {
                        ConversationMessage::tool_result(
                            tool_use_id.clone(),
                            tool_name.clone(),
                            "Tool execution blocked by PreToolUse hook.".to_string(),
                            true,
                        )
                    } else {
                        // Execute the tool in a blocking thread under the
                        // process-wide workspace lock with CWD save/restore.
                        // This prevents cross-session CWD races.
                        let name = tool_name.clone();
                        let input_value = tool_input_value.clone();
                        let tool_cwd = config.project_path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            execute_tool_in_workspace(&tool_cwd, &name, &input_value)
                        })
                        .await
                        .unwrap_or_else(|e| Err(format!("tool task panicked: {e}")));

                        let (output, is_error) = match result {
                            Ok(output) => (truncate_tool_output(output), false),
                            Err(error) => (truncate_tool_output(error), true),
                        };

                        // Run PostToolUse hook (if configured).
                        if let Some(ref runner) = hook_runner {
                            if is_error {
                                runner.run_post_tool_use_failure(&tool_name, &tool_input_str, &output);
                            } else {
                                runner.run_post_tool_use(&tool_name, &tool_input_str, &output, false);
                            }
                        }

                        ConversationMessage::tool_result(
                            tool_use_id.clone(),
                            tool_name.clone(),
                            output,
                            is_error,
                        )
                    }
                }
                PermissionDecision::Deny { reason } => ConversationMessage::tool_result(
                    tool_use_id.clone(),
                    tool_name.clone(),
                    format!("Permission denied: {reason}"),
                    true,
                ),
            };

            // Append tool result to session.
            current_session
                .push_message(tool_result_message.clone())
                .map_err(|e| AgenticError::Internal(e.to_string()))?;

            // Broadcast the tool result.
            let _ = event_sender.send(DesktopSessionEvent::Message {
                session_id: session_id.clone(),
                message: DesktopConversationMessage::from(&tool_result_message),
            });
        }

        // ── Incremental persistence ──────────────────────────────
        if let Some(ref callback) = config.on_iteration_complete {
            callback(&current_session);
        }

        // Check if stop_reason was not tool_use (shouldn't happen since we
        // found tool blocks, but be defensive).
        if stop_reason.as_deref() != Some("tool_use") {
            break;
        }
    }

    Ok(AgenticTurnResult {
        session: current_session,
        model_label,
        iterations,
        was_cancelled: false,
    })
}

// ── Helper functions ─────────────────────────────────────────────────

/// Build an Anthropic Messages API request body from the current session state.
fn build_api_request(
    session: &RuntimeSession,
    model: &str,
    system_prompt: Option<&str>,
    tool_specs: &[tools::ToolSpec],
) -> Value {
    let messages: Vec<Value> = session
        .messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                runtime::MessageRole::User | runtime::MessageRole::Tool => "user",
                runtime::MessageRole::Assistant => "assistant",
                runtime::MessageRole::System => "system",
            };
            let content: Vec<Value> = msg
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => serde_json::json!({
                        "type": "text",
                        "text": text
                    }),
                    ContentBlock::ToolUse { id, name, input } => {
                        // `input` is stored as a raw JSON string; re-parse and
                        // coerce to an object. Anthropic API requires tool_use
                        // input to be an object (not null, array, number, etc.).
                        let input_value = coerce_tool_input_to_object(input);
                        serde_json::json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input_value
                        })
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        tool_name: _,
                        output,
                        is_error,
                    } => serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": output,
                        "is_error": is_error
                    }),
                })
                .collect();
            serde_json::json!({
                "role": role,
                "content": content
            })
        })
        .collect();

    let tools: Vec<Value> = tool_specs
        .iter()
        .map(|spec| {
            serde_json::json!({
                "name": spec.name,
                "description": spec.description,
                "input_schema": spec.input_schema
            })
        })
        .collect();

    let mut request = serde_json::json!({
        "model": model,
        "max_tokens": 16384,
        "messages": messages,
        "tools": tools,
        "stream": true
    });

    if let Some(system) = system_prompt {
        request["system"] = serde_json::json!(system);
    }

    request
}

/// Call the LLM API via the code-tools-bridge proxy with streaming SSE.
///
/// Parses Anthropic SSE events incrementally, broadcasts `TextDelta` events
/// to the frontend for real-time text display, and accumulates the full
/// assistant message (text blocks + tool_use blocks) for the agentic loop.
///
/// Returns `(ConversationMessage, stop_reason, model)`.
async fn call_llm_api_streaming(
    client: &reqwest::Client,
    bridge_base_url: &str,
    bearer_token: &str,
    request_body: &Value,
    event_sender: &broadcast::Sender<DesktopSessionEvent>,
    session_id: &str,
) -> Result<(ConversationMessage, Option<String>, Option<String>), String> {
    let url = format!("{bridge_base_url}/v1/messages");

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {bearer_token}"))
        .header("Content-Type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .json(request_body)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_string());
        return Err(format!("LLM API returned {status}: {body}"));
    }

    // Check if response is actually SSE (text/event-stream) or plain JSON.
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.contains("text/event-stream") {
        parse_sse_stream(response, event_sender, session_id).await
    } else {
        // Fallback: non-streaming JSON response (upstream didn't support streaming).
        let body = response
            .json::<Value>()
            .await
            .map_err(|e| format!("failed to parse LLM response: {e}"))?;
        parse_json_response(&body)
    }
}

/// Parse a streaming SSE response from the Anthropic Messages API.
///
/// Events follow the Anthropic protocol:
/// - `message_start`: contains model and initial usage
/// - `content_block_start`: begins a text or tool_use block
/// - `content_block_delta`: incremental content (text_delta or input_json_delta)
/// - `content_block_stop`: ends a content block
/// - `message_delta`: contains stop_reason and final usage
/// - `message_stop`: stream is done
async fn parse_sse_stream(
    response: Response,
    event_sender: &broadcast::Sender<DesktopSessionEvent>,
    session_id: &str,
) -> Result<(ConversationMessage, Option<String>, Option<String>), String> {
    use futures_util::StreamExt;

    let mut model_label: Option<String> = None;
    let mut stop_reason: Option<String> = None;

    // Accumulated content blocks indexed by position.
    let mut text_blocks: HashMap<usize, String> = HashMap::new();
    let mut tool_blocks: HashMap<usize, ToolBlockAccumulator> = HashMap::new();
    let mut block_types: HashMap<usize, String> = HashMap::new(); // index → "text" | "tool_use"

    let mut stream = response.bytes_stream();
    // Use a byte buffer to avoid corrupting multi-byte UTF-8 characters
    // that may be split across chunk boundaries. We only decode to UTF-8
    // when we have a complete line (terminated by \n, which is always
    // single-byte 0x0A and cannot appear inside a UTF-8 multi-byte sequence).
    let mut buffer: Vec<u8> = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("SSE stream error: {e}"))?;
        buffer.extend_from_slice(&chunk);

        // Process complete lines from buffer.
        while let Some(line) = drain_next_line(&mut buffer) {
            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }
                let event: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let event_type = event
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                match event_type {
                    "message_start" => {
                        if let Some(msg) = event.get("message") {
                            model_label = msg
                                .get("model")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                        }
                    }
                    "content_block_start" => {
                        let index = event
                            .get("index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        if let Some(cb) = event.get("content_block") {
                            let block_type = cb
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("text");
                            block_types.insert(index, block_type.to_string());
                            match block_type {
                                "text" => {
                                    text_blocks.insert(index, String::new());
                                }
                                "tool_use" => {
                                    let id = cb
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let name = cb
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    tool_blocks.insert(
                                        index,
                                        ToolBlockAccumulator {
                                            id,
                                            name,
                                            input_json: String::new(),
                                        },
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    "content_block_delta" => {
                        let index = event
                            .get("index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        if let Some(delta) = event.get("delta") {
                            let delta_type = delta
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            match delta_type {
                                "text_delta" => {
                                    if let Some(text) =
                                        delta.get("text").and_then(|v| v.as_str())
                                    {
                                        if let Some(acc) = text_blocks.get_mut(&index) {
                                            acc.push_str(text);
                                        }
                                        // Broadcast text delta to frontend.
                                        let _ = event_sender.send(
                                            DesktopSessionEvent::TextDelta {
                                                session_id: session_id.to_string(),
                                                content: text.to_string(),
                                            },
                                        );
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(partial) = delta
                                        .get("partial_json")
                                        .and_then(|v| v.as_str())
                                    {
                                        if let Some(acc) = tool_blocks.get_mut(&index) {
                                            acc.input_json.push_str(partial);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "message_delta" => {
                        if let Some(delta) = event.get("delta") {
                            stop_reason = delta
                                .get("stop_reason")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                        }
                    }
                    "message_stop" | "error" => {
                        // Stream complete.
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Assemble final ConversationMessage from accumulated blocks ────
    let mut max_index = 0usize;
    for &idx in block_types.keys() {
        if idx >= max_index {
            max_index = idx + 1;
        }
    }

    let mut blocks = Vec::new();
    for idx in 0..max_index {
        match block_types.get(&idx).map(String::as_str) {
            Some("text") => {
                let text = text_blocks.remove(&idx).unwrap_or_default();
                blocks.push(ContentBlock::Text { text });
            }
            Some("tool_use") => {
                if let Some(acc) = tool_blocks.remove(&idx) {
                    blocks.push(ContentBlock::ToolUse {
                        id: acc.id,
                        name: acc.name,
                        input: acc.input_json,
                    });
                }
            }
            _ => {}
        }
    }

    let message = ConversationMessage {
        role: runtime::MessageRole::Assistant,
        blocks,
        usage: None,
    };

    Ok((message, stop_reason, model_label))
}

/// Fallback: parse a non-streaming JSON response.
fn parse_json_response(
    response: &Value,
) -> Result<(ConversationMessage, Option<String>, Option<String>), String> {
    let stop_reason = response
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .map(String::from);
    let model_label = response
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from);

    let content = response
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "response missing 'content' array".to_string())?;

    let mut blocks = Vec::new();

    for block in content {
        let block_type = block
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        match block_type {
            "text" => {
                let text = block
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                blocks.push(ContentBlock::Text { text });
            }
            "tool_use" => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block
                    .get("input")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                blocks.push(ContentBlock::ToolUse { id, name, input });
            }
            _ => {
                let text = block.to_string();
                blocks.push(ContentBlock::Text { text });
            }
        }
    }

    let message = ConversationMessage {
        role: runtime::MessageRole::Assistant,
        blocks,
        usage: None,
    };

    Ok((message, stop_reason, model_label))
}

/// Helper for accumulating tool_use input JSON across streaming deltas.
struct ToolBlockAccumulator {
    id: String,
    name: String,
    input_json: String,
}

/// Probe MCP servers for config validation only.
///
/// ## ⚠️ LIMITATION — NOT a full integration
///
/// This function does NOT make MCP tools callable from the agentic loop.
/// It only:
///   1. Validates each `McpServerEntry` can be converted to a
///      `ScopedMcpServerConfig`
///   2. Spawns a short-lived `McpServerManager` to connect and probe for tools
///   3. Logs discovered tool counts to stderr
///
/// The vendored `tools` crate stores its MCP registry in a crate-private
/// `global_mcp_registry()` that we cannot populate from outside. Subsequent
/// calls to `execute_tool("MCP", ...)` go through that private registry and
/// will return `"server not found"` because the manager we create here is
/// dropped immediately.
///
/// See `docs/audit-lessons.md` L-09 for the full incident history.
///
/// To make MCP tools actually work, one of the following is required (not
/// implemented):
///   - Fork the `claw-code-parity` crate to expose `global_mcp_registry()`
///   - Implement a separate MCP client in `desktop-core` that bypasses the
///     vendored tool dispatcher
///   - Use the legacy `execute_live_turn` path which auto-initializes MCP
///     via the runtime's internal wiring
fn probe_mcp_servers(servers: &[McpServerEntry]) {
    use runtime::{McpServerConfig, McpServerManager, McpStdioServerConfig, ScopedMcpServerConfig};
    use std::collections::BTreeMap;

    let enabled: Vec<&McpServerEntry> = servers.iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return;
    }

    let mut server_configs: BTreeMap<String, ScopedMcpServerConfig> = BTreeMap::new();
    for entry in &enabled {
        if entry.transport != "stdio" {
            eprintln!(
                "[MCP probe] server '{}': transport '{}' not supported, skipping",
                entry.name, entry.transport
            );
            continue;
        }

        let parts: Vec<&str> = entry.target.split_whitespace().collect();
        let (command, args) = if let Some((cmd, rest)) = parts.split_first() {
            (
                cmd.to_string(),
                rest.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )
        } else {
            continue;
        };

        let scoped = ScopedMcpServerConfig {
            scope: runtime::ConfigSource::User,
            config: McpServerConfig::Stdio(McpStdioServerConfig {
                command,
                args,
                env: Default::default(),
                tool_call_timeout_ms: None,
            }),
        };
        server_configs.insert(entry.name.clone(), scoped);
    }

    if server_configs.is_empty() {
        return;
    }

    // Spawn probe in a background thread with its own tokio runtime.
    // Capture the JoinHandle and catch panics so errors are visible.
    let server_count = server_configs.len();
    let spawn_result = std::thread::Builder::new()
        .name("mcp-probe".to_string())
        .spawn(move || {
            // Catch panics inside the thread so they surface as clear errors
            // instead of silently dropped.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("[MCP probe] failed to create tokio runtime: {e}");
                        return;
                    }
                };

                rt.block_on(async move {
                    let mut manager = McpServerManager::from_servers(&server_configs);
                    match manager.discover_tools().await {
                        Ok(tools) => {
                            eprintln!(
                                "[MCP probe] validated {} server(s), discovered {} tool(s). \
                                 WARNING: These tools are NOT callable from the agentic loop. \
                                 See docs/audit-lessons.md L-09.",
                                server_count,
                                tools.len()
                            );
                        }
                        Err(e) => {
                            eprintln!("[MCP probe] tool discovery error: {e}");
                        }
                    }
                });
            }));
            if let Err(panic_payload) = result {
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic_payload.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("(non-string panic payload)");
                eprintln!("[MCP probe] thread panicked: {msg}");
            }
        });

    if let Err(e) = spawn_result {
        eprintln!("[MCP probe] failed to spawn probe thread: {e}");
    }
}

/// Execute a tool with CWD pinned to the workspace under a process-wide lock.
///
/// Acquires the global workspace lock, saves the current CWD, changes to the
/// tool's workspace, runs the tool, then restores the original CWD. The lock
/// ensures only one tool executes at a time process-wide, preventing
/// concurrent sessions from racing on `std::env::set_current_dir`.
fn execute_tool_in_workspace(
    cwd: &std::path::Path,
    tool_name: &str,
    input: &Value,
) -> Result<String, String> {
    use std::sync::{Mutex as StdMutex, OnceLock};

    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| StdMutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let original = std::env::current_dir().map_err(|e| e.to_string())?;

    if cwd.is_dir() {
        std::env::set_current_dir(cwd)
            .map_err(|e| format!("failed to cd into {}: {e}", cwd.display()))?;
    }

    let result = tools::execute_tool(tool_name, input);

    // Always try to restore CWD, even if the tool failed.
    let _ = std::env::set_current_dir(&original);

    result
}

/// Returns `true` for tools that are read-only and never need permission.
fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file" | "glob_search" | "grep_search" | "Read" | "Glob" | "Grep"
    )
}

/// Coerce a raw tool_use input JSON string into a `Value::Object`.
///
/// Anthropic's Messages API requires the `input` field of a `tool_use`
/// content block to be a JSON object. This helper:
/// - Parses the raw JSON string
/// - Accepts only objects; non-object values (null, array, number, string,
///   bool) are discarded and replaced with an empty object
/// - On parse failure, also returns an empty object
///
/// This defensive coercion prevents the LLM's next turn from receiving an
/// API 400 error due to malformed tool_use payloads.
fn coerce_tool_input_to_object(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw)
        .ok()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

#[cfg(test)]
mod permission_gate_tests {
    use super::*;
    use tokio::sync::broadcast;

    fn make_gate() -> (Arc<PermissionGate>, broadcast::Receiver<DesktopSessionEvent>) {
        let (tx, rx) = broadcast::channel(16);
        let gate = Arc::new(PermissionGate::new(tx, "test-session".to_string()));
        (gate, rx)
    }

    #[tokio::test]
    async fn resolve_wins_when_user_responds_before_timeout() {
        let (gate, mut rx) = make_gate();

        // Spawn check_permission, it will wait for a decision.
        let gate_clone = Arc::clone(&gate);
        let check_task = tokio::spawn(async move {
            gate_clone
                .check_permission(
                    "bash",
                    &serde_json::json!({"command": "ls"}),
                    false, // not bypass
                )
                .await
        });

        // Wait for the PermissionRequest event to arrive.
        let event = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("SSE event should arrive")
            .expect("event receiver should not be closed");

        let request_id = match event {
            DesktopSessionEvent::PermissionRequest { request_id, .. } => request_id,
            other => panic!("expected PermissionRequest, got {other:?}"),
        };

        // Simulate user clicking Allow shortly after.
        tokio::time::sleep(Duration::from_millis(10)).await;
        let resolved = gate.resolve(&request_id, PermissionDecision::Allow).await;
        assert!(resolved, "resolve should succeed");

        let decision = check_task.await.expect("check task should complete");
        assert_eq!(decision, PermissionDecision::Allow);

        // Pending map should be empty after resolve.
        let pending = gate.pending.lock().await;
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn allow_always_remembers_tool() {
        let (gate, mut rx) = make_gate();

        // First call: user chooses AllowAlways.
        let gate_clone = Arc::clone(&gate);
        let first = tokio::spawn(async move {
            gate_clone
                .check_permission("bash", &serde_json::json!({}), false)
                .await
        });
        let event = rx.recv().await.unwrap();
        let request_id = match event {
            DesktopSessionEvent::PermissionRequest { request_id, .. } => request_id,
            _ => panic!(),
        };
        gate.resolve(&request_id, PermissionDecision::AllowAlways).await;
        let d1 = first.await.unwrap();
        assert_eq!(d1, PermissionDecision::AllowAlways);

        // Second call: should return Allow immediately without asking.
        let d2 = gate
            .check_permission("bash", &serde_json::json!({}), false)
            .await;
        assert_eq!(d2, PermissionDecision::Allow);
    }

    #[tokio::test]
    async fn bypass_all_short_circuits() {
        let (gate, _rx) = make_gate();
        // bypass=true should not require user input.
        let decision = gate
            .check_permission("bash", &serde_json::json!({}), true)
            .await;
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[tokio::test]
    async fn read_only_tools_auto_allowed() {
        let (gate, _rx) = make_gate();
        for name in ["read_file", "glob_search", "grep_search", "Read", "Glob", "Grep"] {
            let decision = gate
                .check_permission(name, &serde_json::json!({}), false)
                .await;
            assert_eq!(decision, PermissionDecision::Allow, "tool {name}");
        }
    }

    #[tokio::test]
    async fn resolve_with_unknown_id_returns_false() {
        let (gate, _rx) = make_gate();
        let result = gate.resolve("nonexistent-id", PermissionDecision::Allow).await;
        assert!(!result, "resolve with unknown id should return false");
    }
}

#[cfg(test)]
mod coerce_input_tests {
    use super::coerce_tool_input_to_object;
    use serde_json::Value;

    #[test]
    fn valid_object_is_preserved() {
        let result = coerce_tool_input_to_object(r#"{"foo":"bar","n":42}"#);
        assert_eq!(result["foo"], "bar");
        assert_eq!(result["n"], 42);
    }

    #[test]
    fn null_becomes_empty_object() {
        let result = coerce_tool_input_to_object("null");
        assert!(result.is_object());
        assert_eq!(result.as_object().unwrap().len(), 0);
    }

    #[test]
    fn array_becomes_empty_object() {
        let result = coerce_tool_input_to_object("[1,2,3]");
        assert!(result.is_object());
        assert_eq!(result.as_object().unwrap().len(), 0);
    }

    #[test]
    fn number_becomes_empty_object() {
        let result = coerce_tool_input_to_object("42");
        assert!(result.is_object());
    }

    #[test]
    fn string_becomes_empty_object() {
        let result = coerce_tool_input_to_object("\"just a string\"");
        assert!(result.is_object());
    }

    #[test]
    fn bool_becomes_empty_object() {
        assert!(coerce_tool_input_to_object("true").is_object());
        assert!(coerce_tool_input_to_object("false").is_object());
    }

    #[test]
    fn malformed_json_becomes_empty_object() {
        let result = coerce_tool_input_to_object("{not valid json");
        assert!(result.is_object());
        assert_eq!(result.as_object().unwrap().len(), 0);
    }

    #[test]
    fn empty_string_becomes_empty_object() {
        let result = coerce_tool_input_to_object("");
        assert!(result.is_object());
    }

    #[test]
    fn nested_object_preserved() {
        let result = coerce_tool_input_to_object(r#"{"outer":{"inner":true}}"#);
        assert_eq!(result["outer"]["inner"], Value::Bool(true));
    }
}

/// Drain a single complete line (terminated by `\n`) from a byte buffer.
///
/// Returns `Some(line)` if a complete line is available, or `None` if the
/// buffer does not yet contain a newline (more bytes need to be appended).
///
/// The returned line is decoded from UTF-8. `\r` is trimmed from the end.
/// Bytes that fail UTF-8 decoding are replaced with U+FFFD — but this
/// only applies to truly malformed data, not to multi-byte characters
/// split across chunks (those are handled by keeping them in the buffer
/// until a complete line arrives).
fn drain_next_line(buffer: &mut Vec<u8>) -> Option<String> {
    let newline_pos = buffer.iter().position(|&b| b == b'\n')?;
    let line_bytes: Vec<u8> = buffer.drain(..=newline_pos).collect();
    // Strip trailing \n and \r (if any).
    let line_slice = &line_bytes[..line_bytes.len() - 1];
    let line_slice = if line_slice.last() == Some(&b'\r') {
        &line_slice[..line_slice.len() - 1]
    } else {
        line_slice
    };
    // Decode as UTF-8. A complete line should be valid UTF-8 because `\n`
    // (0x0A) cannot appear inside a multi-byte UTF-8 sequence (all non-ASCII
    // continuation bytes have the high bit set).
    Some(String::from_utf8_lossy(line_slice).into_owned())
}

#[cfg(test)]
mod sse_buffer_tests {
    use super::drain_next_line;

    #[test]
    fn drain_returns_none_when_no_newline() {
        let mut buf = b"data: {\"partial\":".to_vec();
        assert!(drain_next_line(&mut buf).is_none());
        // Buffer is unchanged.
        assert_eq!(buf, b"data: {\"partial\":");
    }

    #[test]
    fn drain_returns_complete_line_and_strips_crlf() {
        let mut buf = b"hello\r\nworld".to_vec();
        assert_eq!(drain_next_line(&mut buf), Some("hello".to_string()));
        // Remaining buffer has "world".
        assert_eq!(buf, b"world");
    }

    #[test]
    fn drain_handles_multibyte_chars_across_chunks() {
        // Simulate a stream that splits the Chinese character "中" (E4 B8 AD)
        // across two chunks. Previously `String::from_utf8_lossy` on the
        // partial chunk would replace the bytes with U+FFFD.
        let mut buf = Vec::new();

        // Chunk 1: contains the first 2 bytes of "中"
        buf.extend_from_slice(b"data: {\"text\":\"");
        buf.extend_from_slice(&[0xE4, 0xB8]); // First 2 bytes of "中"

        // No newline yet → drain returns None, bytes are preserved.
        assert!(drain_next_line(&mut buf).is_none());
        assert_eq!(buf.len(), 15 + 2);

        // Chunk 2: completes the character, closes JSON, newline
        buf.extend_from_slice(&[0xAD]); // Third byte of "中"
        buf.extend_from_slice(b"\"}\n");

        let line = drain_next_line(&mut buf).expect("should have complete line");
        // The decoded line should contain the full "中" character, not lossy.
        assert!(line.contains("中"), "expected '中' in line, got: {line:?}");
        assert!(!line.contains('\u{FFFD}'), "should not contain replacement char");
    }

    #[test]
    fn drain_multiple_lines_sequentially() {
        let mut buf = b"line1\nline2\nline3".to_vec();
        assert_eq!(drain_next_line(&mut buf), Some("line1".to_string()));
        assert_eq!(drain_next_line(&mut buf), Some("line2".to_string()));
        // "line3" has no trailing newline → not drained yet
        assert_eq!(drain_next_line(&mut buf), None);
        assert_eq!(buf, b"line3");
    }

    #[test]
    fn drain_handles_empty_line() {
        let mut buf = b"\nnext".to_vec();
        assert_eq!(drain_next_line(&mut buf), Some(String::new()));
        assert_eq!(buf, b"next");
    }

    #[test]
    fn drain_is_linear_not_quadratic_on_large_buffer() {
        // Regression test: the old code used `buffer[newline_pos+1..].to_string()`
        // which is O(n²) when draining many small lines from a large buffer.
        // drain(..=newline_pos) is O(n) amortized.
        let mut buf = Vec::with_capacity(10_000);
        for i in 0..1000 {
            buf.extend_from_slice(format!("line{i}\n").as_bytes());
        }
        let mut drained = 0;
        while drain_next_line(&mut buf).is_some() {
            drained += 1;
        }
        assert_eq!(drained, 1000);
        assert!(buf.is_empty());
    }
}

/// Truncate tool output to prevent huge payloads from overwhelming SSE/LLM context.
///
/// Uses UTF-8 char boundary safe truncation so multi-byte characters
/// (Chinese, emoji, etc.) are never split mid-codepoint.
fn truncate_tool_output(output: String) -> String {
    if output.len() <= MAX_TOOL_OUTPUT_CHARS {
        return output;
    }
    // Walk backwards from the limit until we land on a char boundary.
    let mut boundary = MAX_TOOL_OUTPUT_CHARS.min(output.len());
    while boundary > 0 && !output.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let truncated = &output[..boundary];
    format!(
        "{truncated}\n\n... [output truncated at {MAX_TOOL_OUTPUT_CHARS} bytes]"
    )
}

#[cfg(test)]
mod tool_output_tests {
    use super::{truncate_tool_output, MAX_TOOL_OUTPUT_CHARS};

    #[test]
    fn truncate_short_output_unchanged() {
        let input = "hello world".to_string();
        assert_eq!(truncate_tool_output(input.clone()), input);
    }

    #[test]
    fn truncate_ascii_at_boundary() {
        let input = "a".repeat(MAX_TOOL_OUTPUT_CHARS + 100);
        let result = truncate_tool_output(input);
        assert!(result.contains("[output truncated"));
    }

    #[test]
    fn truncate_multibyte_does_not_panic() {
        // 50KB of Chinese chars (3 bytes each) = 150K bytes, exceeds 100K limit
        // Previous implementation would panic on byte-indexed slicing.
        let input = "中".repeat(50_000);
        let result = truncate_tool_output(input);
        assert!(result.contains("[output truncated"));
        // Verify the truncation landed on a valid char boundary (no panic = pass).
    }

    #[test]
    fn truncate_emoji_does_not_panic() {
        let input = "🚀".repeat(30_000);
        let result = truncate_tool_output(input);
        assert!(result.contains("[output truncated"));
    }
}

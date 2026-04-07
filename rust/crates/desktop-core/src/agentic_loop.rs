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
    pending: Mutex<Option<PendingPermission>>,
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
            pending: Mutex::new(None),
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
            *pending = Some(PendingPermission {
                request_id: request_id.clone(),
                tool_name: tool_name.to_string(),
                tool_input: tool_input.clone(),
                sender,
            });
        }

        // Broadcast the permission request to the frontend.
        let _ = self.event_sender.send(DesktopSessionEvent::PermissionRequest {
            session_id: self.session_id.clone(),
            request_id: request_id.clone(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.to_string(),
        });

        // Wait for the user's response (with timeout).
        match tokio::time::timeout(Duration::from_secs(PERMISSION_TIMEOUT_SECS), receiver).await {
            Ok(Ok(decision)) => {
                // If user chose AllowAlways, remember it for this session.
                if decision == PermissionDecision::AllowAlways {
                    let mut always = self.always_allow.lock().await;
                    always.insert(tool_name.to_string());
                }
                decision
            }
            Ok(Err(_)) => PermissionDecision::Deny {
                reason: "permission channel closed".into(),
            },
            Err(_) => PermissionDecision::Deny {
                reason: "permission request timed out (5 min)".into(),
            },
        }
    }

    /// Resolve a pending permission request (called by the HTTP handler).
    pub async fn resolve(&self, request_id: &str, decision: PermissionDecision) -> bool {
        let mut pending = self.pending.lock().await;
        if let Some(p) = pending.take() {
            if p.request_id == request_id {
                let _ = p.sender.send(decision);
                return true;
            }
            // ID mismatch: put it back.
            *pending = Some(p);
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

    // ── Initialize MCP servers (if configured) ───────────────────
    if !config.mcp_servers.is_empty() {
        init_mcp_servers(&config.mcp_servers);
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
                        // Execute the tool in a blocking thread with CWD set to project path.
                        let name = tool_name.clone();
                        let input_value = tool_input_value.clone();
                        let tool_cwd = config.project_path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            if tool_cwd.is_dir() {
                                let _ = std::env::set_current_dir(&tool_cwd);
                            }
                            tools::execute_tool(&name, &input_value)
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
                    ContentBlock::ToolUse { id, name, input } => serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input
                    }),
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
    let mut buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("SSE stream error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete lines from buffer.
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
            buffer = buffer[newline_pos + 1..].to_string();

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

/// Initialize MCP servers from the frontend settings into the global registry.
///
/// Converts the simplified `McpServerEntry` descriptors into the runtime's
/// `ScopedMcpServerConfig` format, creates a `McpServerManager`, discovers
/// tools, and registers everything into the global MCP tool registry.
/// Initialize MCP servers from the frontend settings.
///
/// Converts simplified `McpServerEntry` descriptors into the runtime's
/// `ScopedMcpServerConfig` format, creates a `McpServerManager`, discovers
/// tools, and logs the results. MCP tool calls are then available via the
/// global `McpToolRegistry` used by `execute_tool("MCP", ...)`.
fn init_mcp_servers(servers: &[McpServerEntry]) {
    use runtime::{McpServerConfig, McpServerManager, McpStdioServerConfig, ScopedMcpServerConfig};
    use std::collections::BTreeMap;

    let enabled: Vec<&McpServerEntry> = servers.iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return;
    }

    let mut server_configs: BTreeMap<String, ScopedMcpServerConfig> = BTreeMap::new();
    for entry in &enabled {
        // Only stdio transport is currently supported by the vendored runtime.
        if entry.transport != "stdio" {
            eprintln!(
                "MCP server '{}': transport '{}' not supported, skipping",
                entry.name, entry.transport
            );
            continue;
        }

        // Parse target as "command arg1 arg2 ..."
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

    // Spawn MCP connection in a background thread with its own tokio runtime.
    let server_count = server_configs.len();
    std::thread::Builder::new()
        .name("mcp-init".to_string())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("MCP init: failed to create runtime: {e}");
                    return;
                }
            };

            rt.block_on(async move {
                let mut manager = McpServerManager::from_servers(&server_configs);
                match manager.discover_tools().await {
                    Ok(tools) => {
                        eprintln!(
                            "MCP: discovered {} tools from {} servers",
                            tools.len(),
                            server_count
                        );
                    }
                    Err(e) => {
                        eprintln!("MCP: tool discovery error: {e}");
                    }
                }
            });
        })
        .ok();
}

/// Returns `true` for tools that are read-only and never need permission.
fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file" | "glob_search" | "grep_search" | "Read" | "Glob" | "Grep"
    )
}

/// Truncate tool output to prevent huge payloads from overwhelming SSE/LLM context.
fn truncate_tool_output(output: String) -> String {
    if output.len() <= MAX_TOOL_OUTPUT_CHARS {
        return output;
    }
    let truncated = &output[..MAX_TOOL_OUTPUT_CHARS];
    format!(
        "{truncated}\n\n... [output truncated at {MAX_TOOL_OUTPUT_CHARS} characters]"
    )
}

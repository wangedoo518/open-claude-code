//! Async agentic conversation loop.
//!
//! Reimplements the synchronous `ConversationRuntime::run_turn` as a fully async
//! loop with incremental SSE broadcasting and async permission prompting.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use runtime::{ContentBlock, ConversationMessage, Session as RuntimeSession};
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

    loop {
        // ── Check limits ─────────────────────────────────────────
        iterations += 1;
        if iterations > MAX_LOOP_ITERATIONS {
            return Err(AgenticError::MaxIterationsExceeded);
        }
        if cancel_token.is_cancelled() {
            return Ok(AgenticTurnResult {
                session: current_session,
                model_label,
                iterations,
                was_cancelled: true,
            });
        }

        // ── Build Anthropic Messages API request ─────────────────
        let api_request = build_api_request(
            &current_session,
            &config.model,
            config.system_prompt.as_deref(),
            &tool_specs,
        );

        // ── Call LLM via code-tools-bridge ───────────────────────
        let response = call_llm_api(&client, &config.bridge_base_url, &config.bearer_token, &api_request)
            .await
            .map_err(AgenticError::ApiError)?;

        // Extract model label from response.
        if let Some(m) = response.get("model").and_then(|v| v.as_str()) {
            model_label = m.to_string();
        }

        // Parse assistant message from response.
        let (assistant_message, stop_reason) = parse_assistant_response(&response)
            .map_err(|e| AgenticError::Internal(e.to_string()))?;

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
                    // Execute the tool in a blocking thread.
                    let name = tool_name.clone();
                    let input_value = tool_input_value.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        tools::execute_tool(&name, &input_value)
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("tool task panicked: {e}")));

                    match result {
                        Ok(output) => ConversationMessage::tool_result(
                            tool_use_id.clone(),
                            tool_name.clone(),
                            output,
                            false,
                        ),
                        Err(error) => ConversationMessage::tool_result(
                            tool_use_id.clone(),
                            tool_name.clone(),
                            error,
                            true,
                        ),
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
        "stream": false
    });

    if let Some(system) = system_prompt {
        request["system"] = serde_json::json!(system);
    }

    request
}

/// Call the LLM API via the code-tools-bridge proxy (non-streaming for Phase 1).
async fn call_llm_api(
    client: &reqwest::Client,
    bridge_base_url: &str,
    bearer_token: &str,
    request_body: &Value,
) -> Result<Value, String> {
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

    response
        .json::<Value>()
        .await
        .map_err(|e| format!("failed to parse LLM response: {e}"))
}

/// Parse an Anthropic Messages API response into a `ConversationMessage`.
fn parse_assistant_response(response: &Value) -> Result<(ConversationMessage, Option<String>), String> {
    let stop_reason = response
        .get("stop_reason")
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
                // Unknown block type: treat as text.
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

    Ok((message, stop_reason))
}

/// Returns `true` for tools that are read-only and never need permission.
fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file" | "glob_search" | "grep_search" | "Read" | "Glob" | "Grep"
    )
}

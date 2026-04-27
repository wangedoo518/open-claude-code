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

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use runtime::{ContentBlock, ConversationMessage, MessageRole};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::tool_execution::ToolExposurePolicy;
use crate::DesktopSessionEvent;

const FINAL_SYNTHESIS_SYSTEM_INSTRUCTION: &str = "\
最终整合轮：本次请求已禁用工具。请只基于前文对话和已经返回的工具结果，\
直接给用户完整、流畅的最终回答。不要再说“我将搜索”“我会读取”“我将提取”\
“我会运行命令”或“我会调用工具”。如果信息不足，请说明限制，并基于现有上下文\
给出最好的最终答案。";

const TOOL_DISABLED_DIRECT_ANSWER_INSTRUCTION: &str = "\
本次请求已禁用工具。请直接基于对话回答，不要说你将搜索、抓取网页、读取文件、\
运行命令或调用工具。";

/// Internal control tokens that may leak into content streams.
///
/// Some providers, notably DeepSeek, use these markers to delimit structured
/// tool outputs. When a synthesis turn is text-only, the markers may appear in
/// `delta.content`; strip them before they reach the UI.
const KNOWN_LEAK_TOKENS: &[&str] = &[
    "<｜DSML｜tool_calls｜>",
    "<｜DSML｜tool_calls>",
    "<｜DSML｜begin_tool_calls｜>",
    "<｜DSML｜end_tool_calls｜>",
    "<｜DSML｜begin_tool_calls>",
    "<｜DSML｜end_tool_calls>",
    "<|tool_call|>",
    "<|end_tool_call|>",
];

fn strip_leak_tokens(content: &str) -> std::borrow::Cow<'_, str> {
    if !content.contains('<') {
        return std::borrow::Cow::Borrowed(content);
    }

    let mut current = content.to_string();
    let original = current.clone();

    for token in KNOWN_LEAK_TOKENS {
        if current.contains(token) {
            current = current.replace(token, "");
        }
    }

    if contains_dsml_tag(&current) {
        current = strip_dsml_pattern(&current);
    }

    if current == original {
        std::borrow::Cow::Borrowed(content)
    } else {
        std::borrow::Cow::Owned(current)
    }
}

fn contains_dsml_tag(s: &str) -> bool {
    s.contains("<｜DSML｜")
        || s.contains("</｜DSML｜")
        || s.contains("<|DSML|")
        || s.contains("</|DSML|")
}

/// Strip DSML-like tags while preserving the human-readable content between
/// tags, for example:
/// `<｜DSML｜parameter name="url">https://x</｜DSML｜parameter>`
/// becomes `https://x`.
fn strip_dsml_pattern(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut index = 0;

    while index < s.len() {
        let remaining = &s[index..];
        let is_dsml_tag = remaining.starts_with("<｜DSML｜")
            || remaining.starts_with("</｜DSML｜")
            || remaining.starts_with("<|DSML|")
            || remaining.starts_with("</|DSML|");

        if is_dsml_tag {
            if let Some(end_offset) = remaining.find('>') {
                index += end_offset + '>'.len_utf8();
                continue;
            }
        }

        let Some(ch) = remaining.chars().next() else {
            break;
        };
        result.push(ch);
        index += ch.len_utf8();
    }

    result
}

// ============================================================================
// OpenAI-compatible tool calling support
//
// Represents the streaming tool_calls protocol from OpenAI ChatCompletions.
// Tool calls arrive in fragments and must be accumulated by `index`.
//
// Protocol observed in production (DeepSeek 2026-04):
//   chunk 1: {"index":0, "id":"call_xxx", "type":"function",
//             "function":{"name":"get_weather", "arguments":""}}
//   chunk 2: {"index":0, "function":{"arguments":"{"}}
//   chunk 3: {"index":0, "function":{"arguments":"\"city\":\""}}
//   chunk 4: {"index":0, "function":{"arguments":"Beijing\"}"}}
//   final:   {"finish_reason":"tool_calls"}
// ============================================================================

/// Single tool call delta as it appears in a single SSE chunk.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAiToolCallDelta {
    /// Position of this tool call in the parallel tool_calls array
    /// (always required, even in continuation chunks).
    pub index: usize,

    /// Tool call ID, present only in the first chunk for this index.
    #[serde(default)]
    pub id: Option<String>,

    /// Always "function" in current OpenAI spec.
    #[serde(rename = "type", default)]
    pub call_type: Option<String>,

    /// Function name and arguments fragment.
    #[serde(default)]
    pub function: Option<OpenAiFunctionDelta>,
}

/// Function call fragment in a single chunk.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAiFunctionDelta {
    /// Function name, present only in the first chunk.
    #[serde(default)]
    pub name: Option<String>,

    /// JSON arguments string fragment.
    /// Each chunk contains a fragment; concatenate to form valid JSON.
    /// CRITICAL: Do not parse until accumulation is complete.
    #[serde(default)]
    pub arguments: Option<String>,
}

/// Accumulated state for a single tool call across multiple SSE chunks.
#[derive(Debug, Clone, Default)]
pub struct ToolCallAccumulator {
    /// Original index from the streaming protocol.
    pub index: usize,

    /// Tool call ID (received in first chunk for this index).
    pub id: String,

    /// Function name (received in first chunk for this index).
    pub name: String,

    /// Accumulated arguments JSON string (built up across chunks).
    pub arguments: String,
}

impl ToolCallAccumulator {
    /// Apply a delta to this accumulator.
    pub fn apply_delta(&mut self, delta: &OpenAiToolCallDelta) {
        if let Some(id) = &delta.id {
            // First chunk only; later chunks will not have this.
            if self.id.is_empty() {
                self.id = id.clone();
            }
        }
        if let Some(func) = &delta.function {
            if let Some(name) = &func.name {
                if self.name.is_empty() {
                    self.name = name.clone();
                }
            }
            if let Some(args_frag) = &func.arguments {
                self.arguments.push_str(args_frag);
            }
        }
    }

    /// Try to parse the accumulated arguments as JSON.
    /// Returns Err if the arguments are incomplete or malformed.
    pub fn parse_arguments(&self) -> Result<serde_json::Value, String> {
        if self.arguments.is_empty() {
            return Ok(serde_json::json!({}));
        }
        serde_json::from_str(&self.arguments).map_err(|e| {
            format!(
                "Tool call '{}' (id={}) has malformed arguments JSON: {}. Raw: {:?}",
                self.name, self.id, e, self.arguments
            )
        })
    }

    /// Validate this accumulator has the minimum required fields.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err(format!(
                "Tool call at index {} missing id (was first chunk lost?)",
                self.index
            ));
        }
        if self.name.is_empty() {
            return Err(format!(
                "Tool call '{}' (id={}) missing function name",
                self.id, self.id
            ));
        }
        Ok(())
    }
}

/// Map of tool calls being accumulated, keyed by their `index` field.
/// Using BTreeMap for deterministic ordering when iterating.
pub type ToolCallMap = BTreeMap<usize, ToolCallAccumulator>;

/// Reasons a streaming turn can finish, derived from the `finish_reason`
/// field in the final chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamFinishReason {
    /// Normal completion - assistant message ended.
    Stop,
    /// Model wants to call tools - we should execute and continue.
    ToolCalls,
    /// Hit max_tokens limit.
    Length,
    /// Content filter triggered.
    ContentFilter,
    /// Unknown or missing finish_reason.
    Other(String),
}

impl StreamFinishReason {
    pub fn from_str(s: &str) -> Self {
        match s {
            "stop" => StreamFinishReason::Stop,
            "tool_calls" => StreamFinishReason::ToolCalls,
            "length" => StreamFinishReason::Length,
            "content_filter" => StreamFinishReason::ContentFilter,
            other => StreamFinishReason::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptIntent {
    Writing,
    SmallTalk,
    Search,
    Code,
    File,
    Ambiguous,
}

impl PromptIntent {
    fn allows_tools(self) -> bool {
        !matches!(self, PromptIntent::Writing | PromptIntent::SmallTalk)
    }
}

fn classify_latest_user_prompt_intent(messages: &[ConversationMessage]) -> PromptIntent {
    latest_user_prompt(messages)
        .as_deref()
        .map(classify_prompt_intent)
        .unwrap_or(PromptIntent::Ambiguous)
}

fn latest_user_prompt(messages: &[ConversationMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| extract_text_from_blocks(&message.blocks))
        .filter(|text| !text.trim().is_empty())
}

fn classify_prompt_intent(prompt: &str) -> PromptIntent {
    let normalized = normalize_prompt(prompt);
    if normalized.is_empty() {
        return PromptIntent::Ambiguous;
    }

    if looks_like_smalltalk_prompt(&normalized) {
        return PromptIntent::SmallTalk;
    }
    if looks_like_explicit_search_request(&normalized) {
        return PromptIntent::Search;
    }
    if looks_like_code_request(&normalized) {
        return PromptIntent::Code;
    }
    if looks_like_file_request(&normalized) {
        return PromptIntent::File;
    }
    if looks_like_writing_request(&normalized) {
        return PromptIntent::Writing;
    }
    if looks_like_broad_search_request(&normalized) {
        return PromptIntent::Search;
    }

    PromptIntent::Ambiguous
}

fn prompt_requests_forbidden_tool_action(prompt: &str, policy: &ToolExposurePolicy) -> bool {
    let normalized = normalize_prompt(prompt);
    if normalized.is_empty() {
        return false;
    }

    (!policy.include_filesystem_write && looks_like_filesystem_write_request(&normalized))
        || (!policy.include_shell && looks_like_shell_request(&normalized))
        || looks_like_agentic_tool_request(&normalized)
}

fn normalize_prompt(prompt: &str) -> String {
    prompt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_smalltalk_prompt(prompt: &str) -> bool {
    let trimmed = prompt.trim_matches(|ch: char| ch.is_ascii_punctuation() || ch.is_whitespace());
    matches!(
        trimmed,
        "hi" | "hello"
            | "hey"
            | "thanks"
            | "thank you"
            | "ok"
            | "okay"
            | "good morning"
            | "good night"
            | "\u{4f60}\u{597d}"
            | "\u{8c22}\u{8c22}"
    ) || contains_any(
        trimmed,
        &[
            "how are you",
            "who are you",
            "what can you do",
            "\u{4f60}\u{597d}",
            "\u{8c22}\u{8c22}",
        ],
    )
}

fn looks_like_explicit_search_request(prompt: &str) -> bool {
    contains_any(
        prompt,
        &[
            "search",
            "look up",
            "lookup",
            "browse",
            "google",
            "web",
            "internet",
            "http://",
            "https://",
            "www.",
            "site:",
            "\u{641c}\u{4e00}\u{4e0b}",
            "\u{641c}\u{641c}",
            "\u{641c}\u{7d22}",
            "\u{67e5}\u{4e00}\u{4e0b}",
            "\u{67e5}\u{67e5}",
            "\u{8054}\u{7f51}",
            "\u{4e0a}\u{7f51}",
        ],
    )
}

fn looks_like_broad_search_request(prompt: &str) -> bool {
    contains_any(
        prompt,
        &[
            "latest",
            "today",
            "current",
            "news",
            "weather",
            "stock price",
            "exchange rate",
            "\u{6700}\u{65b0}",
            "\u{4eca}\u{5929}",
            "\u{65b0}\u{95fb}",
        ],
    )
}

fn looks_like_code_request(prompt: &str) -> bool {
    contains_any(
        prompt,
        &[
            "code",
            "function",
            "class",
            "method",
            "bug",
            "stack trace",
            "compile",
            "compiler",
            "cargo",
            "npm",
            "typescript",
            "javascript",
            "python",
            "rust",
            "sql",
            "regex",
            "api",
            "endpoint",
            "test failure",
            "\u{4ee3}\u{7801}",
            "\u{51fd}\u{6570}",
        ],
    )
}

fn looks_like_file_request(prompt: &str) -> bool {
    contains_any(
        prompt,
        &[
            "file",
            "folder",
            "directory",
            "workspace",
            "repo",
            "repository",
            "path",
            "read_file",
            "grep",
            "glob",
            "open ",
            "inspect ",
            "find in ",
            ".rs",
            ".ts",
            ".tsx",
            ".js",
            ".jsx",
            ".json",
            ".toml",
            ".md",
            ".py",
            ".go",
            ".java",
            ".cpp",
            ".css",
            ".html",
            "\\",
            "/",
            "\u{6587}\u{4ef6}",
            "\u{76ee}\u{5f55}",
        ],
    )
}

fn looks_like_writing_request(prompt: &str) -> bool {
    contains_any(
        prompt,
        &[
            "write",
            "draft",
            "compose",
            "rewrite",
            "polish",
            "translate",
            "summarize",
            "outline",
            "brainstorm",
            "email",
            "essay",
            "poem",
            "story",
            "copy",
            "tagline",
            "blurb",
            "title",
            "name ideas",
            "make it sound",
            "improve this text",
            "grammar",
            "\u{5199}",
            "\u{64b0}",
            "\u{7ffb}\u{8bd1}",
            "\u{6da6}\u{8272}",
            "\u{603b}\u{7ed3}",
        ],
    )
}

fn looks_like_filesystem_write_request(prompt: &str) -> bool {
    let write_action = contains_any(
        prompt,
        &[
            "write to",
            "write into",
            "save to",
            "save as",
            "create file",
            "create a file",
            "edit file",
            "modify file",
            "update file",
            "delete file",
            "remove file",
            "append to",
            "replace in",
            "patch file",
            "apply patch",
            "rename file",
            "move file",
            "touch ",
            "\u{4fee}\u{6539}",
            "\u{7f16}\u{8f91}",
            "\u{5220}\u{9664}",
            "\u{521b}\u{5efa}",
            "\u{65b0}\u{5efa}",
            "\u{4fdd}\u{5b58}",
            "\u{5199}\u{5165}",
            "\u{4fdd}\u{5b58}\u{5230}",
            "\u{521b}\u{5efa}\u{6587}\u{4ef6}",
            "\u{4fee}\u{6539}\u{6587}\u{4ef6}",
            "\u{5220}\u{9664}\u{6587}\u{4ef6}",
        ],
    );

    write_action
        || (contains_any(prompt, &["write", "edit", "modify", "update"])
            && looks_like_file_request(prompt))
}

fn looks_like_shell_request(prompt: &str) -> bool {
    contains_any(
        prompt,
        &[
            "run command",
            "execute command",
            "shell",
            "terminal",
            "bash",
            "powershell",
            "cmd.exe",
            "command line",
            "npm install",
            "pnpm install",
            "yarn install",
            "cargo ",
            "npm run",
            "cargo check",
            "cargo test",
            "run cargo",
            "run npm",
            "run python",
            "\u{8fd0}\u{884c} npm",
            "\u{6267}\u{884c} npm",
            "\u{8fd0}\u{884c} cargo",
            "\u{6267}\u{884c} cargo",
            "\u{8fd0}\u{884c}\u{547d}\u{4ee4}",
            "\u{6267}\u{884c}\u{547d}\u{4ee4}",
        ],
    )
}

fn looks_like_agentic_tool_request(prompt: &str) -> bool {
    let mentions_agent = contains_any(
        prompt,
        &[
            " agent",
            "agent ",
            "subagent",
            "sub-agent",
            "worker",
            "\u{4ee3}\u{7406}",
        ],
    );
    let asks_to_use = contains_any(
        prompt,
        &[
            "use ",
            "call ",
            "start ",
            "launch ",
            "spawn ",
            "delegate",
            "create an agent",
            "ask an agent",
            "\u{542f}\u{52a8}",
            "\u{8c03}\u{7528}",
        ],
    );
    mentions_agent && asks_to_use
}

fn build_tool_policy_system_prompt(policy: &ToolExposurePolicy) -> String {
    let mut allowed = Vec::new();
    let mut forbidden = Vec::new();

    if policy.include_safe {
        allowed.push(
            "只读文件/搜索工具（read_file, glob_search, grep_search）和联网查询工具（WebSearch, WebFetch）",
        );
    } else {
        forbidden.push("只读文件、搜索和联网查询工具");
    }

    if policy.include_filesystem_write {
        allowed.push("文件写入/编辑工具");
    } else {
        forbidden.push("文件写入或编辑");
    }

    if policy.include_shell {
        allowed.push("Shell 或终端执行工具");
    } else {
        forbidden.push("Shell、终端、Bash、PowerShell 或命令执行");
    }

    if policy.include_specialty {
        allowed.push("已暴露的专项工具，例如 TodoWrite");
    } else {
        forbidden.push("未显式暴露的专项工具");
    }

    forbidden.push("Agent/subagent 协作工具");
    forbidden.push("未知或未列出的工具");

    let allowed_text = if allowed.is_empty() {
        "无".to_string()
    } else {
        allowed.join("；")
    };

    format!(
        "本轮 OpenAI 兼容模型的工具安全策略：\n\
         - 当前允许使用：{allowed_text}。\n\
         - 当前禁止使用：{}。\n\
         - 只能调用本次请求里真实暴露的工具。\n\
         - 如果用户请求需要被禁止的能力（例如写入文件、编辑文件、运行 Shell 命令、启动 Agent/subagent），\
         不要先调用读取/搜索工具试探。请直接说明“当前安全策略不允许该操作”，并给出安全的文本替代方案。\n\
         - 不要承诺会使用不可用工具。",
        forbidden.join("；")
    )
}

fn build_turn_system_prompt(
    base_system_prompt: Option<&str>,
    tool_policy: Option<&ToolExposurePolicy>,
    tool_use_disabled: bool,
    final_synthesis: bool,
) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(base) = base_system_prompt {
        if !base.trim().is_empty() {
            parts.push(base.to_string());
        }
    }

    if let Some(policy) = tool_policy {
        parts.push(build_tool_policy_system_prompt(policy));
    }

    if final_synthesis {
        parts.push(FINAL_SYNTHESIS_SYSTEM_INSTRUCTION.to_string());
    } else if tool_use_disabled {
        parts.push(TOOL_DISABLED_DIRECT_ANSWER_INSTRUCTION.to_string());
    }

    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

fn build_openai_request_body(
    model: &str,
    messages_for_request: &[serde_json::Value],
    tool_specs: Option<&[serde_json::Value]>,
    tool_policy: &ToolExposurePolicy,
    allow_tools: bool,
) -> serde_json::Value {
    let mut request_body = serde_json::json!({
        "model": model,
        "messages": messages_for_request,
        "stream": true,
    });

    let Some(tools) = tool_specs else {
        return request_body;
    };
    if tools.is_empty() {
        return request_body;
    }

    if !allow_tools {
        request_body["tool_choice"] = serde_json::Value::String("none".into());
        return request_body;
    }

    let filtered: Vec<serde_json::Value> = tools
        .iter()
        .filter(|tool| {
            let name = tool
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(|name| name.as_str())
                .unwrap_or("");
            tool_policy.allows(name)
        })
        .cloned()
        .collect();

    if filtered.is_empty() {
        request_body["tool_choice"] = serde_json::Value::String("none".into());
    } else {
        request_body["tools"] = serde_json::Value::Array(filtered);
        request_body["tool_choice"] = serde_json::Value::String("auto".into());
    }

    request_body
}

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
    pub messages: Vec<ConversationMessage>,
    /// Optional system prompt; prepended as a `{role:"system"}` entry.
    pub system_prompt: Option<String>,
    /// P1-1: Throttled (5s) callback invoked during stream to bump session
    /// updated_at. Prevents frontend isStale false-positives on long
    /// single responses (>30s Opus thinking, upstream slow-downs).
    /// Not required for correctness; finalize still sets updated_at.
    pub on_stream_tick: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Optional tools array in OpenAI ChatCompletions format.
    ///
    /// When `Some(non_empty)` and enabled for the prompt intent, the request
    /// body will include:
    ///   - `"tools": [...]` array
    ///   - `"tool_choice": "auto"`
    ///
    /// When disabled for a final/text-only/policy-refusal turn, the request
    /// sets `"tool_choice": "none"`.
    ///
    /// Tool specs should be pre-converted by the caller using
    /// `openai_tool_schema::to_openai_function_tool`.
    pub tool_specs: Option<Vec<serde_json::Value>>,
    /// Tool exposure policy. If None, defaults to safe read-only tools only.
    pub tool_policy: Option<ToolExposurePolicy>,
    /// Workspace path for built-in tool execution.
    pub workspace_path: Option<PathBuf>,
    /// Permission gate for built-in tool execution.
    pub permission_gate: Option<Arc<crate::agentic_loop::PermissionGate>>,
    /// If true, skip permission checks.
    pub bypass_permissions: bool,
    /// Tool execution timeout in seconds. Zero falls back to the shared default.
    pub tool_timeout_secs: u64,
}

/// Run an OpenAI-compatible ChatCompletions turn.
///
/// Supports multi-turn tool calling: model -> tool_use -> tool_result ->
/// model follow-up answer. The loop is capped to prevent runaway tool calls.
pub async fn run_streaming_turn(
    http_client: &reqwest::Client,
    config: StreamingTurnConfig,
    event_sender: &broadcast::Sender<DesktopSessionEvent>,
    session_id: &str,
    cancel_token: &CancellationToken,
) -> Result<(Vec<ConversationMessage>, Option<String>), String> {
    const MAX_TURNS: usize = 5;

    let mut current_messages = config.messages.clone();
    let mut all_new_messages: Vec<ConversationMessage> = Vec::new();
    let mut last_upstream_model: Option<String> = None;
    let tool_policy = config.tool_policy.clone().unwrap_or_default();
    let has_configured_tools = config
        .tool_specs
        .as_ref()
        .is_some_and(|tools| !tools.is_empty());
    let latest_user_text = latest_user_prompt(&current_messages).unwrap_or_default();
    let prompt_intent = classify_latest_user_prompt_intent(&current_messages);
    let prompt_needs_policy_refusal =
        prompt_requests_forbidden_tool_action(&latest_user_text, &tool_policy);
    let should_inject_tool_policy = has_configured_tools || prompt_needs_policy_refusal;

    if has_configured_tools {
        eprintln!(
            "[openai_compat] prompt intent={:?}, policy_refusal={}",
            prompt_intent, prompt_needs_policy_refusal
        );
    }

    for turn_idx in 0..MAX_TURNS {
        if cancel_token.is_cancelled() {
            eprintln!(
                "[openai_compat] turn {}: cancelled by user, stopping loop",
                turn_idx
            );
            break;
        }

        let is_final_synthesis_turn = turn_idx + 1 >= MAX_TURNS;
        let allow_tools = has_configured_tools
            && !is_final_synthesis_turn
            && prompt_intent.allows_tools()
            && !prompt_needs_policy_refusal;
        let turn_system_prompt = build_turn_system_prompt(
            config.system_prompt.as_deref(),
            should_inject_tool_policy.then_some(&tool_policy),
            (has_configured_tools && !allow_tools) || prompt_needs_policy_refusal,
            is_final_synthesis_turn,
        );
        let openai_messages =
            build_openai_messages(&current_messages, turn_system_prompt.as_deref());
        eprintln!(
            "[openai_compat] turn {}: sending {} messages",
            turn_idx,
            openai_messages.len()
        );

        if is_final_synthesis_turn && has_configured_tools {
            eprintln!(
                "[openai_compat] turn {}: final synthesis turn, disabling tools",
                turn_idx
            );
        } else if has_configured_tools && !allow_tools {
            if prompt_needs_policy_refusal {
                eprintln!(
                    "[openai_compat] turn {}: prompt asks for forbidden tool action, disabling tools",
                    turn_idx
                );
            } else {
                eprintln!(
                    "[openai_compat] turn {}: prompt intent {:?} is text-only, disabling tools",
                    turn_idx, prompt_intent
                );
            }
        }

        let (new_messages, upstream_model, finish_reason) = execute_single_turn(
            http_client,
            &config,
            &openai_messages,
            &tool_policy,
            allow_tools,
            event_sender,
            session_id,
            cancel_token,
        )
        .await?;

        if let Some(model) = upstream_model {
            last_upstream_model = Some(model);
        }

        for msg in &new_messages {
            current_messages.push(msg.clone());
            all_new_messages.push(msg.clone());
        }

        match finish_reason {
            StreamFinishReason::ToolCalls => {
                let has_tool_result = new_messages
                    .iter()
                    .any(|message| message.role == MessageRole::Tool);
                if !has_tool_result {
                    eprintln!(
                        "[openai_compat] turn {}: finish_reason=tool_calls but no tool_result produced, breaking loop",
                        turn_idx
                    );
                    break;
                }
                eprintln!(
                    "[openai_compat] turn {}: tool_calls finished, looping for next turn",
                    turn_idx
                );
                continue;
            }
            StreamFinishReason::Stop => {
                eprintln!(
                    "[openai_compat] turn {}: model returned stop, ending loop",
                    turn_idx
                );
                break;
            }
            StreamFinishReason::Length => {
                eprintln!(
                    "[openai_compat] turn {}: hit length limit, ending loop",
                    turn_idx
                );
                break;
            }
            StreamFinishReason::ContentFilter => {
                eprintln!(
                    "[openai_compat] turn {}: content filter triggered, ending loop",
                    turn_idx
                );
                break;
            }
            StreamFinishReason::Other(other) => {
                eprintln!(
                    "[openai_compat] turn {}: unknown finish_reason={}, ending loop",
                    turn_idx, other
                );
                break;
            }
        }
    }

    if all_new_messages
        .last()
        .is_some_and(|message| message.role == MessageRole::Tool)
    {
        eprintln!(
            "[openai_compat] reached MAX_TURNS={} with last message being tool_result; appending exhaustion notice",
            MAX_TURNS
        );
        all_new_messages.push(build_assistant_text_message(&format!(
            "[Reached max tool-call turns ({MAX_TURNS}); final answer was not completed. Retry or simplify the request.]"
        )));
    }

    Ok((all_new_messages, last_upstream_model))
}

async fn execute_single_turn(
    http_client: &reqwest::Client,
    config: &StreamingTurnConfig,
    messages_for_request: &[serde_json::Value],
    tool_policy: &ToolExposurePolicy,
    allow_tools: bool,
    event_sender: &broadcast::Sender<DesktopSessionEvent>,
    session_id: &str,
    cancel_token: &CancellationToken,
) -> Result<(Vec<ConversationMessage>, Option<String>, StreamFinishReason), String> {
    use futures_util::StreamExt;

    // ── Build request body ─────────────────────────────────────────
    let request_body = build_openai_request_body(
        &config.model,
        messages_for_request,
        config.tool_specs.as_deref(),
        tool_policy,
        allow_tools,
    );

    let configured_tool_count = config.tool_specs.as_ref().map_or(0, Vec::len);
    let allowed_tool_count = request_body
        .get("tools")
        .and_then(|tools| tools.as_array())
        .map_or(0, Vec::len);
    let tool_execution_enabled = allow_tools && allowed_tool_count > 0;
    if allow_tools && allowed_tool_count > 0 {
        let dropped_count = configured_tool_count.saturating_sub(allowed_tool_count);
        eprintln!(
            "[openai-compat-stream] session={session_id} tool policy: {} of {} tools allowed ({} filtered out)",
            allowed_tool_count,
            configured_tool_count,
            dropped_count
        );
    } else if allow_tools && configured_tool_count > 0 {
        eprintln!(
            "[openai-compat-stream] session={session_id} tool policy: all {} tools filtered out, request will be text-only",
            configured_tool_count
        );
    } else if configured_tool_count > 0 {
        eprintln!(
            "[openai-compat-stream] session={session_id} tool specs available but disabled; tool_choice=none"
        );
    }

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
    let mut tool_calls: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();
    let mut upstream_model: Option<String> = None;
    let mut delta_count: usize = 0;
    let mut last_finish_reason: Option<String> = None;
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
            if let Some(reason) = first.get("finish_reason").and_then(|v| v.as_str()) {
                last_finish_reason = Some(reason.to_string());
            }
            let Some(delta) = first.get("delta") else {
                continue;
            };
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                let cleaned = strip_leak_tokens(content);
                if !cleaned.is_empty() {
                    accumulated.push_str(&cleaned);
                    delta_count += 1;
                    let _ = event_sender.send(DesktopSessionEvent::TextDelta {
                        session_id: session_id.to_string(),
                        content: cleaned.to_string(),
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
            if let Some(deltas_arr) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for raw_delta in deltas_arr {
                    let parsed: OpenAiToolCallDelta =
                        match serde_json::from_value(raw_delta.clone()) {
                            Ok(d) => d,
                            Err(e) => {
                                eprintln!(
                                    "[openai_compat] failed to parse tool_call delta: {} (raw: {})",
                                    e, raw_delta
                                );
                                continue;
                            }
                        };

                    let acc =
                        tool_calls
                            .entry(parsed.index)
                            .or_insert_with(|| ToolCallAccumulator {
                                index: parsed.index,
                                ..Default::default()
                            });

                    acc.apply_delta(&parsed);
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

    if !tool_calls.is_empty() {
        eprintln!(
            "[openai_compat] stream ended with {} tool call(s)",
            tool_calls.len()
        );
        for (idx, acc) in &tool_calls {
            eprintln!(
                "[openai_compat]   tool_call[{}]: id={:?} name={:?} args_len={}",
                idx,
                acc.id,
                acc.name,
                acc.arguments.len()
            );
        }
    }

    let mut validated_tool_uses: Vec<(String, String, serde_json::Value)> = tool_calls
        .values()
        .filter_map(|acc| {
            if let Err(e) = acc.validate() {
                eprintln!("[openai_compat] dropping invalid tool_call: {}", e);
                return None;
            }
            let args = match acc.parse_arguments() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "[openai_compat] dropping tool_call {} with bad args: {}",
                        acc.id, e
                    );
                    return None;
                }
            };
            Some((acc.id.clone(), acc.name.clone(), args))
        })
        .collect();
    if !tool_execution_enabled && !validated_tool_uses.is_empty() {
        eprintln!(
            "[openai_compat] dropping {} tool_call(s) because tool use is disabled for this turn",
            validated_tool_uses.len()
        );
        validated_tool_uses.clear();
    }

    // Defensive final pass for markers that crossed SSE chunk boundaries.
    let final_text = strip_leak_tokens(&accumulated).to_string();

    let final_message = if !validated_tool_uses.is_empty() {
        build_assistant_message_with_tool_uses(&final_text, &validated_tool_uses)
    } else if final_text.is_empty() && last_finish_reason.as_deref() == Some("tool_calls") {
        build_assistant_text_message("[模型尝试调用工具，但参数解析失败]")
    } else {
        build_assistant_text_message(&final_text)
    };

    let mut messages = vec![final_message];

    let pending_tools: Vec<(String, String, String)> = messages[0]
        .blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => {
                Some((id.clone(), name.clone(), input.clone()))
            }
            _ => None,
        })
        .collect();

    if !pending_tools.is_empty() {
        let Some(workspace) = config.workspace_path.clone() else {
            eprintln!(
                "[openai_compat] tool_use blocks present but no workspace_path; skipping tool execution"
            );
            return Ok((
                messages,
                upstream_model,
                finish_reason_from(last_finish_reason.as_deref()),
            ));
        };
        let Some(permission_gate) = config.permission_gate.clone() else {
            eprintln!(
                "[openai_compat] tool_use blocks present but no permission_gate; skipping tool execution"
            );
            return Ok((
                messages,
                upstream_model,
                finish_reason_from(last_finish_reason.as_deref()),
            ));
        };

        eprintln!(
            "[openai_compat] executing {} tool call(s)",
            pending_tools.len()
        );

        for (tool_use_id, tool_name, input_str) in pending_tools {
            let input_value: serde_json::Value = match serde_json::from_str(&input_str) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "[openai_compat] tool {} input not valid JSON: {} (raw: {:?})",
                        tool_name, e, input_str
                    );
                    messages.push(ConversationMessage::tool_result(
                        tool_use_id,
                        tool_name,
                        format!("invalid input JSON: {e}"),
                        true,
                    ));
                    continue;
                }
            };

            let result_msg = crate::tool_execution::execute_tool_with_gate(
                workspace.clone(),
                tool_use_id,
                tool_name.clone(),
                input_value,
                permission_gate.clone(),
                config.bypass_permissions,
                cancel_token.clone(),
                config.tool_timeout_secs,
            )
            .await;

            let is_error = result_msg
                .blocks
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolResult { is_error: true, .. }));
            eprintln!(
                "[openai_compat] tool {} done: is_error={}",
                tool_name, is_error
            );

            messages.push(result_msg);
        }
    }

    Ok((
        messages,
        upstream_model,
        finish_reason_from(last_finish_reason.as_deref()),
    ))
}

fn finish_reason_from(reason: Option<&str>) -> StreamFinishReason {
    reason
        .map(StreamFinishReason::from_str)
        .unwrap_or_else(|| StreamFinishReason::Other("missing".to_string()))
}

/// Convert internal conversation messages to OpenAI ChatCompletions messages.
///
/// This is intentionally richer than `OpenAiChatMessage`: it preserves
/// assistant `tool_use` blocks as `tool_calls` and `role: "tool"` results so
/// the next OpenAI-compatible request can continue after tool execution.
pub fn build_openai_messages(
    messages: &[ConversationMessage],
    system_prompt: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut result = Vec::new();

    if let Some(system) = system_prompt {
        if !system.trim().is_empty() {
            result.push(serde_json::json!({
                "role": "system",
                "content": system,
            }));
        }
    }

    for msg in messages {
        match msg.role {
            MessageRole::User => {
                result.push(serde_json::json!({
                    "role": "user",
                    "content": extract_text_from_blocks(&msg.blocks),
                }));
            }
            MessageRole::Assistant => {
                let mut object = serde_json::Map::new();
                object.insert(
                    "role".to_string(),
                    serde_json::Value::String("assistant".into()),
                );
                object.insert(
                    "content".to_string(),
                    serde_json::Value::String(extract_text_from_blocks(&msg.blocks)),
                );

                let tool_calls = extract_tool_calls_from_blocks(&msg.blocks);
                if !tool_calls.is_empty() {
                    object.insert(
                        "tool_calls".to_string(),
                        serde_json::Value::Array(tool_calls),
                    );
                }

                result.push(serde_json::Value::Object(object));
            }
            MessageRole::Tool => {
                for block in &msg.blocks {
                    if let ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } = block
                    {
                        let content = if *is_error {
                            format!("ERROR: {output}")
                        } else {
                            output.clone()
                        };
                        result.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": content,
                        }));
                    }
                }
            }
            MessageRole::System => {
                eprintln!("[openai_compat] unexpected System role mid-conversation, skipping");
            }
        }
    }

    result
}

fn extract_text_from_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_tool_calls_from_blocks(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => Some(serde_json::json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": input,
                }
            })),
            _ => None,
        })
        .collect()
}

fn build_assistant_text_message(text: &str) -> ConversationMessage {
    ConversationMessage {
        role: MessageRole::Assistant,
        blocks: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        usage: None,
    }
}

fn build_assistant_message_with_tool_uses(
    text: &str,
    tool_uses: &[(String, String, serde_json::Value)],
) -> ConversationMessage {
    let mut blocks = Vec::new();

    if !text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: text.to_string(),
        });
    }

    for (id, name, input) in tool_uses {
        blocks.push(ContentBlock::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.to_string(),
        });
    }

    ConversationMessage {
        role: MessageRole::Assistant,
        blocks,
        usage: None,
    }
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

#[cfg(test)]
mod prompt_intent_tests {
    use super::*;

    #[test]
    fn classifies_pure_writing_as_text_only() {
        assert_eq!(
            classify_prompt_intent("Draft a warm launch email for our beta users"),
            PromptIntent::Writing
        );
    }

    #[test]
    fn classifies_smalltalk_as_text_only() {
        assert_eq!(classify_prompt_intent("Thanks!"), PromptIntent::SmallTalk);
    }

    #[test]
    fn classifies_explicit_search_as_tool_eligible() {
        assert_eq!(
            classify_prompt_intent("Search the web for the latest Rust release"),
            PromptIntent::Search
        );
    }

    #[test]
    fn classifies_code_before_generic_writing() {
        assert_eq!(
            classify_prompt_intent("Write a Rust function that parses a URL"),
            PromptIntent::Code
        );
    }

    #[test]
    fn classifies_file_requests_as_tool_eligible() {
        assert_eq!(
            classify_prompt_intent("Inspect src/lib.rs and explain the config loader"),
            PromptIntent::File
        );
    }

    #[test]
    fn writing_about_today_news_stays_writing() {
        assert_eq!(
            classify_prompt_intent("写一篇关于今天新闻的散文"),
            PromptIntent::Writing
        );
    }

    #[test]
    fn detects_forbidden_write_without_blocking_read_requests() {
        let policy = ToolExposurePolicy::default();

        assert!(prompt_requests_forbidden_tool_action(
            "Write this summary to README.md",
            &policy
        ));
        assert!(!prompt_requests_forbidden_tool_action(
            "Read README.md and summarize it",
            &policy
        ));
        assert!(prompt_requests_forbidden_tool_action(
            "帮我修改 README.md",
            &policy
        ));
    }

    #[test]
    fn detects_forbidden_shell_and_agent_requests() {
        let policy = ToolExposurePolicy::default();

        assert!(prompt_requests_forbidden_tool_action(
            "Run cargo check in the terminal",
            &policy
        ));
        assert!(prompt_requests_forbidden_tool_action(
            "运行 npm install",
            &policy
        ));
        assert!(prompt_requests_forbidden_tool_action(
            "Use an Agent to inspect the repository",
            &policy
        ));
    }
}

#[cfg(test)]
mod request_planning_tests {
    use super::*;

    fn tool_spec(name: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": name,
                "description": "test tool",
                "parameters": { "type": "object" }
            }
        })
    }

    fn user_messages() -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "role": "user",
            "content": "hello"
        })]
    }

    #[test]
    fn auto_tool_choice_includes_only_policy_allowed_tools() {
        let tools = vec![tool_spec("WebSearch"), tool_spec("write_file")];
        let body = build_openai_request_body(
            "test-model",
            &user_messages(),
            Some(&tools),
            &ToolExposurePolicy::default(),
            true,
        );

        assert_eq!(body["tool_choice"], "auto");
        let exposed_tools = body["tools"].as_array().expect("tools should be exposed");
        assert_eq!(exposed_tools.len(), 1);
        assert_eq!(exposed_tools[0]["function"]["name"], "WebSearch");
    }

    #[test]
    fn disabled_tools_set_explicit_none_without_tools_array() {
        let tools = vec![tool_spec("WebSearch")];
        let body = build_openai_request_body(
            "test-model",
            &user_messages(),
            Some(&tools),
            &ToolExposurePolicy::default(),
            false,
        );

        assert_eq!(body["tool_choice"], "none");
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn all_filtered_tools_set_explicit_none() {
        let tools = vec![tool_spec("write_file")];
        let body = build_openai_request_body(
            "test-model",
            &user_messages(),
            Some(&tools),
            &ToolExposurePolicy::default(),
            true,
        );

        assert_eq!(body["tool_choice"], "none");
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn final_synthesis_prompt_is_appended() {
        let prompt = build_turn_system_prompt(
            Some("Base prompt."),
            Some(&ToolExposurePolicy::default()),
            true,
            true,
        )
        .expect("prompt should be present");

        assert!(prompt.contains("Base prompt."));
        assert!(prompt.contains("工具安全策略"));
        assert!(prompt.contains("最终整合轮"));
        assert!(prompt.contains("已禁用工具"));
    }
}

#[cfg(test)]
mod tool_policy_prompt_tests {
    use super::*;

    #[test]
    fn default_policy_prompt_describes_allowed_and_forbidden_actions() {
        let prompt = build_tool_policy_system_prompt(&ToolExposurePolicy::default());

        assert!(prompt.contains("当前允许使用"));
        assert!(prompt.contains("只读文件/搜索工具"));
        assert!(prompt.contains("当前禁止使用"));
        assert!(prompt.contains("文件写入"));
        assert!(prompt.contains("Shell"));
        assert!(prompt.contains("Agent/subagent"));
        assert!(prompt.contains("不要先调用读取/搜索工具"));
    }
}

#[cfg(test)]
mod accumulator_tests {
    use super::*;

    #[test]
    fn full_streaming_sequence_assembles_valid_json() {
        let mut acc = ToolCallAccumulator::default();

        acc.apply_delta(&OpenAiToolCallDelta {
            index: 0,
            id: Some("call_test".to_string()),
            call_type: Some("function".to_string()),
            function: Some(OpenAiFunctionDelta {
                name: Some("get_weather".to_string()),
                arguments: Some(String::new()),
            }),
        });

        for frag in &["{", "\"", "city", "\"", ": ", "\"", "Beijing", "\"", "}"] {
            acc.apply_delta(&OpenAiToolCallDelta {
                index: 0,
                id: None,
                call_type: None,
                function: Some(OpenAiFunctionDelta {
                    name: None,
                    arguments: Some(frag.to_string()),
                }),
            });
        }

        assert_eq!(acc.id, "call_test");
        assert_eq!(acc.name, "get_weather");
        let parsed = acc.parse_arguments().expect("should parse as JSON");
        assert_eq!(parsed["city"], "Beijing");
    }

    #[test]
    fn parallel_tool_calls_accumulate_independently() {
        let mut tool_calls: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();

        let deltas = vec![
            OpenAiToolCallDelta {
                index: 0,
                id: Some("call_a".to_string()),
                call_type: None,
                function: Some(OpenAiFunctionDelta {
                    name: Some("foo".to_string()),
                    arguments: Some(String::new()),
                }),
            },
            OpenAiToolCallDelta {
                index: 1,
                id: Some("call_b".to_string()),
                call_type: None,
                function: Some(OpenAiFunctionDelta {
                    name: Some("bar".to_string()),
                    arguments: Some(String::new()),
                }),
            },
            OpenAiToolCallDelta {
                index: 0,
                id: None,
                call_type: None,
                function: Some(OpenAiFunctionDelta {
                    name: None,
                    arguments: Some("{\"x\":1}".to_string()),
                }),
            },
            OpenAiToolCallDelta {
                index: 1,
                id: None,
                call_type: None,
                function: Some(OpenAiFunctionDelta {
                    name: None,
                    arguments: Some("{\"y\":2}".to_string()),
                }),
            },
        ];

        for d in &deltas {
            let acc = tool_calls
                .entry(d.index)
                .or_insert_with(|| ToolCallAccumulator {
                    index: d.index,
                    ..Default::default()
                });
            acc.apply_delta(d);
        }

        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[&0].name, "foo");
        assert_eq!(tool_calls[&1].name, "bar");
        assert_eq!(tool_calls[&0].parse_arguments().unwrap()["x"], 1);
        assert_eq!(tool_calls[&1].parse_arguments().unwrap()["y"], 2);
    }

    #[test]
    fn malformed_arguments_are_dropped_not_propagated() {
        let mut acc = ToolCallAccumulator::default();
        acc.apply_delta(&OpenAiToolCallDelta {
            index: 0,
            id: Some("call_bad".to_string()),
            call_type: None,
            function: Some(OpenAiFunctionDelta {
                name: Some("foo".to_string()),
                arguments: Some("{not json".to_string()),
            }),
        });

        assert!(acc.validate().is_ok());
        assert!(acc.parse_arguments().is_err());
    }
}

#[cfg(test)]
mod leak_token_tests {
    use super::*;

    #[test]
    fn no_leak_returns_borrowed() {
        let input = "Hello world";
        let result = strip_leak_tokens(input);
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn dsml_tool_calls_stripped() {
        let input = "Result: <｜DSML｜tool_calls｜> more text";
        let result = strip_leak_tokens(input);
        assert_eq!(result, "Result:  more text");
    }

    #[test]
    fn multiple_dsml_tokens_stripped() {
        let input = "<｜DSML｜begin_tool_calls｜>data<｜DSML｜end_tool_calls｜>";
        let result = strip_leak_tokens(input);
        assert_eq!(result, "data");
    }

    #[test]
    fn unknown_dsml_pattern_stripped() {
        let input = "Before <｜DSML｜some_unknown_marker｜> after";
        let result = strip_leak_tokens(input);
        assert_eq!(result, "Before  after");
    }

    #[test]
    fn dsml_tags_with_attributes_are_stripped() {
        let input = "<｜DSML｜invoke name=\"WebFetch\"><｜DSML｜parameter name=\"url\">https://example.com</｜DSML｜parameter></｜DSML｜invoke>";
        let result = strip_leak_tokens(input);
        assert_eq!(result, "https://example.com");
    }

    #[test]
    fn ascii_pipe_variant_stripped() {
        let input = "<|tool_call|>data<|end_tool_call|>";
        let result = strip_leak_tokens(input);
        assert_eq!(result, "data");
    }

    #[test]
    fn preserves_non_dsml_angle_brackets() {
        let input = "Some <strong>bold</strong> text";
        let result = strip_leak_tokens(input);
        assert_eq!(result, "Some <strong>bold</strong> text");
    }
}

#[cfg(test)]
mod openai_messages_serialization {
    use super::*;

    fn make_user(text: &str) -> ConversationMessage {
        ConversationMessage::user_text(text.to_string())
    }

    fn make_assistant_text(text: &str) -> ConversationMessage {
        ConversationMessage::assistant(vec![ContentBlock::Text {
            text: text.to_string(),
        }])
    }

    fn make_assistant_with_tool_use(
        text: &str,
        tool_id: &str,
        tool_name: &str,
        args: &str,
    ) -> ConversationMessage {
        let mut blocks = Vec::new();
        if !text.is_empty() {
            blocks.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }
        blocks.push(ContentBlock::ToolUse {
            id: tool_id.to_string(),
            name: tool_name.to_string(),
            input: args.to_string(),
        });
        ConversationMessage::assistant(blocks)
    }

    fn make_tool_result(
        tool_id: &str,
        tool_name: &str,
        output: &str,
        is_error: bool,
    ) -> ConversationMessage {
        ConversationMessage::tool_result(
            tool_id.to_string(),
            tool_name.to_string(),
            output.to_string(),
            is_error,
        )
    }

    #[test]
    fn pure_text_conversation_serializes_correctly() {
        let msgs = vec![make_user("hello"), make_assistant_text("hi there")];
        let result = build_openai_messages(&msgs, None);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[0]["content"], "hello");
        assert_eq!(result[1]["role"], "assistant");
        assert_eq!(result[1]["content"], "hi there");
        assert!(result[1].get("tool_calls").is_none());
    }

    #[test]
    fn system_prompt_prepended() {
        let msgs = vec![make_user("hello")];
        let result = build_openai_messages(&msgs, Some("You are helpful."));

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["role"], "system");
        assert_eq!(result[0]["content"], "You are helpful.");
        assert_eq!(result[1]["role"], "user");
    }

    #[test]
    fn tool_use_includes_tool_calls_array() {
        let msgs = vec![
            make_user("search news"),
            make_assistant_with_tool_use(
                "I'll search.",
                "call_abc",
                "WebSearch",
                "{\"query\":\"news\"}",
            ),
        ];
        let result = build_openai_messages(&msgs, None);

        assert_eq!(result.len(), 2);
        let assistant = &result[1];
        assert_eq!(assistant["role"], "assistant");
        assert_eq!(assistant["content"], "I'll search.");

        let tool_calls = assistant["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_abc");
        assert_eq!(tool_calls[0]["type"], "function");
        assert_eq!(tool_calls[0]["function"]["name"], "WebSearch");
        assert_eq!(
            tool_calls[0]["function"]["arguments"],
            "{\"query\":\"news\"}"
        );
    }

    #[test]
    fn tool_result_becomes_tool_role_message() {
        let msgs = vec![
            make_user("search"),
            make_assistant_with_tool_use("Searching", "call_xyz", "WebSearch", "{}"),
            make_tool_result("call_xyz", "WebSearch", "results here", false),
        ];
        let result = build_openai_messages(&msgs, None);

        assert_eq!(result.len(), 3);
        assert_eq!(result[2]["role"], "tool");
        assert_eq!(result[2]["tool_call_id"], "call_xyz");
        assert_eq!(result[2]["content"], "results here");
    }

    #[test]
    fn tool_error_prefixed_in_content() {
        let msgs = vec![make_tool_result("call_x", "Bash", "command failed", true)];
        let result = build_openai_messages(&msgs, None);

        assert_eq!(result[0]["role"], "tool");
        assert!(result[0]["content"].as_str().unwrap().starts_with("ERROR:"));
    }

    #[test]
    fn empty_assistant_with_only_tool_calls_has_empty_string_content() {
        let msgs = vec![make_assistant_with_tool_use("", "call_x", "Bash", "{}")];
        let result = build_openai_messages(&msgs, None);

        assert_eq!(result[0]["role"], "assistant");
        assert!(result[0]["content"].is_string());
        assert_eq!(result[0]["content"], "");
        assert!(result[0].get("tool_calls").is_some());
    }

    #[test]
    fn multi_turn_conversation_serializes_in_order() {
        let msgs = vec![
            make_user("search news"),
            make_assistant_with_tool_use("I'll search.", "call_1", "WebSearch", "{\"q\":\"news\"}"),
            make_tool_result("call_1", "WebSearch", "results...", false),
            make_assistant_text("Based on the search, today's news is..."),
        ];
        let result = build_openai_messages(&msgs, None);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
        assert!(result[1].get("tool_calls").is_some());
        assert_eq!(result[2]["role"], "tool");
        assert_eq!(result[2]["tool_call_id"], "call_1");
        assert_eq!(result[3]["role"], "assistant");
        assert!(result[3].get("tool_calls").is_none());
    }

    #[test]
    fn parallel_tool_calls_in_single_assistant_message() {
        let assistant_msg = ConversationMessage::assistant(vec![
            ContentBlock::ToolUse {
                id: "call_a".to_string(),
                name: "WebSearch".to_string(),
                input: "{\"q\":\"news\"}".to_string(),
            },
            ContentBlock::ToolUse {
                id: "call_b".to_string(),
                name: "WebFetch".to_string(),
                input: "{\"url\":\"https://example.com\"}".to_string(),
            },
        ]);
        let result = build_openai_messages(&[assistant_msg], None);

        let tool_calls = result[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0]["id"], "call_a");
        assert_eq!(tool_calls[1]["id"], "call_b");
    }
}

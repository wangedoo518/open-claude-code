//! Shared built-in tool execution helpers.
//!
//! OpenAI-compatible streaming and the Anthropic-style agentic loop both need
//! the same safety wrapper around built-in tool execution:
//! workspace CWD pinning, permission checks, timeout, cancellation, and
//! standard `tool_result` message construction.

use runtime::ConversationMessage;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::agentic_loop::{PermissionDecision, PermissionGate};

/// Maximum size of a single tool output before truncation (100 KB).
pub const MAX_TOOL_OUTPUT_CHARS: usize = 100_000;

/// Default tool execution timeout for OpenAI-compatible turns.
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 60;

/// Tools that may execute without explicit user approval.
///
/// This includes pure-read filesystem tools and outbound-only network tools.
/// Mutating tools such as Bash/Edit/Write are deliberately excluded.
pub fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "glob_search"
            | "grep_search"
            | "Read"
            | "Glob"
            | "Grep"
            | "WebSearch"
            | "WebFetch"
    )
}

/// Categorize tools by risk and applicability for OpenAI-compatible providers.
///
/// This is intentionally narrower than the agentic loop tool surface. OpenAI
/// compat models receive JSON schemas directly and may confidently call any
/// exposed tool, so the default exposure must be conservative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    /// Safe to expose by default: read-only file/search/web operations.
    SafeReadOnly,
    /// Requires explicit user opt-in: mutates filesystem state.
    FilesystemWrite,
    /// Requires explicit user opt-in: executes shell or REPL commands.
    ShellExecution,
    /// Only meaningful in the Anthropic-style agentic loop context.
    AgenticOnly,
    /// Specialty tools requiring a specific product surface.
    Specialty,
    /// Conservative fallback for newly added or unknown tools.
    Unknown,
}

/// Classify a tool by name for OpenAI-compatible tool exposure.
///
/// When adding entries to `tools::mvp_tool_specs()`, update this function so
/// OpenAI-compatible providers do not accidentally receive unsafe tools.
pub fn classify_tool(name: &str) -> ToolCategory {
    use ToolCategory::*;
    match name {
        // Safe read-only tools exposed by default.
        "WebSearch" | "WebFetch" => SafeReadOnly,
        "read_file" | "Read" => SafeReadOnly,
        "glob_search" | "Glob" => SafeReadOnly,
        "grep_search" | "Grep" => SafeReadOnly,

        // Filesystem writes require explicit opt-in.
        "write_file" | "Write" => FilesystemWrite,
        "edit_file" | "Edit" => FilesystemWrite,
        "NotebookEdit" => FilesystemWrite,

        // Shell execution requires explicit opt-in.
        "bash" | "Bash" => ShellExecution,
        "PowerShell" => ShellExecution,
        "REPL" => ShellExecution,

        // Agentic-only tools should not be exposed on OpenAI compat.
        "Agent" | "TeamCreate" | "WorkerCreate" | "SendPrompt" => AgenticOnly,
        "CronCreate" | "CronList" | "CronDelete" => AgenticOnly,
        "MCP" | "ListMcpResources" | "ReadMcpResource" => AgenticOnly,
        "Skill" | "ToolSearch" => AgenticOnly,

        // Specialty tools are opt-in.
        "TodoWrite" => Specialty,
        "RemoteTrigger" => Specialty,

        _ => Unknown,
    }
}

/// Tool exposure policy for one OpenAI-compatible turn.
#[derive(Debug, Clone)]
pub struct ToolExposurePolicy {
    /// Include safe read-only tools. Default: true.
    pub include_safe: bool,
    /// Include filesystem mutation tools. Default: false.
    pub include_filesystem_write: bool,
    /// Include shell execution tools. Default: false.
    pub include_shell: bool,
    /// Include specialty tools. Default: false.
    pub include_specialty: bool,
}

impl Default for ToolExposurePolicy {
    fn default() -> Self {
        Self {
            include_safe: true,
            include_filesystem_write: false,
            include_shell: false,
            include_specialty: false,
        }
    }
}

impl ToolExposurePolicy {
    /// Return true if a tool should be exposed under this policy.
    pub fn allows(&self, tool_name: &str) -> bool {
        match classify_tool(tool_name) {
            ToolCategory::SafeReadOnly => self.include_safe,
            ToolCategory::FilesystemWrite => self.include_filesystem_write,
            ToolCategory::ShellExecution => self.include_shell,
            ToolCategory::Specialty => self.include_specialty,
            ToolCategory::AgenticOnly | ToolCategory::Unknown => false,
        }
    }
}

/// Execute a built-in tool with permission, timeout, cancellation, and
/// standardized `tool_result` message construction.
pub async fn execute_tool_with_gate(
    cwd: PathBuf,
    tool_use_id: String,
    tool_name: String,
    tool_input: Value,
    permission_gate: Arc<PermissionGate>,
    bypass_permissions: bool,
    cancel_token: CancellationToken,
    timeout_secs: u64,
) -> ConversationMessage {
    let permission = permission_gate
        .check_permission(
            &tool_name,
            &tool_input,
            bypass_permissions,
            &cancel_token,
        )
        .await;

    match permission {
        PermissionDecision::Allow | PermissionDecision::AllowAlways => {}
        PermissionDecision::Deny { reason } => {
            return ConversationMessage::tool_result(
                tool_use_id,
                tool_name,
                format!("Permission denied: {reason}"),
                true,
            );
        }
    }

    if cancel_token.is_cancelled() {
        return ConversationMessage::tool_result(
            tool_use_id,
            tool_name,
            "cancelled by user before execution".to_string(),
            true,
        );
    }

    let result = execute_builtin_tool_with_timeout(
        cwd,
        tool_name.clone(),
        tool_input,
        cancel_token,
        timeout_secs,
    )
    .await;

    match result {
        Ok(output) => ConversationMessage::tool_result(
            tool_use_id,
            tool_name,
            truncate_tool_output(output),
            false,
        ),
        Err(error) => ConversationMessage::tool_result(
            tool_use_id,
            tool_name,
            truncate_tool_output(error),
            true,
        ),
    }
}

/// Execute a built-in tool under the workspace CWD lock.
///
/// This lower-level helper does not perform permission checks or build
/// messages. It exists so the agentic loop can preserve its existing MCP and
/// hook behavior while sharing the same timeout/CWD implementation.
pub async fn execute_builtin_tool_with_timeout(
    cwd: PathBuf,
    tool_name: String,
    tool_input: Value,
    cancel_token: CancellationToken,
    timeout_secs: u64,
) -> Result<String, String> {
    if cancel_token.is_cancelled() {
        return Err("cancelled by user before execution".to_string());
    }

    let effective_timeout = if timeout_secs > 0 {
        timeout_secs
    } else {
        DEFAULT_TOOL_TIMEOUT_SECS
    };
    let join_handle = tokio::task::spawn_blocking(move || {
        execute_tool_in_workspace(&cwd, &tool_name, &tool_input)
    });
    let timeout_fut = tokio::time::timeout(Duration::from_secs(effective_timeout), join_handle);

    tokio::select! {
        biased;
        _ = cancel_token.cancelled() => {
            Err("cancelled by user during execution".to_string())
        }
        timeout_outcome = timeout_fut => {
            match timeout_outcome {
                Ok(Ok(Ok(output))) => Ok(output),
                Ok(Ok(Err(tool_err))) => Err(tool_err),
                Ok(Err(join_err)) => Err(format!("tool task panicked: {join_err}")),
                Err(_) => Err(format!("tool execution timed out after {effective_timeout}s")),
            }
        }
    }
}

/// Truncate tool output using UTF-8 char boundaries.
pub fn truncate_tool_output(output: String) -> String {
    if output.len() <= MAX_TOOL_OUTPUT_CHARS {
        return output;
    }

    let mut boundary = MAX_TOOL_OUTPUT_CHARS.min(output.len());
    while boundary > 0 && !output.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let truncated = &output[..boundary];
    format!("{truncated}\n\n... [output truncated at {MAX_TOOL_OUTPUT_CHARS} bytes]")
}

fn execute_tool_in_workspace(
    cwd: &std::path::Path,
    tool_name: &str,
    input: &Value,
) -> Result<String, String> {
    let lock = crate::process_workspace_lock();
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let original = std::env::current_dir().map_err(|e| e.to_string())?;

    if cwd.is_dir() {
        std::env::set_current_dir(cwd)
            .map_err(|e| format!("failed to cd into {}: {e}", cwd.display()))?;
    }

    let result = tools::execute_tool(tool_name, input);
    let _ = std::env::set_current_dir(&original);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_includes_web_tools() {
        assert!(is_read_only_tool("WebSearch"));
        assert!(is_read_only_tool("WebFetch"));
    }

    #[test]
    fn read_only_excludes_mutating_tools() {
        assert!(!is_read_only_tool("Bash"));
        assert!(!is_read_only_tool("write_file"));
        assert!(!is_read_only_tool("Edit"));
    }

    #[test]
    fn read_only_includes_legacy_aliases() {
        assert!(is_read_only_tool("read_file"));
        assert!(is_read_only_tool("Read"));
        assert!(is_read_only_tool("Glob"));
        assert!(is_read_only_tool("Grep"));
    }

    #[test]
    fn truncate_long_ascii_output() {
        let input = "a".repeat(MAX_TOOL_OUTPUT_CHARS + 20);
        let result = truncate_tool_output(input);
        assert!(result.contains("[output truncated"));
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn default_policy_only_includes_safe() {
        let policy = ToolExposurePolicy::default();
        assert!(policy.allows("WebSearch"));
        assert!(policy.allows("read_file"));
        assert!(!policy.allows("bash"));
        assert!(!policy.allows("write_file"));
        assert!(!policy.allows("Agent"));
        assert!(!policy.allows("UnknownTool"));
    }

    #[test]
    fn agentic_tools_never_exposed_even_with_all_flags() {
        let policy = ToolExposurePolicy {
            include_safe: true,
            include_filesystem_write: true,
            include_shell: true,
            include_specialty: true,
        };
        assert!(!policy.allows("Agent"));
        assert!(!policy.allows("TeamCreate"));
        assert!(!policy.allows("MCP"));
    }

    #[test]
    fn classification_examples() {
        assert_eq!(classify_tool("WebSearch"), ToolCategory::SafeReadOnly);
        assert_eq!(classify_tool("write_file"), ToolCategory::FilesystemWrite);
        assert_eq!(classify_tool("bash"), ToolCategory::ShellExecution);
        assert_eq!(classify_tool("Agent"), ToolCategory::AgenticOnly);
        assert_eq!(classify_tool("xyzzy"), ToolCategory::Unknown);
    }
}

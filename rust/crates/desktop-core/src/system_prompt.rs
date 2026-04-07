//! System prompt construction and CLAUDE.md discovery.

use std::path::{Path, PathBuf};

use tools::ToolSpec;

/// Build a complete system prompt including agent role, tool definitions, and project context.
pub fn build_system_prompt(
    project_path: &Path,
    tool_specs: &[ToolSpec],
    claude_md: Option<&str>,
) -> String {
    let mut prompt = String::with_capacity(8192);

    // ── Agent preamble ───────────────────────────────────────────
    prompt.push_str(AGENT_PREAMBLE);
    prompt.push('\n');

    // ── Tool descriptions ────────────────────────────────────────
    // Filter out MCP tools (ListMcpResources, ReadMcpResource, McpAuth, MCP)
    // because the vendored crate's global MCP registry is crate-private and
    // cannot be populated from the agentic loop. Including them in the prompt
    // would cause the LLM to call tools that always return "server not found".
    // See docs/audit-lessons.md L-09.
    let filtered_specs: Vec<&tools::ToolSpec> = tool_specs
        .iter()
        .filter(|spec| {
            !matches!(
                spec.name,
                "ListMcpResources" | "ReadMcpResource" | "McpAuth" | "MCP"
            )
        })
        .collect();

    if !filtered_specs.is_empty() {
        prompt.push_str("\n# Available tools\n\n");
        prompt.push_str("You have access to the following tools. Call them by generating a tool_use content block.\n\n");
        for spec in &filtered_specs {
            prompt.push_str(&format!("## {}\n", spec.name));
            prompt.push_str(spec.description);
            prompt.push('\n');
            prompt.push_str(&format!(
                "Input schema: {}\n\n",
                serde_json::to_string(&spec.input_schema).unwrap_or_default()
            ));
        }
    }

    // ── Project context ──────────────────────────────────────────
    prompt.push_str("# Environment\n\n");
    prompt.push_str(&format!(
        "- Working directory: {}\n",
        project_path.display()
    ));
    if let Some(name) = project_path.file_name().and_then(|n| n.to_str()) {
        prompt.push_str(&format!("- Project name: {name}\n"));
    }
    prompt.push_str(&format!("- Platform: {}\n", std::env::consts::OS));
    prompt.push('\n');

    // ── CLAUDE.md content ────────────────────────────────────────
    if let Some(content) = claude_md {
        if !content.trim().is_empty() {
            prompt.push_str("# User instructions (CLAUDE.md)\n\n");
            prompt.push_str(content);
            prompt.push('\n');
        }
    }

    prompt
}

/// Walk upward from `start` looking for CLAUDE.md or .claude/CLAUDE.md.
///
/// Returns the content of the first one found, or `None`.
pub fn find_claude_md(start: &Path) -> Option<String> {
    let mut current = start.to_path_buf();
    loop {
        // Check CLAUDE.md at current level.
        let candidate = current.join("CLAUDE.md");
        if candidate.is_file() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                return Some(content);
            }
        }
        // Check .claude/CLAUDE.md at current level.
        let nested = current.join(".claude").join("CLAUDE.md");
        if nested.is_file() {
            if let Ok(content) = std::fs::read_to_string(&nested) {
                return Some(content);
            }
        }
        // Move to parent.
        if !current.pop() {
            break;
        }
    }
    None
}

const AGENT_PREAMBLE: &str = r#"You are an AI coding assistant running inside a desktop application. You help users with software engineering tasks including reading, writing, and editing code, running commands, searching files, and more.

# Core behavior

- You are highly capable and can accomplish complex multi-step tasks autonomously.
- When given a task, execute it directly using the available tools. Do not ask for confirmation unless genuinely ambiguous.
- Prefer editing existing files over creating new ones.
- After making changes, verify your work by reading the result or running tests.
- Be concise in your responses. Lead with the action, not the reasoning.

# Tool usage

- Use `bash` to run shell commands (git, npm, cargo, etc.).
- Use `read_file` to examine file contents before editing.
- Use `write_file` to create new files. Use `edit_file` to modify existing files.
- Use `glob_search` and `grep_search` to find files and code.
- Execute tools sequentially. Wait for each result before proceeding.

# Safety

- Do not run destructive commands without explicit user instruction.
- Do not push to remote repositories unless asked.
- Validate your changes compile/build before declaring completion."#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_system_prompt_includes_tools_and_path() {
        let specs = tools::mvp_tool_specs();
        let prompt = build_system_prompt(Path::new("/tmp/my-project"), &specs, None);
        assert!(prompt.contains("bash"));
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("edit_file"));
        assert!(prompt.contains("glob_search"));
        assert!(prompt.contains("grep_search"));
        assert!(prompt.contains("/tmp/my-project"));
        assert!(prompt.contains("my-project"));
    }

    #[test]
    fn build_system_prompt_includes_claude_md() {
        let prompt = build_system_prompt(
            Path::new("/tmp/test"),
            &[],
            Some("Always use TypeScript."),
        );
        assert!(prompt.contains("Always use TypeScript."));
        assert!(prompt.contains("CLAUDE.md"));
    }

    #[test]
    fn find_claude_md_returns_none_for_empty_dir() {
        let result = find_claude_md(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(result.is_none());
    }
}

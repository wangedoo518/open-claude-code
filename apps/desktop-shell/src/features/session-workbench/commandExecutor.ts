/**
 * Slash command executor — handles /commands from InputBar.
 *
 * Command types (from docs/desktop-shell/tokens/functional-tokens.md §5.1):
 *   prompt   — expand to prompt sent to the model (e.g., /commit, /review)
 *   local    — synchronous local execution, return text (e.g., /clear, /cost)
 *   local-jsx — async local, render interactive UI (e.g., /config, /theme, /model)
 */

import type { ConversationMessage } from "./types";

/* ─── Command definitions ───────────────────────────────────────── */

export type CommandType = "prompt" | "local" | "local-jsx";

export interface CommandResult {
  type: "system_message" | "clear" | "navigate" | "noop";
  message?: string;
  navigateTo?: string;
}

export interface CommandDefinition {
  name: string;
  type: CommandType;
  description: string;
  /**
   * Execute the command. May return a result synchronously or asynchronously.
   * Async commands are awaited by the caller before the result is applied.
   */
  execute: (
    args: string,
    context: CommandContext,
  ) => CommandResult | Promise<CommandResult>;
}

export interface CommandContext {
  messages: ConversationMessage[];
  permissionMode: string;
  modelLabel: string;
  sessionId?: string;
  onSendAsPrompt: (prompt: string) => void;
  onInjectSystemMessage: (message: string) => void;
  onClearMessages: () => void;
  onNavigate?: (section: string) => void;
}

/* ─── Built-in commands ─────────────────────────────────────────── */

const COMMANDS: CommandDefinition[] = [
  {
    name: "clear",
    type: "local",
    description: "Clear conversation history",
    execute: (_args, ctx) => {
      ctx.onClearMessages();
      return {
        type: "system_message",
        message: "Conversation cleared.",
      };
    },
  },
  {
    name: "compact",
    type: "local",
    description: "Compact conversation to save context",
    execute: async (_args, ctx) => {
      // Must have a session to compact.
      if (!ctx.sessionId) {
        return {
          type: "system_message",
          message: "No active session to compact.",
        };
      }
      // Wait for backend ACK before clearing UI. If backend refuses
      // (e.g. SessionBusy), leave UI untouched and show the error.
      try {
        const { compactSession } = await import("./api/client");
        await compactSession(ctx.sessionId);
        ctx.onClearMessages();
        return {
          type: "system_message",
          message: "✓ Session compacted. Older messages were summarized to free context window.",
        };
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return {
          type: "system_message",
          message: `Failed to compact session: ${message}. Local display is unchanged.`,
        };
      }
    },
  },
  {
    name: "cost",
    type: "local",
    description: "Show token usage and costs",
    execute: (_args, ctx) => {
      const msgCount = ctx.messages.length;
      const toolUseCount = ctx.messages.filter(
        (m) => m.type === "tool_use"
      ).length;
      return {
        type: "system_message",
        message: [
          "**Session Cost Summary**",
          `- Messages: ${msgCount}`,
          `- Tool calls: ${toolUseCount}`,
          `- Model: ${ctx.modelLabel}`,
          `- Permission mode: ${ctx.permissionMode}`,
          "",
          "_Counts are from local message history. Actual token costs require backend API metering._",
        ].join("\n"),
      };
    },
  },
  {
    name: "diff",
    type: "local",
    description: "Show file changes in this session",
    execute: (_args, ctx) => {
      const editTools = new Set(["edit", "editfile", "write", "writefile"]);
      const filePaths = new Set<string>();

      for (const msg of ctx.messages) {
        if (msg.type === "tool_use" && msg.toolUse) {
          const toolLower = msg.toolUse.toolName.toLowerCase();
          if (editTools.has(toolLower)) {
            try {
              const parsed = JSON.parse(msg.toolUse.toolInput) as Record<string, unknown>;
              const path = parsed.file_path ?? parsed.path;
              if (typeof path === "string") filePaths.add(path);
            } catch { /* ignore parse errors */ }
          }
        }
      }

      if (filePaths.size === 0) {
        return {
          type: "system_message",
          message: "No file modifications detected in this session's tool history.",
        };
      }

      const fileList = [...filePaths].map((p) => `- \`${p}\``).join("\n");
      return {
        type: "system_message",
        message: [
          `**Files Modified (${filePaths.size})**`,
          "",
          fileList,
          "",
          "_Based on Edit/Write tool calls in this session. Actual diffs require backend integration._",
        ].join("\n"),
      };
    },
  },
  {
    name: "session",
    type: "local",
    description: "Show session information",
    execute: (_args, ctx) => {
      return {
        type: "system_message",
        message: [
          "**Session Info**",
          `- Session ID: ${ctx.sessionId ?? "N/A"}`,
          `- Messages: ${ctx.messages.length}`,
          `- Model: ${ctx.modelLabel}`,
          `- Mode: ${ctx.permissionMode}`,
        ].join("\n"),
      };
    },
  },
  {
    name: "model",
    type: "local-jsx",
    description: "Switch AI model",
    execute: () => {
      return {
        type: "navigate",
        navigateTo: "settings",
        message: "Opening model settings...",
      };
    },
  },
  {
    name: "theme",
    type: "local-jsx",
    description: "Switch theme",
    execute: () => {
      return {
        type: "navigate",
        navigateTo: "settings",
        message: "Opening theme settings...",
      };
    },
  },
  {
    name: "config",
    type: "local-jsx",
    description: "Open configuration",
    execute: () => {
      return {
        type: "navigate",
        navigateTo: "settings",
        message: "Opening settings...",
      };
    },
  },
  {
    name: "help",
    type: "local",
    description: "Show available commands",
    execute: () => {
      const lines = COMMANDS.filter((cmd) => cmd.name !== "help").map(
        (cmd) => `- \`/${cmd.name}\` — ${cmd.description}`
      );
      return {
        type: "system_message",
        message: [
          "**Available Commands**",
          "",
          ...lines,
          "",
          "_Type `/` followed by a command name._",
        ].join("\n"),
      };
    },
  },
  {
    name: "permissions",
    type: "local",
    description: "View current permission mode",
    execute: (_args, ctx) => {
      return {
        type: "system_message",
        message: `Current permission mode: **${ctx.permissionMode}**`,
      };
    },
  },
  {
    name: "status",
    type: "local",
    description: "Show session status",
    execute: (_args, ctx) => {
      return {
        type: "system_message",
        message: [
          "**Status**",
          `- Model: ${ctx.modelLabel}`,
          `- Permission: ${ctx.permissionMode}`,
          `- Messages: ${ctx.messages.length}`,
          `- Session: ${ctx.sessionId ?? "none"}`,
        ].join("\n"),
      };
    },
  },
  // Prompt-type commands: these expand into a prompt sent to the model
  {
    name: "commit",
    type: "prompt",
    description: "Commit code changes",
    execute: (_args, ctx) => {
      ctx.onSendAsPrompt(
        "Review all staged and unstaged changes, then create a git commit with a concise message."
      );
      return { type: "noop" };
    },
  },
  {
    name: "review",
    type: "prompt",
    description: "Review code changes",
    execute: (_args, ctx) => {
      ctx.onSendAsPrompt(
        "Review the recent code changes for bugs, style issues, and potential improvements."
      );
      return { type: "noop" };
    },
  },
  {
    name: "init",
    type: "prompt",
    description: "Initialize CLAUDE.md",
    execute: (_args, ctx) => {
      ctx.onSendAsPrompt(
        "Analyze this project and create a CLAUDE.md file with project conventions, architecture overview, and common patterns."
      );
      return { type: "noop" };
    },
  },
];

/* ─── Executor ──────────────────────────────────────────────────── */

/**
 * Parse and execute a slash command.
 * Returns null if the input is not a command.
 * May return a Promise for async commands (e.g. /compact which waits for
 * backend ACK before returning).
 */
export function executeCommand(
  input: string,
  context: CommandContext,
): CommandResult | Promise<CommandResult> | null {
  if (!input.startsWith("/")) return null;

  const parts = input.slice(1).split(/\s+/);
  const name = parts[0]?.toLowerCase();
  const args = parts.slice(1).join(" ");

  if (!name) return null;

  const command = COMMANDS.find((cmd) => cmd.name === name);

  if (!command) {
    return {
      type: "system_message",
      message: `Unknown command: \`/${name}\`. Type \`/help\` to see available commands.`,
    };
  }

  return command.execute(args, context);
}

/**
 * Check if a string is a slash command.
 */
export function isSlashCommand(input: string): boolean {
  return input.startsWith("/") && input.length > 1;
}

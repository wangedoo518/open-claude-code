import { useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Terminal as TerminalIcon,
  FileEdit,
  Search,
  Globe,
  AlertCircle,
  CheckCircle2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { ConversationMessage } from "@/store/slices/sessions";

interface MessageItemProps {
  message: ConversationMessage;
}

export function MessageItem({ message }: MessageItemProps) {
  switch (message.type) {
    case "text":
      return message.role === "user" ? (
        <UserMessage content={message.content} />
      ) : (
        <AssistantMessage content={message.content} />
      );
    case "tool_use":
      return <ToolUseMessage message={message} />;
    case "tool_result":
      return <ToolResultMessage message={message} />;
    case "error":
      return <ErrorMessage content={message.content} />;
    default:
      return <AssistantMessage content={message.content} />;
  }
}

/**
 * User message — card style with left blue accent bar.
 *
 * Uses design tokens:
 *   --color-msg-user-bg (rgb(240,240,240) light / rgb(55,55,55) dark)
 *   --color-label-you   (rgb(37,99,235) light / rgb(122,180,232) dark)
 */
function UserMessage({ content }: { content: string }) {
  return (
    <div className="mx-4 my-2">
      <div
        className="relative overflow-hidden rounded-lg border border-border/50"
        style={{ backgroundColor: "var(--color-msg-user-bg, var(--color-accent))" }}
      >
        {/* Left accent bar */}
        <div
          className="absolute left-0 top-0 h-full w-[3px]"
          style={{ backgroundColor: "var(--color-label-you, rgb(37,99,235))" }}
        />
        <div className="py-3 pl-5 pr-4">
          <div
            className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider"
            style={{ color: "var(--color-label-you, rgb(37,99,235))" }}
          >
            You
          </div>
          <div className="whitespace-pre-wrap text-sm leading-relaxed text-foreground">
            {content}
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * Assistant message — card style with left orange accent bar.
 *
 * Uses design tokens:
 *   --color-msg-assistant-bg (rgb(250,250,250) light / rgb(35,35,35) dark)
 *   --color-label-claude     (rgb(215,119,87) — same in both modes)
 */
function AssistantMessage({ content }: { content: string }) {
  return (
    <div className="mx-4 my-2">
      <div
        className="relative overflow-hidden rounded-lg border border-border/50"
        style={{ backgroundColor: "var(--color-msg-assistant-bg, var(--color-background))" }}
      >
        {/* Left accent bar */}
        <div
          className="absolute left-0 top-0 h-full w-[3px]"
          style={{ backgroundColor: "var(--color-label-claude, rgb(215,119,87))" }}
        />
        <div className="py-3 pl-5 pr-4">
          <div
            className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider"
            style={{ color: "var(--color-label-claude, rgb(215,119,87))" }}
          >
            Assistant
          </div>
          <div className="prose prose-sm dark:prose-invert max-w-none text-sm leading-relaxed">
            {content}
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * Tool use message — collapsible card with tool name and input preview.
 */
function ToolUseMessage({ message }: { message: ConversationMessage }) {
  const [expanded, setExpanded] = useState(false);
  const toolName = message.toolUse?.toolName ?? "Tool";
  const toolInput = message.toolUse?.toolInput ?? "";

  const icon = getToolIcon(toolName);

  return (
    <div className="mx-4 my-1">
      <button
        className="flex w-full items-center gap-2 rounded-lg border border-border/50 bg-muted/30 px-3 py-2 text-xs transition-colors hover:bg-muted/50"
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? (
          <ChevronDown className="size-3 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3 text-muted-foreground" />
        )}
        <span className="text-terminal-tool">{icon}</span>
        <span className="font-medium text-terminal-tool">{toolName}</span>
        {!expanded && toolInput && (
          <span className="flex-1 truncate text-left text-muted-foreground">
            {toolInput.slice(0, 80)}
          </span>
        )}
      </button>
      {expanded && toolInput && (
        <div
          className="mt-1 rounded-b-lg border border-t-0 border-border/50 p-3"
          style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-muted))" }}
        >
          <pre className="overflow-x-auto whitespace-pre-wrap font-mono text-xs text-foreground/80">
            {toolInput}
          </pre>
        </div>
      )}
    </div>
  );
}

/**
 * Tool result message — collapsible card with output preview.
 */
function ToolResultMessage({ message }: { message: ConversationMessage }) {
  const [expanded, setExpanded] = useState(false);
  const toolName = message.toolResult?.toolName ?? "Result";
  const output = message.toolResult?.output ?? message.content;
  const isError = message.toolResult?.isError ?? false;

  return (
    <div className="mx-4 my-1">
      <button
        className={cn(
          "flex w-full items-center gap-2 rounded-lg border px-3 py-2 text-xs transition-colors",
          isError
            ? "border-destructive/30 bg-destructive/5 hover:bg-destructive/10"
            : "border-border/50 bg-muted/20 hover:bg-muted/40"
        )}
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? (
          <ChevronDown className="size-3" />
        ) : (
          <ChevronRight className="size-3" />
        )}
        {isError ? (
          <AlertCircle className="size-3.5 text-destructive" />
        ) : (
          <CheckCircle2 className="size-3.5 text-terminal-tool" />
        )}
        <span className="font-medium">{toolName} result</span>
        {!expanded && (
          <span className="flex-1 truncate text-left text-muted-foreground">
            {output.split("\n")[0]?.slice(0, 60)}
          </span>
        )}
      </button>
      {expanded && (
        <div
          className="mt-1 max-h-[300px] overflow-auto rounded-b-lg border border-t-0 border-border/50 p-3"
          style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-muted))" }}
        >
          <pre className="whitespace-pre-wrap font-mono text-xs text-foreground/80">
            {output}
          </pre>
        </div>
      )}
    </div>
  );
}

/**
 * Error message — destructive card.
 */
function ErrorMessage({ content }: { content: string }) {
  return (
    <div className="mx-4 my-2 flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2">
      <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="font-mono text-xs text-destructive">{content}</div>
    </div>
  );
}

function getToolIcon(toolName: string) {
  const lower = toolName.toLowerCase();
  if (lower.includes("bash") || lower.includes("shell"))
    return <TerminalIcon className="size-3.5" />;
  if (
    lower.includes("edit") ||
    lower.includes("write") ||
    lower.includes("read")
  )
    return <FileEdit className="size-3.5" />;
  if (lower.includes("grep") || lower.includes("glob") || lower.includes("search"))
    return <Search className="size-3.5" />;
  if (lower.includes("web") || lower.includes("fetch"))
    return <Globe className="size-3.5" />;
  return <TerminalIcon className="size-3.5" />;
}

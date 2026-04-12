/**
 * ToolActionsGroup — renders a cluster of consecutive tool_use + tool_result
 * messages as a collapsible card with a summary header.
 *
 * Architecture borrowed from CodePilot's tool-actions-group.tsx:
 *   - Auto-expands while streaming, auto-collapses when done.
 *   - Manual toggle overrides auto-behavior.
 *   - Collapsed groups render NO children (lazy rendering for perf).
 *   - Context-groups consecutive Read/Grep/Glob under "Gathered context".
 */

import { memo, useState, useEffect, useRef, useMemo } from "react";
import {
  ChevronDown,
  ChevronRight,
  Loader2,
  CheckCircle2,
  AlertCircle,
} from "lucide-react";
import { getToolMeta, isContextTool } from "./tool-meta";
import { Message } from "./Message";
import type { ConversationMessage } from "@/features/common/message-types";

interface ToolActionsGroupProps {
  messages: ConversationMessage[];
  isStreaming?: boolean;
}

/** Compute summary stats from a tool group. */
function summarize(messages: ConversationMessage[]) {
  let toolCalls = 0;
  let completed = 0;
  let errors = 0;
  let contextCalls = 0;
  const toolNames: string[] = [];

  for (const msg of messages) {
    if (msg.type === "tool_use") {
      toolCalls++;
      const name = msg.toolUse?.toolName ?? "";
      toolNames.push(name);
      if (isContextTool(name)) contextCalls++;
    }
    if (msg.type === "tool_result") {
      completed++;
      if (msg.toolResult?.isError) errors++;
    }
  }

  // If most tool calls are context-gathering, label the whole group as such
  const isContextGroup = contextCalls >= 3 && contextCalls >= toolCalls * 0.6;
  const running = toolCalls - completed;

  return { toolCalls, completed, errors, running, isContextGroup, toolNames };
}

export const ToolActionsGroup = memo(function ToolActionsGroup({
  messages,
  isStreaming = false,
}: ToolActionsGroupProps) {
  const [manualToggle, setManualToggle] = useState<boolean | null>(null);
  const wasStreamingRef = useRef(isStreaming);

  const stats = useMemo(() => summarize(messages), [messages]);

  // Auto-collapse when streaming ends
  useEffect(() => {
    if (wasStreamingRef.current && !isStreaming) {
      // Only auto-collapse if user hasn't manually toggled
      if (manualToggle === null) {
        // leave collapsed (default state when not streaming)
      }
    }
    wasStreamingRef.current = isStreaming;
  }, [isStreaming, manualToggle]);

  // Determine expanded state: manual override > streaming auto-expand > default collapsed
  const expanded = manualToggle ?? isStreaming;

  const handleToggle = () => {
    setManualToggle((prev) => !(prev ?? isStreaming));
  };

  // Pick a representative icon from the first tool call
  const firstToolName = stats.toolNames[0] ?? "Tool";
  const { icon: FirstIcon, color: firstColor } = getToolMeta(firstToolName);

  // Build status line
  const statusParts: string[] = [];
  if (stats.running > 0) statusParts.push(`${stats.running} running`);
  if (stats.completed > 0) statusParts.push(`${stats.completed - stats.errors} done`);
  if (stats.errors > 0) statusParts.push(`${stats.errors} failed`);

  const label = stats.isContextGroup
    ? "Gathered context"
    : stats.toolCalls === 1
      ? getToolMeta(firstToolName).label
      : `${stats.toolCalls} tool calls`;

  return (
    <div className="ask-tool-group rounded-lg border border-border/40 bg-card/50">
      {/* Header */}
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-body-sm transition-colors hover:bg-accent/50"
        onClick={handleToggle}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3 shrink-0 text-muted-foreground" />
        )}
        <FirstIcon className="size-3.5 shrink-0" style={{ color: firstColor }} />
        <span className="font-medium text-foreground">{label}</span>

        {/* Status badges */}
        <span className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
          {stats.running > 0 && (
            <span className="flex items-center gap-0.5">
              <Loader2 className="size-3 animate-spin" style={{ color: "var(--deeptutor-primary, var(--claude-orange))" }} />
              {stats.running} 执行中
            </span>
          )}
          {stats.completed > 0 && stats.errors === 0 && (
            <span className="flex items-center gap-0.5">
              <CheckCircle2 className="size-3" style={{ color: "var(--color-success)" }} />
              {stats.completed} 完成
            </span>
          )}
          {stats.errors > 0 && (
            <span className="flex items-center gap-0.5">
              <AlertCircle className="size-3" style={{ color: "var(--color-error)" }} />
              {stats.errors} 失败
            </span>
          )}
        </span>
      </button>

      {/* Lazy-rendered children */}
      {expanded && (
        <div className="border-t border-border/30 px-1 py-1">
          {messages.map((msg) => (
            <Message key={msg.id} message={msg} />
          ))}
        </div>
      )}
    </div>
  );
});

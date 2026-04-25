/**
 * ToolActionsGroup — renders a cluster of consecutive tool_use + tool_result
 * messages as a compact work-log section with a summary header.
 *
 * Architecture borrowed from CodePilot's tool-actions-group.tsx:
 *   - Auto-expands while streaming, auto-collapses when done.
 *   - Manual toggle overrides auto-behavior.
 *   - Collapsed groups render NO children (lazy rendering for perf).
 *   - Context-groups consecutive Read/Grep/Glob under a work-log label.
 */

import { memo, useState, useEffect, useRef, useMemo } from "react";
import {
  ChevronDown,
  ChevronRight,
  Loader2,
  CheckCircle2,
  AlertCircle,
} from "lucide-react";
import { Shimmer } from "./Shimmer";
import { getToolMeta, isContextTool } from "./tool-meta";
import { Message } from "./Message";
import type { ConversationMessage } from "@/features/common/message-types";

interface ToolActionsGroupProps {
  messages: ConversationMessage[];
  isStreaming?: boolean;
}

function workVerb(toolName: string): string {
  const lower = toolName.toLowerCase();
  if (lower === "read" || lower.includes("read")) return "读取文件";
  if (lower === "grep" || lower.includes("grep")) return "搜索代码";
  if (lower === "glob" || lower.includes("glob")) return "扫描文件";
  if (lower === "bash" || lower.includes("shell") || lower === "powershell")
    return "执行命令";
  if (lower.includes("web")) return "访问网页";
  if (lower.includes("edit")) return "编辑文件";
  if (lower.includes("write")) return "写入文件";
  if (lower.includes("todo")) return "更新任务";
  if (lower === "agent") return "委托子任务";
  return "调用工具";
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

  const label = stats.isContextGroup
    ? "收集上下文"
    : stats.toolCalls === 1
      ? workVerb(firstToolName)
      : `${stats.toolCalls} 步工具工作`;

  return (
    <div className="ask-tool-group">
      {/* Header */}
      <button
        type="button"
        className="ask-tool-group-header"
        onClick={handleToggle}
        aria-expanded={expanded}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0 text-muted-foreground/75" />
        ) : (
          <ChevronRight className="size-3 shrink-0 text-muted-foreground/75" />
        )}
        <span
          className={isStreaming ? "ask-work-dot ask-work-dot--active" : "ask-work-dot"}
          aria-hidden
        />
        <FirstIcon className="size-3.5 shrink-0" style={{ color: firstColor }} />
        <span className="ask-tool-group-title font-medium text-foreground">{label}</span>

        {/* Status badges */}
        <span className="ask-tool-group-status ml-auto flex items-center gap-2 text-[11px] text-muted-foreground">
          {stats.running > 0 && (
            <span className="flex items-center gap-0.5">
              <Loader2 className="size-3 animate-spin" style={{ color: "var(--deeptutor-primary, var(--claude-orange))" }} />
              <Shimmer className="text-[11px] font-medium">{stats.running} 执行中</Shimmer>
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
        <div className="ask-tool-group-body">
          {messages.map((msg) => (
            <Message key={msg.id} message={msg} />
          ))}
        </div>
      )}
    </div>
  );
});

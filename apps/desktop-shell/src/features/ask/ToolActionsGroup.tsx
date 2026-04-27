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
  AlertCircle,
} from "lucide-react";
import { Shimmer } from "./Shimmer";
import { getToolMeta, isContextTool } from "./tool-meta";
import type { ConversationMessage } from "@/features/common/message-types";

interface ToolActionsGroupProps {
  messages: ConversationMessage[];
  isStreaming?: boolean;
}

type PairedToolStatus = "running" | "done" | "error";

type PairedToolEntry = {
  toolUseMessage: ConversationMessage;
  result: ConversationMessage | null;
  status: PairedToolStatus;
};

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

function pairToolMessages(messages: ConversationMessage[]): PairedToolEntry[] {
  const resultByToolUseId = new Map<string, ConversationMessage>();
  for (const msg of messages) {
    if (msg.type === "tool_result" && msg.toolResult?.toolUseId) {
      resultByToolUseId.set(msg.toolResult.toolUseId, msg);
    }
  }

  const entries: PairedToolEntry[] = [];
  for (const msg of messages) {
    if (msg.type !== "tool_use") continue;

    const toolUseId = msg.toolUse?.toolUseId;
    const result = toolUseId ? resultByToolUseId.get(toolUseId) ?? null : null;
    const status: PairedToolStatus = !result
      ? "running"
      : result.toolResult?.isError
        ? "error"
        : "done";

    entries.push({ toolUseMessage: msg, result, status });
  }

  return entries;
}

/** Compute summary stats from a tool group. */
function summarize(entries: PairedToolEntry[]) {
  let completed = 0;
  let errors = 0;
  let contextCalls = 0;
  const toolNames: string[] = [];

  for (const entry of entries) {
    const name = entry.toolUseMessage.toolUse?.toolName ?? "";
    toolNames.push(name);
    if (isContextTool(name)) contextCalls++;
    if (entry.status !== "running") {
      completed++;
      if (entry.status === "error") errors++;
    }
  }

  const toolCalls = entries.length;
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

  const entries = useMemo(() => pairToolMessages(messages), [messages]);
  const stats = useMemo(() => summarize(entries), [entries]);

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
  const groupTone =
    stats.running > 0 ? "working" : stats.errors > 0 ? "error" : stats.completed > 0 ? "ok" : "idle";

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
        <span className="ask-state-dot" data-tone={groupTone} aria-hidden />
        <FirstIcon className="size-3.5 shrink-0" style={{ color: firstColor }} />
        <span className="ask-tool-group-title font-medium text-foreground">{label}</span>

        {/* Status badges */}
        <span className="ask-tool-group-status ml-auto flex items-center gap-2 text-[11px] text-muted-foreground">
          {stats.running > 0 && (
            <span className="flex items-center gap-0.5">
              <span className="ask-state-dot" data-tone="working" aria-hidden />
              <Shimmer className="text-[11px] font-medium">{stats.running} 执行中</Shimmer>
            </span>
          )}
          {stats.completed > 0 && stats.errors === 0 && (
            <span className="flex items-center gap-0.5">
              <span className="ask-state-dot" data-tone="ok" aria-hidden />
              {stats.completed} 完成
            </span>
          )}
          {stats.errors > 0 && (
            <span className="flex items-center gap-0.5">
              <span className="ask-state-dot" data-tone="error" aria-hidden />
              {stats.errors} 失败
            </span>
          )}
        </span>
      </button>

      {/* Lazy-rendered children */}
      {expanded && (
        <div className="ask-tool-group-body">
          {entries.map((entry) => (
            <PairedToolRow
              key={entry.toolUseMessage.id}
              entry={entry}
            />
          ))}
        </div>
      )}
    </div>
  );
});

function PairedToolRow({ entry }: { entry: PairedToolEntry }) {
  const [expanded, setExpanded] = useState(false);
  const { toolUseMessage, result, status } = entry;
  const toolName = toolUseMessage.toolUse?.toolName ?? "Tool";
  const toolInput = toolUseMessage.toolUse?.toolInput ?? "";
  const output = result?.toolResult?.output ?? "";
  const lines = output.split("\n");
  const lineCount = output ? lines.length : 0;
  const { icon: ToolIcon, label, color } = getToolMeta(toolName);

  const parsedInput = useMemo(() => {
    try {
      return JSON.parse(toolInput) as Record<string, unknown>;
    } catch {
      return null;
    }
  }, [toolInput]);

  const inputPreview = useMemo(
    () => previewToolInput(parsedInput, toolInput),
    [parsedInput, toolInput],
  );
  const outputPreview = output ? lines[0]?.slice(0, 80) : "";

  const rowClass =
    status === "running"
      ? "ask-tool-row ask-tool-row--running"
      : status === "error"
        ? "ask-tool-row ask-tool-row--error"
        : "ask-tool-row ask-tool-row--completed";

  return (
    <div className="ask-tool-log">
      <button
        type="button"
        className={rowClass}
        onClick={() => setExpanded((value) => !value)}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0 text-muted-foreground/70" />
        ) : (
          <ChevronRight className="size-3 shrink-0 text-muted-foreground/70" />
        )}
        {status === "error" ? (
          <AlertCircle className="size-3.5 shrink-0" style={{ color: "var(--color-error)" }} />
        ) : (
          <ToolIcon className="size-3.5 shrink-0" style={{ color }} />
        )}
        <span className="ask-tool-name" style={{ color }}>{label}</span>
        <PairedStatusBadge status={status} />
        {status !== "running" && lineCount > 3 && (
          <span className="rounded bg-muted/50 px-1 py-0.5 text-[10px] text-muted-foreground/70">
            {lineCount} 行
          </span>
        )}
        <span className="ask-tool-preview">
          {outputPreview || inputPreview}
        </span>
      </button>

      {expanded && (
        <div className="ask-tool-detail max-h-[400px] overflow-auto">
          <div className="border-b border-border/20 px-3 py-2">
            <div className="mb-1 font-mono text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
              Input
            </div>
            {parsedInput ? (
              <StructuredPreview params={parsedInput} />
            ) : (
              <pre className="overflow-x-auto whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-foreground/80">
                {toolInput}
              </pre>
            )}
          </div>

          {result ? (
            <div className="px-3 py-2">
              <div className="mb-1 font-mono text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                Result
              </div>
              <pre className="overflow-x-auto whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-foreground/80">
                {output}
              </pre>
            </div>
          ) : (
            <div className="flex items-center gap-1.5 px-3 py-2 text-[11px] text-muted-foreground/70">
              <span className="ask-state-dot" data-tone="working" aria-hidden />
              <span>等待工具结果…</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function PairedStatusBadge({ status }: { status: PairedToolStatus }) {
  const tone = status === "running" ? "working" : status === "error" ? "error" : "ok";
  const label = status === "running" ? "执行中" : status === "error" ? "失败" : "完成";

  return (
    <span className="ask-tool-status">
      <span className="ask-state-dot" data-tone={tone} aria-hidden />
      {label}
    </span>
  );
}

function previewToolInput(
  parsedInput: Record<string, unknown> | null,
  toolInput: string,
): string {
  if (parsedInput) {
    if ("command" in parsedInput) return String(parsedInput.command);
    if ("file_path" in parsedInput) return String(parsedInput.file_path);
    if ("pattern" in parsedInput) {
      const path = parsedInput.path ? ` in ${parsedInput.path}` : "";
      return `${parsedInput.pattern}${path}`;
    }
    if ("description" in parsedInput && "prompt" in parsedInput) {
      const type = parsedInput.subagent_type ? `[${parsedInput.subagent_type}] ` : "";
      return `${type}${parsedInput.description}`;
    }
    if ("query" in parsedInput) return String(parsedInput.query);
    if ("url" in parsedInput) return String(parsedInput.url);
    if ("old_string" in parsedInput && "new_string" in parsedInput) {
      return `${String(parsedInput.old_string).slice(0, 30)} ↙ ${String(parsedInput.new_string).slice(0, 30)}`;
    }
    if ("content" in parsedInput && "file_path" in parsedInput) {
      return String(parsedInput.file_path);
    }
    if ("content" in parsedInput) return String(parsedInput.content).slice(0, 60);
    if ("skill" in parsedInput) return String(parsedInput.skill);
  }
  return toolInput.slice(0, 100);
}

function StructuredPreview({ params }: { params: Record<string, unknown> }) {
  return (
    <div className="divide-y divide-border/20">
      {Object.entries(params).map(([key, value]) => {
        const strValue = typeof value === "string" ? value : JSON.stringify(value, null, 2);
        return (
          <div key={key} className="flex gap-3 py-1.5">
            <span className="w-24 shrink-0 truncate font-mono text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
              {key}
            </span>
            <pre className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-foreground/80">
              {strValue}
            </pre>
          </div>
        );
      })}
    </div>
  );
}

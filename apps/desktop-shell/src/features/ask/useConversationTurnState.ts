import { useEffect, useMemo, useRef, useState } from "react";
import type { ConversationMessage } from "@/features/common/message-types";
import type { DesktopSessionDetail } from "@/lib/tauri";
import { classifyAskError, type AskErrorKind } from "./ask-error-classifier";

export type ConversationTurnState =
  | "idle"
  | "composing"
  | "sending"
  | "connecting"
  | "thinking"
  | "tool_planning"
  | "tool_running"
  | "tool_waiting"
  | "streaming"
  | "waiting_permission"
  | "complete"
  | "interrupted"
  | "failed_retriable"
  | "failed_fatal";

export type ConversationTone = "idle" | "ok" | "working" | "warn" | "error";

export interface ConversationTurnStatus {
  state: ConversationTurnState;
  tone: ConversationTone;
  label: string;
  detail: string;
  metricsLabel: string;
  elapsedMs: number;
  inputTokens: number;
  outputTokens: number;
  activeToolName: string | null;
  isWorking: boolean;
  isInputBlocked: boolean;
  canInterrupt: boolean;
  canRetry: boolean;
  failureKind?: AskErrorKind;
}

interface UseConversationTurnStateInput {
  session: DesktopSessionDetail | null;
  messages: ConversationMessage[];
  isSending: boolean;
  isRunning: boolean;
  isReplayStreaming: boolean;
  streamingContent: string;
  streamingThinking: string;
  hasPendingPermission: boolean;
  errorMessage?: string;
  settingsUnready?: boolean;
  isComposing: boolean;
  interruptedAt: number | null;
}

const WORKING_STATES: ReadonlySet<ConversationTurnState> = new Set([
  "sending",
  "connecting",
  "thinking",
  "tool_planning",
  "tool_running",
  "tool_waiting",
  "streaming",
]);

function estimateTokens(text: string): number {
  const trimmed = text.trim();
  if (!trimmed) return 0;
  // Mixed Chinese/English approximation. Good enough for live UI
  // movement; final token usage still comes from backend `usage`.
  const cjk = trimmed.match(/[\u3400-\u9fff]/g)?.length ?? 0;
  const nonCjk = Math.max(0, trimmed.length - cjk);
  return Math.max(1, Math.round(cjk * 0.72 + nonCjk / 4));
}

function formatDuration(ms: number): string {
  const seconds = Math.max(0, ms / 1000);
  if (seconds < 10) return `${seconds.toFixed(1)}s`;
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${Math.round(seconds % 60)}s`;
}

function latestUserText(messages: ConversationMessage[]): string {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const msg = messages[i];
    if (msg.role === "user" && msg.type === "text") return msg.content;
  }
  return "";
}

function messagesAfterLastUser(messages: ConversationMessage[]) {
  let lastUserIndex = -1;
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i].role === "user") {
      lastUserIndex = i;
      break;
    }
  }
  return lastUserIndex >= 0 ? messages.slice(lastUserIndex + 1) : messages;
}

function findLastUserMessage(messages: ConversationMessage[]) {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i].role === "user") return messages[i];
  }
  return null;
}

/**
 * Aggregate token usage from all messages in the current turn.
 * Current turn = everything after the last user message.
 */
function aggregateCurrentTurnTokens(messages: ConversationMessage[]): {
  inputTokens: number;
  outputTokens: number;
} {
  const turnMessages = messagesAfterLastUser(messages);
  let inputTokens = 0;
  let outputTokens = 0;

  for (const msg of turnMessages) {
    if (msg.usage) {
      inputTokens += msg.usage.inputTokens ?? 0;
      outputTokens += msg.usage.outputTokens ?? 0;
    }
  }

  return { inputTokens, outputTokens };
}

function estimateCurrentTurnTokens(
  messages: ConversationMessage[],
  streamingContent: string,
): {
  inputTokens: number;
  outputTokens: number;
} {
  const turnMessages = messagesAfterLastUser(messages);
  let inputTokens = estimateTokens(latestUserText(messages));
  let outputTokens = estimateTokens(streamingContent);

  for (const msg of turnMessages) {
    if (msg.type === "tool_result") {
      // Tool results become model input in the next OpenAI compat turn.
      inputTokens += estimateTokens(msg.toolResult?.output ?? msg.content);
      continue;
    }

    if (msg.role === "assistant") {
      if (msg.type === "tool_use") {
        outputTokens += estimateTokens(msg.toolUse?.toolInput ?? msg.content);
      } else {
        outputTokens += estimateTokens(msg.content);
      }
    }
  }

  return { inputTokens, outputTokens };
}

function timestampLooksLikeEpochMs(value: number): boolean {
  // Flattened backend messages currently use display order as timestamp.
  // Only treat large millisecond values as real time.
  return value > 1_000_000_000_000;
}

/**
 * Compute elapsed time for the current turn from message timestamps when
 * timestamps are real epoch-ms values. Returns 0 for order-only timestamps.
 */
function computeCurrentTurnElapsed(
  messages: ConversationMessage[],
  isWorking: boolean,
  now: number,
): number {
  const userMsg = findLastUserMessage(messages);
  if (!userMsg || !timestampLooksLikeEpochMs(userMsg.timestamp)) return 0;

  const lastMsg = messages[messages.length - 1];
  const endTime =
    isWorking || !lastMsg || !timestampLooksLikeEpochMs(lastMsg.timestamp)
      ? now
      : lastMsg.timestamp;

  return Math.max(0, endTime - userMsg.timestamp);
}

function summarizeActiveTool(messages: ConversationMessage[]) {
  const tail = messagesAfterLastUser(messages);
  const completed = new Set<string>();
  for (const msg of tail) {
    if (msg.type === "tool_result" && msg.toolResult?.toolUseId) {
      completed.add(msg.toolResult.toolUseId);
    }
  }

  for (let i = tail.length - 1; i >= 0; i -= 1) {
    const msg = tail[i];
    if (msg.type !== "tool_use" || !msg.toolUse) continue;
    const id = msg.toolUse.toolUseId;
    if (!id || !completed.has(id)) {
      return {
        name: msg.toolUse.toolName,
        hasRunningTool: true,
        hasRecentToolResult: false,
      };
    }
  }

  const last = tail[tail.length - 1];
  if (last?.type === "tool_result") {
    return {
      name: last.toolResult?.toolName ?? null,
      hasRunningTool: false,
      hasRecentToolResult: true,
    };
  }

  return {
    name: null,
    hasRunningTool: false,
    hasRecentToolResult: false,
  };
}

function isFatalError(kind: AskErrorKind, settingsUnready?: boolean): boolean {
  return (
    settingsUnready === true ||
    kind === "credentials_missing" ||
    kind === "broker_empty" ||
    kind === "session_not_found"
  );
}

function labelForState(
  state: ConversationTurnState,
  modelLabel: string,
  activeToolName: string | null,
) {
  switch (state) {
    case "idle":
      return "新对话 · 未命名";
    case "composing":
      return "正在输入";
    case "sending":
      return "准备中…";
    case "connecting":
      return "连接 AI 服务…";
    case "thinking":
      return `${modelLabel} 思考中`;
    case "tool_planning":
      return "规划工具调用";
    case "tool_running":
      return `执行中 · ${activeToolName ?? "工具"}`;
    case "tool_waiting":
      return "工具完成 · 整理结果中";
    case "streaming":
      return "回答中";
    case "waiting_permission":
      return "等待你的确认";
    case "complete":
      return "已完成";
    case "interrupted":
      return "已停止";
    case "failed_retriable":
      return "出错了 · 可重试";
    case "failed_fatal":
      return "服务不可用";
  }
}

function toneForState(state: ConversationTurnState): ConversationTone {
  if (state === "failed_fatal" || state === "failed_retriable") return "error";
  if (state === "waiting_permission") return "warn";
  if (WORKING_STATES.has(state)) return "working";
  if (state === "complete") return "ok";
  return "idle";
}

export function useConversationTurnState({
  session,
  messages,
  isSending,
  isRunning,
  isReplayStreaming,
  streamingContent,
  streamingThinking,
  hasPendingPermission,
  errorMessage,
  settingsUnready,
  isComposing,
  interruptedAt,
}: UseConversationTurnStateInput): ConversationTurnStatus {
  const [now, setNow] = useState(() => Date.now());
  const turnStartedAtRef = useRef<number | null>(null);
  const lastCompletedDurationRef = useRef(0);
  const lastInterruptedAtRef = useRef<number | null>(null);
  const activeTurnUserIdRef = useRef<string | null>(null);

  const safeMessages = Array.isArray(messages) ? messages : [];
  const active = isSending || isRunning || isReplayStreaming;
  const lastUserMessage = findLastUserMessage(safeMessages);

  useEffect(() => {
    const userId = lastUserMessage?.id ?? null;
    if (userId && userId !== activeTurnUserIdRef.current) {
      activeTurnUserIdRef.current = userId;
      if (active) {
        if (turnStartedAtRef.current === null) {
          turnStartedAtRef.current = Date.now();
        }
        lastCompletedDurationRef.current = 0;
      }
    }

    if (active && turnStartedAtRef.current === null) {
      turnStartedAtRef.current = Date.now();
    }
    if (!active && turnStartedAtRef.current !== null) {
      lastCompletedDurationRef.current = Date.now() - turnStartedAtRef.current;
      turnStartedAtRef.current = null;
    }
  }, [active, lastUserMessage?.id]);

  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [active]);

  useEffect(() => {
    if (interruptedAt) {
      lastInterruptedAtRef.current = interruptedAt;
      lastCompletedDurationRef.current =
        turnStartedAtRef.current != null
          ? Date.now() - turnStartedAtRef.current
          : lastCompletedDurationRef.current;
      turnStartedAtRef.current = null;
      setNow(Date.now());
    }
  }, [interruptedAt]);

  return useMemo(() => {
    const activeTool = summarizeActiveTool(safeMessages);
    const { inputTokens: aggInputTokens, outputTokens: aggOutputTokens } =
      aggregateCurrentTurnTokens(safeMessages);
    const {
      inputTokens: estimatedInputTokens,
      outputTokens: estimatedOutputTokens,
    } = estimateCurrentTurnTokens(safeMessages, streamingContent);
    const inputTokens =
      aggInputTokens > 0 ? aggInputTokens : estimatedInputTokens;
    const outputTokens =
      aggOutputTokens > 0 ? aggOutputTokens : estimatedOutputTokens;
    const messageBasedElapsed = computeCurrentTurnElapsed(safeMessages, active, now);
    const elapsedMs = active
      ? Math.max(
          messageBasedElapsed,
          now - (turnStartedAtRef.current ?? now),
        )
      : messageBasedElapsed > 0
        ? messageBasedElapsed
        : lastCompletedDurationRef.current;

    let state: ConversationTurnState;
    let failureKind: AskErrorKind | undefined;

    if (interruptedAt) {
      state = "interrupted";
    } else if (errorMessage || settingsUnready) {
      const classified = classifyAskError(errorMessage ?? "settings not ready");
      failureKind = classified.kind;
      state = isFatalError(classified.kind, settingsUnready)
        ? "failed_fatal"
        : "failed_retriable";
    } else if (hasPendingPermission) {
      state = "waiting_permission";
    } else if (isSending) {
      state = "sending";
    } else if (isRunning || isReplayStreaming) {
      if (streamingContent.length > 0) {
        state = "streaming";
      } else if (activeTool.hasRunningTool) {
        state = "tool_running";
      } else if (activeTool.hasRecentToolResult) {
        state = "tool_waiting";
      } else if (streamingThinking.length > 0) {
        state = "thinking";
      } else if (elapsedMs > 1200) {
        state = "thinking";
      } else {
        state = "connecting";
      }
    } else if (isComposing) {
      state = "composing";
    } else if (safeMessages.length > 0) {
      state = "complete";
    } else {
      state = "idle";
    }

    const isWorking = WORKING_STATES.has(state);
    const tone = toneForState(state);
    const label = labelForState(
      state,
      session?.model_label || "AI",
      activeTool.name,
    );
    const detail =
      state === "interrupted" && lastInterruptedAtRef.current
        ? "你中断了这次生成"
        : state === "failed_fatal"
          ? "请先检查模型或设置"
          : state === "failed_retriable"
            ? "可以重试或切换模型"
            : state === "waiting_permission"
              ? "处理上方授权请求后继续"
              : isWorking && elapsedMs > 30_000
                ? "耗时较长，可以按 Esc 中断或继续等待"
                : "";

    const metricsLabel =
      isWorking
        ? `↑ ${inputTokens} tokens · ${formatDuration(elapsedMs)} · Esc 中断`
        : state === "complete"
          ? `用时 ${formatDuration(elapsedMs)} · ↑${inputTokens} ↓${outputTokens} tokens`
          : state === "interrupted"
            ? `用时 ${formatDuration(elapsedMs)} · 你按了 Esc`
            : state.startsWith("failed")
              ? `网络或服务异常 · ${formatDuration(elapsedMs)}`
              : "";

    return {
      state,
      tone,
      label,
      detail,
      metricsLabel,
      elapsedMs,
      inputTokens,
      outputTokens,
      activeToolName: activeTool.name,
      isWorking,
      isInputBlocked:
        isWorking || state === "waiting_permission" || state === "failed_fatal",
      canInterrupt: isWorking,
      canRetry: state === "failed_retriable",
      failureKind,
    };
  }, [
    active,
    errorMessage,
    hasPendingPermission,
    interruptedAt,
    isComposing,
    isReplayStreaming,
    isRunning,
    isSending,
    safeMessages,
    now,
    session,
    settingsUnready,
    streamingContent,
    streamingThinking,
  ]);
}

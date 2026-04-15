/**
 * S0.3 → S1 redesign: Ask page workbench.
 *
 * Architecture changes from S0.3:
 *   - ConversationScroller (use-stick-to-bottom) replaces manual scroll div
 *   - StreamingMessage component replaces inline StreamingIndicator
 *   - MessageList groups tool messages into ToolActionsGroup
 *   - SlashCommandPalette wired into Composer
 *   - DeepTutor warm visual language (#C35A2C + Lora serif + #FAF9F6)
 *
 * Preserved:
 *   - Welcome screen + demo mode
 *   - Permission dialog rendering
 *   - Tool / message flattening from runtime types
 *   - Subagent panel (MaintainerTaskTree)
 *   - All store wiring (settings, permissions, streaming)
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Terminal,
  FileEdit,
  Search,
  Globe,
  Code2,
  Zap,
  MessageSquare,
  Play,
} from "lucide-react";
import { AskHeader } from "./AskHeader";
import { useWikiQuery } from "./useWikiQuery";
import { WikiQueryMessage } from "./WikiQueryMessage";
import { MessageList } from "./MessageList";
import { Composer } from "./Composer";
import { ConversationScroller } from "./ConversationScroller";
import { ScrollToBottomButton } from "./ScrollToBottomButton";
import { WikiPermissionDialog } from "@/features/permission/WikiPermissionDialog";
import type { PermissionAction } from "@/features/permission/permission-types";
import {
  MaintainerTaskTree,
  extractSubagents,
} from "@/features/inbox/MaintainerTaskTree";
import { usePermissionsStore } from "@/state/permissions-store";
import { useStreamingStore } from "@/state/streaming-store";
import {
  forwardPermissionDecision,
  type ContentBlock,
  type DesktopSessionDetail,
  type RuntimeConversationMessage,
} from "@/lib/tauri";
import { compactSession } from "./api/client";
import type { ConversationMessage } from "@/features/common/message-types";
import { MOCK_DEMO_MESSAGES } from "./mockDemoMessages";

/** Detect URLs in text and return the first one. */
function extractUrl(text: string): string | null {
  const match = text.match(/https?:\/\/[^\s，。！？]+/);
  return match ? match[0] : null;
}

// v2 bugfix: fetchAndIngestUrl + isValidContent removed from the frontend.
// Backend's `maybe_enrich_url` (desktop-core) now handles the entire
// fetch → ingest → inject-to-system-prompt flow. The frontend only
// shows a progress hint via addSystemMessage in handleSendWithUrlFetch.

interface ProviderOption {
  id: string;
  label: string;
  model: string;
  isActive: boolean;
}

interface AskWorkbenchProps {
  session: DesktopSessionDetail | null;
  isLoadingSession: boolean;
  isSending: boolean;
  errorMessage?: string;
  onSend: (message: string) => void | Promise<void>;
  onStop?: () => void;
  onCreateSession?: () => void;
  modelLabel?: string;
  environmentLabel?: string;
  projectPath?: string;
  providers?: ProviderOption[];
  onSwitchProvider?: (id: string) => void;
}

export function AskWorkbench({
  session,
  isLoadingSession,
  isSending,
  errorMessage,
  onSend,
  onStop,
  onCreateSession,
  modelLabel = "AI",
  environmentLabel = "Local",
  projectPath,
  providers,
  onSwitchProvider,
}: AskWorkbenchProps) {
  // Python deps auto-installed by backend on startup — no frontend action needed.

  const pendingPermission = usePermissionsStore(
    (state) => state.pendingRequest
  );
  const setPendingPermission = usePermissionsStore(
    (state) => state.setPendingPermission
  );
  const clearPendingPermission = usePermissionsStore(
    (state) => state.clearPendingPermission
  );
  const streamingContent = useStreamingStore((s) => s.streamingContent);

  const [showDemo, setShowDemo] = useState(false);
  const [localMessages, setLocalMessages] = useState<ConversationMessage[]>([]);
  const [showAgentPanel, setShowAgentPanel] = useState(false);

  const messages = useMemo(
    () => flattenSessionMessages(session?.session.messages ?? []),
    [session?.session.messages]
  );
  const displayMessages = useMemo(() => {
    if (messages.length > 0) return [...messages, ...localMessages];
    if (showDemo) return [...MOCK_DEMO_MESSAGES, ...localMessages];
    return localMessages;
  }, [messages, showDemo, localMessages]);

  const agentCount = useMemo(
    () => extractSubagents(displayMessages).length,
    [displayMessages]
  );
  // isRunning: true while the AI is generating a response.
  // Keep it true for a few seconds after POST returns, bridging the gap
  // until the next poll detects turn_state=running.
  const [extendedRunning, setExtendedRunning] = useState(false);
  useEffect(() => {
    if (isSending) {
      setExtendedRunning(true);
    }
  }, [isSending]);
  useEffect(() => {
    if (!extendedRunning) return;
    if (session?.turn_state === "running") {
      // Backend confirmed running — no longer need the extension
      setExtendedRunning(false);
      return;
    }
    // Auto-clear after 5s if backend never reports running (very short reply)
    const t = setTimeout(() => setExtendedRunning(false), 5000);
    return () => clearTimeout(t);
  }, [extendedRunning, session?.turn_state]);
  const isRunning = session?.turn_state === "running" || isSending || extendedRunning;

  // Clear local messages when session changes
  useEffect(() => {
    setLocalMessages([]);
  }, [session?.id]);

  const handlePermissionDecision = useCallback(
    (action: PermissionAction) => {
      if (pendingPermission) {
        if (session?.id) {
          void forwardPermissionDecision(session.id, {
            requestId: pendingPermission.id,
            decision: action,
          })
            .then(() => {
              clearPendingPermission(pendingPermission.id);
            })
            .catch((error) => {
              console.warn("Failed to forward permission decision to backend", {
                sessionId: session.id,
                requestId: pendingPermission.id,
                decision: action,
                error,
              });
            });
          return;
        }

        clearPendingPermission(pendingPermission.id);
      }
    },
    [clearPendingPermission, pendingPermission, session?.id]
  );

  const addSystemMessage = useCallback((text: string) => {
    setLocalMessages((prev) => [
      ...prev,
      {
        id: `system-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
        role: "system" as const,
        type: "text" as const,
        content: text,
        timestamp: Date.now(),
      },
    ]);
  }, []);

  // Slash command handlers
  const handleClear = useCallback(() => {
    setLocalMessages([]);
    onCreateSession?.();
  }, [onCreateSession]);

  const handleNewSession = useCallback(() => {
    onCreateSession?.();
  }, [onCreateSession]);

  const handleCompact = useCallback(() => {
    if (!session?.id) return;
    void compactSession(session.id)
      .then(() => addSystemMessage("会话已压缩。"))
      .catch((err) => addSystemMessage(`压缩失败：${err instanceof Error ? err.message : String(err)}`));
  }, [session?.id, addSystemMessage]);

  // Input ref for focus shortcut
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // v2: Wiki query hook for ?-prefixed questions.
  const wikiQuery = useWikiQuery();

  // Wrap onSend: detect ? prefix → wiki query; detect URLs → fetch; else → session send
  const handleSendWithUrlFetch = useCallback(async (message: string) => {
    // ?-prefix: route to /query (wiki knowledge Q&A)
    const trimmed = message.trimStart();
    if (trimmed.startsWith("?") || trimmed.startsWith("/query ")) {
      const question = trimmed.startsWith("/query ")
        ? trimmed.slice(7).trim()
        : trimmed.slice(1).trim();
      if (question) {
        wikiQuery.queryWiki(question);
        return;
      }
    }

    // v2 bugfix: unified URL handling. The backend's `maybe_enrich_url`
    // (desktop-core::append_user_message) handles fetch + ingest + inject
    // into system prompt. The frontend's only job is to show a progress
    // hint so the user knows the backend is working.
    //
    // Previously this function called `/api/desktop/wechat-fetch` directly
    // and inlined the enriched content into the user message, which both
    // polluted session history AND raced with the backend's own enrichment.
    const url = extractUrl(message);
    if (url) {
      const isWeChat = url.includes("mp.weixin.qq.com") || url.includes("weixin.qq.com");
      if (isWeChat) {
        addSystemMessage("⏳ 正在抓取微信文章（Playwright，最长 45 秒）...");
      } else {
        addSystemMessage(`⏳ 正在抓取 ${url.slice(0, 60)}${url.length > 60 ? "…" : ""}`);
      }
      // Fall through to backend. maybe_enrich_url will handle fetch/ingest
      // and inject content into the turn's system prompt.
    }
    await onSend(message);
  }, [onSend, addSystemMessage, wikiQuery]);

  return (
    <div className="flex flex-1 overflow-hidden">
     <div className="flex flex-1 flex-col overflow-hidden">
      <AskHeader
        projectPath={projectPath}
        modelLabel={modelLabel}
        environmentLabel={environmentLabel}
        isStreaming={isRunning}
        agentCount={agentCount}
        showAgentPanel={showAgentPanel}
        onToggleAgentPanel={() => setShowAgentPanel((v) => !v)}
      />

      {displayMessages.length === 0 && !isLoadingSession ? (
        <WelcomeScreen onShowDemo={() => setShowDemo(true)} />
      ) : (
        <ConversationScroller>
          {showDemo && (
            <div className="mb-2 flex items-center justify-between rounded-lg border border-border/30 bg-muted/10 px-3 py-1.5">
              <span className="text-label text-muted-foreground">
                演示模式 — 展示示例对话
              </span>
              <div className="flex items-center gap-2">
                {!pendingPermission && (
                  <button
                    className="text-label font-medium text-muted-foreground hover:text-foreground hover:underline"
                    onClick={() =>
                      setPendingPermission({
                        id: `demo-perm-${Date.now()}`,
                        toolName: "Bash",
                        toolInput: {
                          command: "npm install @radix-ui/react-dialog",
                        },
                        riskLevel: "high",
                      })
                    }
                  >
                    测试权限
                  </button>
                )}
                <button
                  className="text-label font-medium text-foreground hover:underline"
                  onClick={() => setShowDemo(false)}
                >
                  退出演示
                </button>
              </div>
            </div>
          )}

          {isLoadingSession && <MessageSkeleton />}

          <MessageList
            messages={displayMessages}
            streamingContent={streamingContent}
            isStreaming={isRunning && !pendingPermission}
          />

          {/* v2: Wiki query result (? prefix) — not stored in session history */}
          {(wikiQuery.isQuerying || wikiQuery.answer || wikiQuery.error) && (
            <WikiQueryMessage
              question={wikiQuery.question}
              answer={wikiQuery.answer}
              sources={wikiQuery.sources}
              isStreaming={wikiQuery.isQuerying}
              error={wikiQuery.error}
            />
          )}

          {/* Permission dialog rendered inside scroller for flow */}
          {pendingPermission && (
            <WikiPermissionDialog
              request={pendingPermission}
              onDecision={handlePermissionDecision}
            />
          )}

          {/* Error banner */}
          {errorMessage && (
            <div
              className="flex items-start gap-2 rounded-lg border px-3 py-2 text-body-sm"
              style={{
                borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
                backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
                color: "var(--color-error)",
              }}
            >
              <span className="flex-1">{errorMessage}</span>
              <button
                className="shrink-0 rounded px-1.5 py-0.5 text-label font-medium transition-colors hover:bg-[color-mix(in_srgb,var(--color-error)_15%,transparent)]"
                onClick={() => addSystemMessage("错误已忽略。你可以重试上一条消息。")}
              >
                忽略
              </button>
            </div>
          )}

          <ScrollToBottomButton />
        </ConversationScroller>
      )}

      <Composer
        onSend={handleSendWithUrlFetch}
        onStop={onStop}
        isBusy={isRunning || !!pendingPermission}
        modelLabel={modelLabel}
        environmentLabel={environmentLabel}
        providers={providers}
        onSwitchProvider={onSwitchProvider}
        inputRef={inputRef}
        onClear={handleClear}
        onNewSession={handleNewSession}
        onCompact={handleCompact}
      />

      {/* StatusLine removed — Composer's inline toolbar now shows model + permission + environment */}
     </div>

      {/* Maintainer task tree side panel (CCD soul ④) */}
      {showAgentPanel && (
        <MaintainerTaskTree
          messages={displayMessages}
          onClose={() => setShowAgentPanel(false)}
        />
      )}
    </div>
  );
}

/* ─── Loading skeleton ──────────────────────────────────────────── */

function MessageSkeleton() {
  return (
    <div className="space-y-3">
      {/* User message skeleton */}
      <div className="rounded-lg border border-border/30 bg-muted/10 p-3">
        <div className="mb-2 h-2.5 w-12 animate-pulse rounded bg-muted/40" />
        <div className="space-y-1.5">
          <div className="h-3 w-3/4 animate-pulse rounded bg-muted/30" />
          <div className="h-3 w-1/2 animate-pulse rounded bg-muted/30" />
        </div>
      </div>
      {/* Assistant message skeleton */}
      <div className="rounded-lg border border-border/30 bg-muted/10 p-3">
        <div className="mb-2 h-2.5 w-16 animate-pulse rounded bg-muted/40" />
        <div className="space-y-1.5">
          <div className="h-3 w-full animate-pulse rounded bg-muted/30" />
          <div className="h-3 w-5/6 animate-pulse rounded bg-muted/30" />
          <div className="h-3 w-2/3 animate-pulse rounded bg-muted/30" />
        </div>
      </div>
      {/* Tool use skeleton */}
      <div className="rounded-lg border border-border/30 bg-muted/10 p-2.5">
        <div className="flex items-center gap-2">
          <div className="size-3.5 animate-pulse rounded bg-muted/40" />
          <div className="h-3 w-20 animate-pulse rounded bg-muted/40" />
          <div className="h-3 flex-1 animate-pulse rounded bg-muted/20" />
        </div>
      </div>
    </div>
  );
}

/* ─── Welcome Screen ─────────────────────────────────────────────── */

function WelcomeScreen({ onShowDemo }: { onShowDemo?: () => void }) {
  const capabilities = [
    {
      icon: Terminal,
      title: "执行命令",
      desc: "运行 Shell 命令、脚本和构建工具",
      color: "var(--color-terminal-tool)",
    },
    {
      icon: FileEdit,
      title: "编辑文件",
      desc: "精确读写和修改代码",
      color: "var(--deeptutor-primary, var(--claude-orange))",
    },
    {
      icon: Search,
      title: "搜索代码",
      desc: "在代码库中查找文件和模式",
      color: "var(--deeptutor-purple, var(--claude-blue))",
    },
    {
      icon: Globe,
      title: "网络访问",
      desc: "抓取 URL 和搜索网络信息",
      color: "var(--deeptutor-purple, var(--agent-cyan))",
    },
    {
      icon: Code2,
      title: "多文件编辑",
      desc: "同时协调多个文件的修改",
      color: "var(--deeptutor-purple, var(--agent-purple))",
    },
    {
      icon: Zap,
      title: "MCP 工具",
      desc: "使用已连接的 MCP 服务器工具",
      color: "var(--deeptutor-warn, var(--color-fast-mode))",
    },
  ];

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-5 p-6">
      {/* Logo */}
      <div className="flex flex-col items-center gap-3">
        <div
          className="flex size-14 items-center justify-center rounded-2xl"
          style={{
            background: "linear-gradient(135deg, var(--deeptutor-primary, var(--claude-orange)), var(--deeptutor-primary-hi, var(--claude-orange-shimmer)))",
          }}
        >
          <MessageSquare className="size-7 text-white" />
        </div>
        <div className="text-center">
          <h2 className="ask-serif text-base font-semibold text-foreground">
            有什么我能帮你的？
          </h2>
          <p className="mt-1 max-w-sm text-body text-muted-foreground">
            我能阅读、编写和运行你项目中的代码。告诉我你需要什么。
          </p>
        </div>
      </div>

      {/* Capabilities grid */}
      <div className="grid w-full max-w-lg grid-cols-2 gap-2">
        {capabilities.map((item) => (
          <div
            key={item.title}
            className="flex items-start gap-2.5 rounded-lg border border-border/40 bg-card/50 p-3 shadow-[var(--deeptutor-shadow-sm,none)] transition-colors hover:bg-accent/50"
          >
            <item.icon className="mt-0.5 size-4 shrink-0" style={{ color: item.color }} />
            <div className="min-w-0">
              <div className="text-body-sm font-medium text-foreground">{item.title}</div>
              <div className="text-label leading-snug text-muted-foreground">{item.desc}</div>
            </div>
          </div>
        ))}
      </div>

      {/* Hint + Demo */}
      <div className="flex flex-col items-center gap-2">
        <div className="flex items-center gap-1.5 text-label text-muted-foreground/60">
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-caption">Enter</kbd>
          <span>发送</span>
          <span className="mx-1">|</span>
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-caption">Shift+Enter</kbd>
          <span>换行</span>
          <span className="mx-1">|</span>
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-caption">/</kbd>
          <span>命令</span>
        </div>
        {onShowDemo && (
          <button
            className="flex items-center gap-1.5 rounded-md border border-border/40 bg-card/50 px-3 py-1 text-label text-muted-foreground shadow-[var(--deeptutor-shadow-sm,none)] transition-colors hover:bg-accent/50 hover:text-foreground"
            onClick={onShowDemo}
          >
            <Play className="size-3" />
            查看演示对话
          </button>
        )}
      </div>
    </div>
  );
}

/* ─── Message flattening ─────────────────────────────────────────── */

function flattenSessionMessages(
  source: RuntimeConversationMessage[]
): ConversationMessage[] {
  const items: ConversationMessage[] = [];
  let order = 0;

  for (const message of source) {
    let usageAttached = false;
    for (const block of message.blocks) {
      order += 1;
      const entry = toDisplayMessage(message.role, block, order);
      if (entry) {
        // Attach usage data to the first text block of assistant messages
        if (!usageAttached && message.usage && entry.role === "assistant" && entry.type === "text") {
          entry.usage = {
            inputTokens: message.usage.input_tokens,
            outputTokens: message.usage.output_tokens,
          };
          usageAttached = true;
        }
        items.push(entry);
      }
    }
  }

  return items;
}

function toDisplayMessage(
  role: RuntimeConversationMessage["role"],
  block: ContentBlock,
  order: number
): ConversationMessage | null {
  if (block.type === "text") {
    return {
      id: `${role}-text-${order}`,
      role: role === "user" ? "user" : role === "system" ? "system" : "assistant",
      type: "text",
      content: block.text,
      timestamp: order,
    };
  }

  if (block.type === "tool_use") {
    return {
      id: block.id,
      role: "assistant",
      type: "tool_use",
      content: block.input,
      timestamp: order,
      toolUse: {
        toolUseId: block.id,
        toolName: block.name,
        toolInput: block.input,
      },
    };
  }

  return {
    id: `${block.tool_use_id}-result-${order}`,
    role: "assistant",
    type: "tool_result",
    content: block.output,
    timestamp: order,
    toolResult: {
      toolUseId: block.tool_use_id,
      toolName: block.tool_name,
      output: block.output,
      isError: block.is_error,
    },
  };
}

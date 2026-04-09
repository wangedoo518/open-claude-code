/**
 * S0.3 extraction target: Ask page workbench (CCD soul ① + ②).
 *
 * Original: features/session-workbench/SessionWorkbenchTerminal.tsx.
 *
 * MVP cuts per ClawWiki canonical §11.1 + §6.1:
 *   - Drop slash command handling (commandExecutor / handleSlashCommand).
 *     The Composer no longer accepts an `onSlashCommand` prop and the
 *     `addSystemMessage` "command failed" branch goes with it. Error
 *     dismissal still adds a system message via the same helper.
 *   - Drop the `useKeyboardShortcuts` integration during S0.3 — the
 *     hook lives in the still-existing session-workbench/ directory and
 *     S3 will rebuild a smaller Ask-specific shortcut surface.
 *
 * Preserved:
 *   - Welcome screen + demo mode (so AskPage's "View demo" CTA works
 *     immediately when S3 wires it up).
 *   - Streaming indicator + permission dialog rendering.
 *   - Tool / message flattening from runtime types.
 *   - Subagent panel (now MaintainerTaskTree).
 *   - All store wiring (settings, permissions, streaming).
 *
 * S3 will replace the body of this file with the ask_runtime-backed
 * implementation. The component shape stays put.
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
import { MessageList } from "./MessageList";
import { Composer } from "./Composer";
import { StatusLine } from "@/features/common/StatusLine";
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
import type { ConversationMessage } from "@/features/common/message-types";
import { MOCK_DEMO_MESSAGES } from "./mockDemoMessages";

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
}

export function AskWorkbench({
  session,
  isLoadingSession,
  isSending,
  errorMessage,
  onSend,
  onStop,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars -- reserved for S3 keyboard shortcuts
  onCreateSession: _onCreateSession,
  modelLabel = "Codex GPT-5.4",
  environmentLabel = "via internal broker",
  projectPath,
}: AskWorkbenchProps) {
  const pendingPermission = usePermissionsStore(
    (state) => state.pendingRequest
  );
  const setPendingPermission = usePermissionsStore(
    (state) => state.setPendingPermission
  );
  const clearPendingPermission = usePermissionsStore(
    (state) => state.clearPendingPermission
  );
  const [scrollNode, setScrollNode] = useState<HTMLDivElement | null>(null);
  const scrollCallbackRef = useCallback((node: HTMLDivElement | null) => {
    setScrollNode(node);
  }, []);
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
  const isRunning = session?.turn_state === "running" || isSending;

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

  // Input ref for focus shortcut (used by S3 keyboard shortcuts)
  const inputRef = useRef<HTMLTextAreaElement>(null);

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
        <div
          ref={scrollCallbackRef}
          className="flex-1 overflow-y-auto pb-4 [&::-webkit-scrollbar]:w-2 [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-border [&::-webkit-scrollbar-track]:bg-transparent"
        >
            {showDemo && (
              <div className="mx-4 mb-2 mt-1 flex items-center justify-between rounded-lg border border-border/30 bg-muted/10 px-3 py-1.5">
                <span className="text-label text-muted-foreground">
                  Demo mode — showing sample conversation
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
                      Test permission
                    </button>
                  )}
                  <button
                    className="text-label font-medium text-foreground hover:underline"
                    onClick={() => setShowDemo(false)}
                  >
                    Exit demo
                  </button>
                </div>
              </div>
            )}
            {isLoadingSession && <MessageSkeleton />}

            {/* Virtualized message list */}
            <MessageList
              messages={displayMessages}
              scrollElement={scrollNode}
            />

            {/* Fixed items after virtual list */}
            {pendingPermission && (
              <WikiPermissionDialog
                request={pendingPermission}
                onDecision={handlePermissionDecision}
              />
            )}
            {isRunning && !pendingPermission && <StreamingIndicator />}
            {errorMessage && (
              <div
                className="mx-4 mt-3 flex items-start gap-2 rounded-lg border px-3 py-2 text-body-sm"
                style={{
                  borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
                  backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
                  color: "var(--color-error)",
                }}
              >
                <span className="flex-1">{errorMessage}</span>
                <button
                  className="shrink-0 rounded px-1.5 py-0.5 text-label font-medium transition-colors hover:bg-[color-mix(in_srgb,var(--color-error)_15%,transparent)]"
                  onClick={() => addSystemMessage("Error dismissed. You can retry your last message.")}
                >
                  Dismiss
                </button>
              </div>
            )}
        </div>
      )}

      <Composer
        onSend={onSend}
        onStop={onStop}
        isBusy={isRunning || !!pendingPermission}
        environmentLabel={environmentLabel}
        inputRef={inputRef}
      />

      <StatusLine
        modelLabel={modelLabel}
        environmentLabel={environmentLabel}
        isRunning={isRunning}
        projectPath={session?.project_path}
      />
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
    <div className="space-y-3 px-4 py-4">
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

/* ─── Streaming Spinner with shimmer ─────────────────────────────── */

function StreamingIndicator() {
  const streamingContent = useStreamingStore((s) => s.streamingContent);

  if (streamingContent) {
    // Show streaming text in real time.
    return (
      <div className="mx-4 my-2">
        <div className="rounded-lg border border-border/30 bg-muted/10 px-4 py-3">
          <div className="whitespace-pre-wrap text-body text-foreground/90">
            {streamingContent}
            <span className="ml-0.5 inline-block h-4 w-0.5 animate-pulse bg-foreground/60" />
          </div>
        </div>
      </div>
    );
  }

  // No text yet — show thinking spinner.
  return (
    <div className="mx-4 my-2">
      <div className="flex items-center gap-3 rounded-lg border border-border/30 bg-muted/10 px-4 py-3">
        <div className="flex items-center gap-1">
          <ShimmerDot delay={0} />
          <ShimmerDot delay={150} />
          <ShimmerDot delay={300} />
        </div>
        <span className="text-body text-muted-foreground">
          Thinking...
        </span>
      </div>
    </div>
  );
}

function ShimmerDot({ delay }: { delay: number }) {
  return (
    <span
      className="inline-block size-1.5 rounded-full animate-pulse"
      style={{
        backgroundColor: "var(--claude-orange)",
        animationDelay: `${delay}ms`,
      }}
    />
  );
}

/* ─── Welcome Screen ─────────────────────────────────────────────── */

function WelcomeScreen({ onShowDemo }: { onShowDemo?: () => void }) {
  const capabilities = [
    {
      icon: Terminal,
      title: "Run Commands",
      desc: "Execute shell commands, scripts, and build tools",
      color: "var(--color-terminal-tool)",
    },
    {
      icon: FileEdit,
      title: "Edit Files",
      desc: "Read, write, and modify code with precise diffs",
      color: "var(--claude-orange)",
    },
    {
      icon: Search,
      title: "Search Code",
      desc: "Find files and patterns across your codebase",
      color: "var(--claude-blue)",
    },
    {
      icon: Globe,
      title: "Web Access",
      desc: "Fetch URLs and search the web for information",
      color: "var(--agent-cyan)",
    },
    {
      icon: Code2,
      title: "Multi-file Edits",
      desc: "Coordinate changes across multiple files at once",
      color: "var(--agent-purple)",
    },
    {
      icon: Zap,
      title: "MCP Tools",
      desc: "Use connected MCP server tools and extensions",
      color: "var(--color-fast-mode)",
    },
  ];

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-5 p-6">
      {/* Logo */}
      <div className="flex flex-col items-center gap-3">
        <div
          className="flex size-14 items-center justify-center rounded-2xl"
          style={{
            background: "linear-gradient(135deg, var(--claude-orange), var(--claude-orange-shimmer))",
          }}
        >
          <MessageSquare className="size-7 text-white" />
        </div>
        <div className="text-center">
          <h2 className="text-base font-semibold text-foreground">
            What can I help you with?
          </h2>
          <p className="mt-1 max-w-sm text-body text-muted-foreground">
            I can read, write, and run code in your project. Just describe what you need.
          </p>
        </div>
      </div>

      {/* Capabilities grid */}
      <div className="grid w-full max-w-lg grid-cols-2 gap-2">
        {capabilities.map((item) => (
          <div
            key={item.title}
            className="flex items-start gap-2.5 rounded-lg border border-border/40 bg-muted/10 p-3 transition-colors hover:bg-muted/20"
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
          <span>to send</span>
          <span className="mx-1">|</span>
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-caption">Shift+Enter</kbd>
          <span>for newline</span>
        </div>
        {onShowDemo && (
          <button
            className="flex items-center gap-1.5 rounded-md border border-border/40 bg-muted/10 px-3 py-1 text-label text-muted-foreground transition-colors hover:bg-muted/30 hover:text-foreground"
            onClick={onShowDemo}
          >
            <Play className="size-3" />
            View demo conversation
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
    for (const block of message.blocks) {
      order += 1;
      const entry = toDisplayMessage(message.role, block, order);
      if (entry) {
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

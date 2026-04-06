import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
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
import { ContentHeader } from "./ContentHeader";
import { VirtualizedMessageList } from "./VirtualizedMessageList";
import { InputBar } from "./InputBar";
import { StatusLine } from "./StatusLine";
import {
  PermissionDialog,
  type PermissionAction,
} from "./PermissionDialog";
import { executeCommand, type CommandContext } from "./commandExecutor";
import { SubagentPanel, extractSubagents } from "./SubagentPanel";
import { exportAsMarkdown, exportAsJson } from "./sessionExport";
import { useKeyboardShortcuts } from "./useKeyboardShortcuts";
import { useAppDispatch, useAppSelector } from "@/store";
import {
  setShowSessionSidebar,
} from "@/store/slices/settings";
import { usePermissionsStore } from "@/state/permissions-store";
import {
  forwardPermissionDecision,
  type ContentBlock,
  type DesktopSessionDetail,
  type RuntimeConversationMessage,
} from "@/lib/tauri";
import type { ConversationMessage } from "./types";
import { MOCK_DEMO_MESSAGES } from "./mockDemoMessages";

interface SessionWorkbenchTerminalProps {
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

export function SessionWorkbenchTerminal({
  session,
  isLoadingSession,
  isSending,
  errorMessage,
  onSend,
  onStop,
  onCreateSession,
  modelLabel = "Opus 4.6",
  environmentLabel = "Local",
  projectPath,
}: SessionWorkbenchTerminalProps) {
  const dispatch = useAppDispatch();
  const navigate = useNavigate();
  const pendingPermission = usePermissionsStore(
    (state) => state.pendingRequest
  );
  const setPendingPermission = usePermissionsStore(
    (state) => state.setPendingPermission
  );
  const resolvePermission = usePermissionsStore(
    (state) => state.resolvePermission
  );
  const permissionMode = useAppSelector((s) => s.settings.permissionMode);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollNode, setScrollNode] = useState<HTMLDivElement | null>(null);
  const scrollCallbackRef = useCallback((node: HTMLDivElement | null) => {
    scrollRef.current = node;
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

  // Auto-scroll is now handled inside VirtualizedMessageList

  const handlePermissionDecision = useCallback(
    (action: PermissionAction) => {
      if (pendingPermission) {
        resolvePermission({
          requestId: pendingPermission.id,
          decision: action,
        });
        // Forward decision to Tauri backend
        if (session?.id) {
          void forwardPermissionDecision(session.id, {
            requestId: pendingPermission.id,
            decision: action,
          }).catch(() => {
            // Backend may not be ready yet — decision is still stored locally
          });
        }
      }
    },
    [pendingPermission, resolvePermission, session?.id]
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

  const handleSlashCommand = useCallback(
    (input: string): boolean => {
      const context: CommandContext = {
        dispatch,
        messages,
        permissionMode,
        modelLabel,
        sessionId: session?.id,
        onSendAsPrompt: (prompt) => void onSend(prompt),
        onInjectSystemMessage: addSystemMessage,
        onClearMessages: () => setLocalMessages([]),
        onNavigate: (section) => navigate(`/${section}`),
      };

      const result = executeCommand(input, context);
      if (!result) return false;

      switch (result.type) {
        case "system_message":
          if (result.message) addSystemMessage(result.message);
          break;
        case "navigate":
          if (result.message) addSystemMessage(result.message);
          if (result.navigateTo) navigate(`/${result.navigateTo}`);
          break;
        case "clear":
          setLocalMessages([]);
          break;
        case "noop":
          break;
      }

      return true;
    },
    [dispatch, messages, permissionMode, modelLabel, session, onSend, navigate, addSystemMessage]
  );

  // Export handlers
  const handleExportMarkdown = useCallback(() => {
    if (displayMessages.length === 0) return;
    exportAsMarkdown(displayMessages, session?.title, projectPath);
  }, [displayMessages, session?.title, projectPath]);

  const handleExportJson = useCallback(() => {
    if (displayMessages.length === 0) return;
    exportAsJson(displayMessages, session?.title, projectPath);
  }, [displayMessages, session?.title, projectPath]);

  // Input ref for focus shortcut
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Keyboard shortcuts
  const showSidebar = useAppSelector((s) => s.settings.showSessionSidebar);
  useKeyboardShortcuts({
    onEscape: useCallback(() => {
      if (pendingPermission) return; // don't interfere with permission dialog
      if (isRunning && onStop) onStop();
    }, [isRunning, onStop, pendingPermission]),
    onClearMessages: useCallback(() => {
      setLocalMessages([]);
      addSystemMessage("Messages cleared.");
    }, [addSystemMessage]),
    onNewSession: onCreateSession,
    onFocusInput: useCallback(() => {
      inputRef.current?.focus();
    }, []),
    onOpenSettings: useCallback(() => {
      navigate("/settings");
    }, [navigate]),
    onToggleSidebar: useCallback(() => {
      dispatch(setShowSessionSidebar(!showSidebar));
    }, [dispatch, showSidebar]),
    onExportSession: handleExportMarkdown,
    onToggleAgentPanel: useCallback(() => {
      setShowAgentPanel((v) => !v);
    }, []),
  });

  return (
    <div className="flex flex-1 overflow-hidden">
     <div className="flex flex-1 flex-col overflow-hidden">
      <ContentHeader
        projectPath={projectPath}
        modelLabel={modelLabel}
        environmentLabel={environmentLabel}
        isStreaming={isRunning}
        agentCount={agentCount}
        showAgentPanel={showAgentPanel}
        onToggleAgentPanel={() => setShowAgentPanel((v) => !v)}
        onExportMarkdown={displayMessages.length > 0 ? handleExportMarkdown : undefined}
        onExportJson={displayMessages.length > 0 ? handleExportJson : undefined}
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
            <VirtualizedMessageList
              messages={displayMessages}
              scrollElement={scrollNode}
            />

            {/* Fixed items after virtual list */}
            {pendingPermission && (
              <PermissionDialog
                request={pendingPermission}
                onDecision={handlePermissionDecision}
              />
            )}
            {isRunning && !pendingPermission && <StreamingSpinner />}
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

      <InputBar
        onSend={onSend}
        onStop={onStop}
        onSlashCommand={handleSlashCommand}
        isBusy={isRunning || !!pendingPermission}
        environmentLabel={environmentLabel}
        inputRef={inputRef}
      />

      <StatusLine
        modelLabel={modelLabel}
        environmentLabel={environmentLabel}
        isRunning={isRunning}
      />
     </div>

      {/* Subagent side panel */}
      {showAgentPanel && (
        <SubagentPanel
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

function StreamingSpinner() {
  return (
    <div className="mx-4 my-2">
      <div className="flex items-center gap-3 rounded-lg border border-border/30 bg-muted/10 px-4 py-3">
        {/* Shimmer dots */}
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
          <span>Type</span>
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-caption">/</kbd>
          <span>for commands</span>
          <span className="mx-1">|</span>
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-caption">Enter</kbd>
          <span>to send</span>
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
      toolName: block.tool_name,
      output: block.output,
      isError: block.is_error,
    },
  };
}

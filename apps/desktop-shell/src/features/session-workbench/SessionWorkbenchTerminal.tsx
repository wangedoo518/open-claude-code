import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Loader2,
  Terminal,
  FileEdit,
  Search,
  Globe,
  Code2,
  Zap,
  MessageSquare,
  Play,
} from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ContentHeader } from "./ContentHeader";
import { MessageItem } from "./MessageItem";
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
  resolvePermission,
  setPendingPermission,
} from "@/store/slices/permissions";
import {
  setShowSessionSidebar,
} from "@/store/slices/settings";
import type {
  ContentBlock,
  DesktopSessionDetail,
  RuntimeConversationMessage,
} from "@/lib/tauri";
import type { ConversationMessage } from "@/store/slices/sessions";
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
  const pendingPermission = useAppSelector(
    (s) => s.permissions.pendingRequest
  );
  const permissionMode = useAppSelector((s) => s.settings.permissionMode);
  const scrollRef = useRef<HTMLDivElement>(null);
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

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [displayMessages, isRunning, pendingPermission]);

  const handlePermissionDecision = useCallback(
    (action: PermissionAction) => {
      if (pendingPermission) {
        dispatch(
          resolvePermission({
            requestId: pendingPermission.id,
            decision: action,
          })
        );
        // TODO: forward decision to Tauri backend when protocol is available
      }
    },
    [dispatch, pendingPermission]
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
        <ScrollArea className="flex-1">
          <div ref={scrollRef} className="pb-4">
            {showDemo && (
              <div className="mx-4 mb-2 mt-1 flex items-center justify-between rounded-lg border border-border/30 bg-muted/10 px-3 py-1.5">
                <span className="text-[11px] text-muted-foreground">
                  Demo mode — showing sample conversation
                </span>
                <div className="flex items-center gap-2">
                  {!pendingPermission && (
                    <button
                      className="text-[11px] font-medium text-muted-foreground hover:text-foreground hover:underline"
                      onClick={() =>
                        dispatch(
                          setPendingPermission({
                            id: `demo-perm-${Date.now()}`,
                            toolName: "Bash",
                            toolInput: {
                              command: "npm install @radix-ui/react-dialog",
                            },
                            riskLevel: "high",
                          })
                        )
                      }
                    >
                      Test permission
                    </button>
                  )}
                  <button
                    className="text-[11px] font-medium text-foreground hover:underline"
                    onClick={() => setShowDemo(false)}
                  >
                    Exit demo
                  </button>
                </div>
              </div>
            )}
            {isLoadingSession && (
              <div className="flex items-center gap-2 px-4 py-4 text-[13px] text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
                <span>Loading session...</span>
              </div>
            )}
            {displayMessages.map((msg) => (
              <MessageItem key={msg.id} message={msg} />
            ))}
            {/* Permission dialog — renders inline at end of messages */}
            {pendingPermission && (
              <PermissionDialog
                request={pendingPermission}
                onDecision={handlePermissionDecision}
              />
            )}
            {isRunning && !pendingPermission && <StreamingSpinner />}
            {errorMessage && (
              <div
                className="mx-4 mt-3 rounded-lg border px-3 py-2 text-[12px]"
                style={{
                  borderColor: "color-mix(in srgb, var(--color-error, rgb(171,43,63)) 30%, transparent)",
                  backgroundColor: "color-mix(in srgb, var(--color-error, rgb(171,43,63)) 5%, transparent)",
                  color: "var(--color-error, rgb(171,43,63))",
                }}
              >
                {errorMessage}
              </div>
            )}
          </div>
        </ScrollArea>
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
        <span className="text-[13px] text-muted-foreground">
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
        backgroundColor: "var(--claude-orange, rgb(215,119,87))",
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
      color: "var(--color-terminal-tool, rgb(44,122,57))",
    },
    {
      icon: FileEdit,
      title: "Edit Files",
      desc: "Read, write, and modify code with precise diffs",
      color: "var(--claude-orange, rgb(215,119,87))",
    },
    {
      icon: Search,
      title: "Search Code",
      desc: "Find files and patterns across your codebase",
      color: "var(--claude-blue, rgb(87,105,247))",
    },
    {
      icon: Globe,
      title: "Web Access",
      desc: "Fetch URLs and search the web for information",
      color: "var(--agent-cyan, rgb(8,145,178))",
    },
    {
      icon: Code2,
      title: "Multi-file Edits",
      desc: "Coordinate changes across multiple files at once",
      color: "var(--agent-purple, rgb(147,51,234))",
    },
    {
      icon: Zap,
      title: "MCP Tools",
      desc: "Use connected MCP server tools and extensions",
      color: "var(--color-fast-mode, rgb(255,106,0))",
    },
  ];

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-5 p-6">
      {/* Logo */}
      <div className="flex flex-col items-center gap-3">
        <div
          className="flex size-14 items-center justify-center rounded-2xl"
          style={{
            background: "linear-gradient(135deg, var(--claude-orange, rgb(215,119,87)), var(--claude-orange-shimmer, rgb(245,149,117)))",
          }}
        >
          <MessageSquare className="size-7 text-white" />
        </div>
        <div className="text-center">
          <h2 className="text-base font-semibold text-foreground">
            What can I help you with?
          </h2>
          <p className="mt-1 max-w-sm text-[13px] text-muted-foreground">
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
              <div className="text-[12px] font-medium text-foreground">{item.title}</div>
              <div className="text-[11px] leading-snug text-muted-foreground">{item.desc}</div>
            </div>
          </div>
        ))}
      </div>

      {/* Hint + Demo */}
      <div className="flex flex-col items-center gap-2">
        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground/60">
          <span>Type</span>
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-[10px]">/</kbd>
          <span>for commands</span>
          <span className="mx-1">|</span>
          <kbd className="rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 font-mono text-[10px]">Enter</kbd>
          <span>to send</span>
        </div>
        {onShowDemo && (
          <button
            className="flex items-center gap-1.5 rounded-md border border-border/40 bg-muted/10 px-3 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-muted/30 hover:text-foreground"
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

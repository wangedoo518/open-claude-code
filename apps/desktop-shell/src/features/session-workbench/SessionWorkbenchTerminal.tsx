import { useEffect, useMemo, useRef } from "react";
import { Loader2, Terminal, Sparkles } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ContentHeader } from "./ContentHeader";
import { MessageItem } from "./MessageItem";
import { InputBar } from "./InputBar";
import type {
  ContentBlock,
  DesktopSessionDetail,
  RuntimeConversationMessage,
} from "@/lib/tauri";
import type { ConversationMessage } from "@/store/slices/sessions";

interface SessionWorkbenchTerminalProps {
  session: DesktopSessionDetail | null;
  isLoadingSession: boolean;
  isSending: boolean;
  errorMessage?: string;
  onSend: (message: string) => void | Promise<void>;
  onStop?: () => void;
  /** Model label for ContentHeader + InputBar */
  modelLabel?: string;
  /** Permission mode label for InputBar */
  permissionModeLabel?: string;
  /** Environment label for ContentHeader + InputBar */
  environmentLabel?: string;
  /** Project path for ContentHeader */
  projectPath?: string;
}

export function SessionWorkbenchTerminal({
  session,
  isLoadingSession,
  isSending,
  errorMessage,
  onSend,
  onStop,
  modelLabel = "Opus 4.6",
  permissionModeLabel = "Ask permissions",
  environmentLabel = "Local",
  projectPath,
}: SessionWorkbenchTerminalProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const messages = useMemo(
    () => flattenSessionMessages(session?.session.messages ?? []),
    [session?.session.messages]
  );
  const isRunning = session?.turn_state === "running" || isSending;

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isRunning]);

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* ContentHeader — transparent, merges with content */}
      <ContentHeader
        projectPath={projectPath}
        modelLabel={modelLabel}
        environmentLabel={environmentLabel}
        isStreaming={isRunning}
      />

      {messages.length === 0 && !isLoadingSession ? (
          <SessionWorkbenchWelcomeScreen />
      ) : (
        <ScrollArea className="flex-1">
          <div ref={scrollRef} className="pb-4">
            {isLoadingSession && (
              <div className="flex items-center gap-2 px-4 py-4 text-sm text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
                <span>Loading session...</span>
              </div>
            )}
            {messages.map((msg) => (
              <MessageItem key={msg.id} message={msg} />
            ))}
            {isRunning && (
              <div className="flex items-center gap-2 px-4 py-3 text-sm text-muted-foreground">
                <div className="flex gap-1">
                  <span className="animate-bounce delay-0">.</span>
                  <span className="animate-bounce delay-150">.</span>
                  <span className="animate-bounce delay-300">.</span>
                </div>
                <span>Thinking</span>
              </div>
            )}
            {errorMessage && (
              <div className="mx-4 mt-3 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                {errorMessage}
              </div>
            )}
          </div>
        </ScrollArea>
      )}

      <InputBar
        onSend={onSend}
        onStop={onStop}
        isBusy={isRunning}
        permissionModeLabel={permissionModeLabel}
        environmentLabel={environmentLabel}
      />
    </div>
  );
}

function SessionWorkbenchWelcomeScreen() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 p-8">
      <div className="flex size-16 items-center justify-center rounded-2xl bg-primary/10">
        <Sparkles className="size-8 text-primary" />
      </div>
      <div className="text-center">
        <h2 className="mb-2 text-lg font-semibold text-foreground">
          Warwolf Code
        </h2>
        <p className="max-w-md text-sm text-muted-foreground">
          Claude Code style desktop workspace powered by the Rust runtime.
          Start a session or just type below to let the desktop client create
          one for you.
        </p>
      </div>
      <div className="grid max-w-lg grid-cols-2 gap-3">
        {[
          {
            icon: Terminal,
            title: "Run Commands",
            desc: "Execute shell commands and scripts",
          },
          {
            icon: Terminal,
            title: "Edit Files",
            desc: "Read, write, and modify code files",
          },
          {
            icon: Terminal,
            title: "Search Code",
            desc: "Find files and patterns in your codebase",
          },
          {
            icon: Terminal,
            title: "MCP Tools",
            desc: "Use connected MCP server tools",
          },
        ].map((item) => (
          <div
            key={item.title}
            className="rounded-lg border border-border/50 bg-muted/20 p-3"
          >
            <item.icon className="mb-2 size-4 text-muted-foreground" />
            <div className="text-xs font-medium">{item.title}</div>
            <div className="text-[10px] text-muted-foreground">
              {item.desc}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

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

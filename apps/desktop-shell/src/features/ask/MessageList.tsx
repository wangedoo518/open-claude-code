/**
 * MessageList — virtualized message list using @tanstack/react-virtual.
 * Groups consecutive tool_use/tool_result into ToolActionsGroup.
 * Appends StreamingMessage when turn is active.
 * Auto-scrolls to bottom when user was at bottom.
 */

import { memo, useMemo, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Message } from "./Message";
import { ToolActionsGroup } from "./ToolActionsGroup";
import { StreamingMessage } from "./StreamingMessage";
import { useStickToBottomContext } from "./ConversationScroller";
import type { ConversationMessage } from "@/features/common/message-types";
import type { SourceRef } from "@/lib/tauri";
import type { ConversationTurnStatus } from "./useConversationTurnState";

interface MessageListProps {
  sessionKey?: string;
  messages: ConversationMessage[];
  streamingContent?: string;
  /**
   * A5 — forward-compatible thinking payload. When the backend emits
   * a `thinking_delta` event (not yet implemented), useAskSSE routes
   * it through `streaming-store::appendStreamingThinking` and the
   * value lands here; StreamingMessage will render a collapsible
   * ThinkingBlock. Today this is always "" and the Ask UI only shows
   * the phased shimmer before the first text_delta arrives.
   */
  streamingThinking?: string;
  isStreaming?: boolean;
  /**
   * True while the outgoing POST has started but the backend snapshot may
   * not have appended the new user turn yet. In that short window the
   * previous assistant message can still be the tail, so we keep showing
   * the working indicator instead of treating the old assistant tail as a
   * completed handoff.
   */
  isStartingTurn?: boolean;
  turnStatus?: ConversationTurnStatus;
  /**
   * A3 — forwarded to <Message> → <UsedSourcesBar> so the inline
   * "固定到会话" action can upgrade an auto-bound turn source
   * into a persistent session binding.
   */
  onPromoteToSession?: (source: SourceRef) => void | Promise<void>;
}

interface SingleGroup {
  kind: "single";
  message: ConversationMessage;
  key: string;
}

interface ToolGroupItem {
  kind: "tool-group";
  messages: ConversationMessage[];
  key: string;
}

interface StreamingItem {
  kind: "streaming";
  content: string;
  thinking: string;
  key: string;
}

type RenderItem = SingleGroup | ToolGroupItem | StreamingItem;

function buildItems(
  messages: ConversationMessage[],
  streamingContent: string | undefined,
  streamingThinking: string | undefined,
  isStreaming: boolean,
  isStartingTurn: boolean,
): RenderItem[] {
  const items: RenderItem[] = [];
  let toolBuf: ConversationMessage[] = [];
  const lastMessage = messages[messages.length - 1];
  const terminalAssistantMessage =
    !isStartingTurn &&
    isStreaming &&
    lastMessage?.role === "assistant" &&
    lastMessage.type === "text"
      ? lastMessage
      : undefined;
  // SSE can deliver the completed assistant message before the session
  // flips to idle. Keep rendering that tail through StreamingMessage so
  // the transcript settles instead of snapping from stream -> final.
  const messagesToRender = terminalAssistantMessage
    ? messages.slice(0, -1)
    : messages;

  const flushToolBuf = () => {
    if (toolBuf.length > 0) {
      items.push({
        kind: "tool-group",
        messages: [...toolBuf],
        key: `tg-${toolBuf[0].id}`,
      });
      toolBuf = [];
    }
  };

  for (const msg of messagesToRender) {
    if (msg.type === "tool_use" || msg.type === "tool_result") {
      toolBuf.push(msg);
    } else {
      flushToolBuf();
      items.push({ kind: "single", message: msg, key: msg.id });
    }
  }
  flushToolBuf();

  const hasTerminalAssistantTail =
    lastMessage?.role === "assistant" && lastMessage.type === "text";

  if (
    isStreaming &&
    (isStartingTurn || !hasTerminalAssistantTail || terminalAssistantMessage)
  ) {
    items.push({
      kind: "streaming",
      content: terminalAssistantMessage?.content ?? streamingContent ?? "",
      thinking: streamingThinking ?? "",
      key: "streaming",
    });
  }

  return items;
}

export const MessageList = memo(function MessageList({
  sessionKey,
  messages,
  streamingContent,
  streamingThinking,
  isStreaming = false,
  isStartingTurn = false,
  turnStatus,
  onPromoteToSession,
}: MessageListProps) {
  const { scrollElement, isAtBottom } = useStickToBottomContext();
  const scrollRafRef = useRef<number | null>(null);
  const items = useMemo(
    () =>
      buildItems(
        messages,
        streamingContent,
        streamingThinking,
        isStreaming,
        isStartingTurn,
      ),
    [messages, streamingContent, streamingThinking, isStreaming, isStartingTurn],
  );

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollElement,
    getItemKey: (index) => items[index]?.key ?? index,
    estimateSize: () => 96,
    overscan: 5,
  });

  // When the user switches to a different session, clear any stale
  // height cache and reset the scroll position so a previous thread's
  // large measurements don't leave giant blank space or hide the new
  // conversation above/below the viewport.
  useEffect(() => {
    virtualizer.measure();
    if (scrollElement) {
      scrollElement.scrollTo({ top: 0, behavior: "auto" });
    }
  }, [sessionKey, scrollElement, virtualizer]);

  // Auto-scroll to bottom when new items arrive or streaming token appended.
  // NOTE: `streamingContent` must be in deps — during streaming `items.length`
  // is stable (the trailing streaming item exists for the whole turn) and only
  // the content grows. Without this dep the list would not follow the token
  // tail. `isStreaming` is also included for symmetry when the streaming item
  // is first appended / removed.
  useEffect(() => {
    if (items.length === 0 || !isAtBottom) return;
    if (scrollRafRef.current !== null) {
      cancelAnimationFrame(scrollRafRef.current);
    }
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      virtualizer.scrollToIndex(items.length - 1, { align: "end" });
    });
    return () => {
      if (scrollRafRef.current !== null) {
        cancelAnimationFrame(scrollRafRef.current);
        scrollRafRef.current = null;
      }
    };
  }, [
    items.length,
    streamingContent?.length,
    streamingThinking?.length,
    isStreaming,
    virtualizer,
    isAtBottom,
  ]);

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();

  return (
    <div
      style={{
        height: `${totalSize}px`,
        width: "100%",
        position: "relative",
      }}
    >
      {virtualItems.map((virtualItem) => {
        const item = items[virtualItem.index];
        if (!item) return null;

        return (
          <div
            key={item.key}
            className="ask-message-row"
            data-index={virtualItem.index}
            data-kind={item.kind}
            data-tail={virtualItem.index === items.length - 1 ? "true" : "false"}
            ref={virtualizer.measureElement}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              transform: `translateY(${virtualItem.start}px)`,
              padding: "6px clamp(14px, 4vw, 48px)",
              overflow: "hidden",
            }}
          >
            {item.kind === "tool-group" ? (
              <ToolActionsGroup
                messages={item.messages}
                isStreaming={isStreaming && virtualItem.index === items.length - 2}
              />
            ) : item.kind === "streaming" ? (
              <StreamingMessage
                content={item.content}
                thinkingContent={item.thinking || undefined}
                turnStatus={turnStatus}
              />
            ) : (
              <Message
                message={item.message}
                onPromoteToSession={onPromoteToSession}
              />
            )}
          </div>
        );
      })}
    </div>
  );
});

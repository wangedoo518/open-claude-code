/**
 * MessageList — virtualized message list using @tanstack/react-virtual.
 * Groups consecutive tool_use/tool_result into ToolActionsGroup.
 * Appends StreamingMessage when turn is active.
 * Auto-scrolls to bottom when user was at bottom.
 */

import { memo, useMemo, useEffect } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Message } from "./Message";
import { ToolActionsGroup } from "./ToolActionsGroup";
import { StreamingMessage } from "./StreamingMessage";
import { useStickToBottomContext } from "./ConversationScroller";
import type { ConversationMessage } from "@/features/common/message-types";
import type { SourceRef } from "@/lib/tauri";

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
): RenderItem[] {
  const items: RenderItem[] = [];
  let toolBuf: ConversationMessage[] = [];

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

  for (const msg of messages) {
    if (msg.type === "tool_use" || msg.type === "tool_result") {
      toolBuf.push(msg);
    } else {
      flushToolBuf();
      items.push({ kind: "single", message: msg, key: msg.id });
    }
  }
  flushToolBuf();

  if (isStreaming) {
    items.push({
      kind: "streaming",
      content: streamingContent ?? "",
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
  onPromoteToSession,
}: MessageListProps) {
  const { scrollElement, isAtBottom } = useStickToBottomContext();
  const items = useMemo(
    () =>
      buildItems(messages, streamingContent, streamingThinking, isStreaming),
    [messages, streamingContent, streamingThinking, isStreaming],
  );

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollElement,
    getItemKey: (index) => items[index]?.key ?? index,
    estimateSize: () => 80,
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
    if (items.length > 0 && isAtBottom) {
      virtualizer.scrollToIndex(items.length - 1, { align: "end" });
    }
  }, [
    items.length,
    streamingContent,
    streamingThinking,
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
        return (
          <div
            key={item.key}
            data-index={virtualItem.index}
            ref={virtualizer.measureElement}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              transform: `translateY(${virtualItem.start}px)`,
              padding: "4px 16px",
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

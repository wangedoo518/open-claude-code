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

interface MessageListProps {
  messages: ConversationMessage[];
  streamingContent?: string;
  isStreaming?: boolean;
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
  key: string;
}

type RenderItem = SingleGroup | ToolGroupItem | StreamingItem;

function buildItems(
  messages: ConversationMessage[],
  streamingContent: string | undefined,
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
    items.push({ kind: "streaming", content: streamingContent ?? "", key: "streaming" });
  }

  return items;
}

export const MessageList = memo(function MessageList({
  messages,
  streamingContent,
  isStreaming = false,
}: MessageListProps) {
  const { scrollElement, isAtBottom } = useStickToBottomContext();
  const items = useMemo(
    () => buildItems(messages, streamingContent, isStreaming),
    [messages, streamingContent, isStreaming],
  );

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollElement,
    estimateSize: () => 80,
    overscan: 5,
  });

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
  }, [items.length, streamingContent, isStreaming, virtualizer, isAtBottom]);

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
              <StreamingMessage content={item.content} />
            ) : (
              <Message message={item.message} />
            )}
          </div>
        );
      })}
    </div>
  );
});

import { memo, useEffect, useRef, useCallback } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { MessageItem } from "./MessageItem";
import type { ConversationMessage } from "./types";

interface VirtualizedMessageListProps {
  messages: ConversationMessage[];
  scrollElement: HTMLDivElement | null;
}

export const VirtualizedMessageList = memo(function VirtualizedMessageList({
  messages,
  scrollElement,
}: VirtualizedMessageListProps) {
  const isAtBottomRef = useRef(true);

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollElement,
    estimateSize: () => 80,
    overscan: 5,
  });

  // Track scroll position to decide auto-scroll
  const checkScrollPosition = useCallback(() => {
    if (!scrollElement) return;
    const threshold = 100;
    isAtBottomRef.current =
      scrollElement.scrollHeight - scrollElement.scrollTop - scrollElement.clientHeight < threshold;
  }, [scrollElement]);

  useEffect(() => {
    if (!scrollElement) return;
    scrollElement.addEventListener("scroll", checkScrollPosition, { passive: true });
    return () => scrollElement.removeEventListener("scroll", checkScrollPosition);
  }, [scrollElement, checkScrollPosition]);

  // Auto-scroll to bottom when new messages arrive (if user was at bottom)
  useEffect(() => {
    if (messages.length > 0 && isAtBottomRef.current) {
      virtualizer.scrollToIndex(messages.length - 1, { align: "end" });
    }
  }, [messages.length, virtualizer]);

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
        const message = messages[virtualItem.index];
        return (
          <div
            key={message.id}
            data-index={virtualItem.index}
            ref={virtualizer.measureElement}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              transform: `translateY(${virtualItem.start}px)`,
            }}
          >
            <MessageItem message={message} />
          </div>
        );
      })}
    </div>
  );
});

/**
 * Floating scroll-to-bottom button — appears when user scrolls up in the
 * conversation, disappears when at bottom.  Reads state from the
 * ConversationScroller's StickToBottom context.
 */

import { useCallback } from "react";
import { ChevronDown } from "lucide-react";
import { useStickToBottomContext } from "./ConversationScroller";

export function ScrollToBottomButton() {
  const { isAtBottom, scrollToBottom } = useStickToBottomContext();

  const handleClick = useCallback(() => {
    scrollToBottom();
  }, [scrollToBottom]);

  if (isAtBottom) return null;

  return (
    <button
      type="button"
      className="absolute bottom-6 right-3 z-10 inline-flex h-9 items-center gap-1.5 rounded-full border border-border bg-card/95 px-3 text-[11px] font-medium text-muted-foreground shadow-[var(--deeptutor-shadow-md,0_4px_12px_-2px_rgba(0,0,0,0.1))] backdrop-blur transition-all hover:bg-accent hover:text-foreground"
      onClick={handleClick}
      aria-label="滚动到对话底部"
    >
      <ChevronDown className="size-4" />
      <span>到底部</span>
    </button>
  );
}

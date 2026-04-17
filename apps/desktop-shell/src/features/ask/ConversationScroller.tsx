/**
 * Conversation scroll container — provides scroll element ref for
 * virtualized message list + auto-scroll context.
 */

import {
  type ReactNode,
  useRef,
  useEffect,
  useCallback,
  createContext,
  useContext,
  useState,
} from "react";

interface ScrollCtx {
  isAtBottom: boolean;
  scrollToBottom: () => void;
  scrollElement: HTMLDivElement | null;
}

const ScrollContext = createContext<ScrollCtx>({
  isAtBottom: true,
  scrollToBottom: () => {},
  scrollElement: null,
});

export function useStickToBottomContext() {
  return useContext(ScrollContext);
}

export function ConversationScroller({ children }: { children: ReactNode }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);

  useEffect(() => {
    setScrollElement(scrollRef.current);
  }, []);

  const checkBottom = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    setIsAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 120);
  }, []);

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, []);

  // NOTE: the previous dependency-less useEffect that forced scrollTop on every
  // render has been removed. It raced with MessageList's own
  // virtualizer.scrollToIndex (see MessageList.tsx), causing the scroll position
  // to jitter during streaming. MessageList now owns auto-scroll and uses
  // `isAtBottom` from this context to decide whether to stick.

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.addEventListener("scroll", checkBottom, { passive: true });
    return () => el.removeEventListener("scroll", checkBottom);
  }, [checkBottom]);

  return (
    <ScrollContext.Provider value={{ isAtBottom, scrollToBottom, scrollElement }}>
      <div
        ref={scrollRef}
        className="relative flex-1 overflow-y-auto"
        style={{ minHeight: 0 }}
      >
        {children}
      </div>
    </ScrollContext.Provider>
  );
}

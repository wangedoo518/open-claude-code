/**
 * Slice E19.2 — Cross-component plumbing for the right-edge
 * conversation navigator.
 *
 * MessageList owns the @tanstack/react-virtual instance, so any
 * scroll-to-message must go through `virtualizer.scrollToIndex`.
 * Navigator and MessageList are siblings — they coordinate via this
 * context.
 *
 * Design notes:
 *   - Single context, but the value is built from a stable ref so
 *     consumers (notably MessageList) don't re-render every time the
 *     navigator highlights a new active dot. Active-dot tracking is
 *     navigator-local: it reads `data-message-index` off the rendered
 *     rows directly, no shared state needed.
 *   - `registerScroller` returns a teardown so MessageList can
 *     deregister on unmount or on `items` change.
 */

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  type ReactNode,
} from "react";

export type ScrollToMessageHandler = (messageId: string) => void;

interface ConversationNavigatorApi {
  scrollToMessageId: ScrollToMessageHandler;
  registerScroller: (handler: ScrollToMessageHandler | null) => void;
}

const Ctx = createContext<ConversationNavigatorApi | null>(null);

export function ConversationNavigatorProvider({
  children,
}: {
  children: ReactNode;
}) {
  const handlerRef = useRef<ScrollToMessageHandler | null>(null);

  const scrollToMessageId = useCallback<ScrollToMessageHandler>((id) => {
    handlerRef.current?.(id);
  }, []);

  const registerScroller = useCallback(
    (handler: ScrollToMessageHandler | null) => {
      handlerRef.current = handler;
    },
    [],
  );

  // Memoised so identity is stable across renders — MessageList's
  // useEffect dep list won't fire on every parent rerender.
  const value = useMemo<ConversationNavigatorApi>(
    () => ({ scrollToMessageId, registerScroller }),
    [scrollToMessageId, registerScroller],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/**
 * Optional consumer — returns `null` when no provider is mounted so
 * MessageList can render in non-Ask contexts (e.g. inbox previews)
 * without exploding.
 */
export function useConversationNavigator(): ConversationNavigatorApi | null {
  return useContext(Ctx);
}

import { useEffect, useMemo, useRef, useState } from "react";
import {
  extractNavigatorTurns,
  findActiveTurnIndex,
  type NavigatorTurn,
} from "./conversation-navigator";
import { useConversationNavigator } from "./ConversationNavigatorContext";
import type { ConversationMessage } from "@/features/common/message-types";

/**
 * Slice E19.3 — ConversationNavigator.
 *
 * Right-edge dot timeline for the Ask chat, mirroring Gemini's
 * navigator. One dot per user-text message; click jumps the
 * conversation to that turn; the dot under the topmost-visible row
 * lights up.
 *
 * Layout: thin (24px) absolute-positioned column, vertically centred
 * inside the conversation viewport. Sits OUTSIDE the scroll element
 * (parent must be `position: relative`) so dots stay put while
 * messages scroll under them. The active dot uses `--color-primary`
 * (terracotta); idle dots are muted-foreground at low alpha.
 *
 * Scroll-spy strategy: listens to scroll events on
 * `.ask-conversation-scroll`, queries `[data-message-index]` rows
 * inside it, picks the row whose top edge is just above the viewport
 * top, then maps its message-index back to a turn via
 * `findActiveTurnIndex`. No shared state with MessageList — keeps the
 * virtualizer free to do its own thing.
 *
 * Strict no-input rule: navigator only displays + dispatches scrolls.
 * It never mutates session state or message content.
 */

export interface ConversationNavigatorProps {
  messages: ReadonlyArray<ConversationMessage>;
}

const SCROLL_CONTAINER_SELECTOR = ".ask-conversation-scroll";
// Hide the rail entirely until the user has at least 2 turns — a
// single dot adds noise without adding navigation value.
const MIN_TURNS_TO_SHOW = 2;

export function ConversationNavigator({ messages }: ConversationNavigatorProps) {
  const turns = useMemo(() => extractNavigatorTurns(messages), [messages]);
  const navApi = useConversationNavigator();
  const [activeTurnIdx, setActiveTurnIdx] = useState<number>(-1);
  const rafRef = useRef<number | null>(null);
  const navRef = useRef<HTMLElement | null>(null);

  // Scroll-spy: find the topmost visible message-row inside the
  // ask-conversation-scroll element and map its data-message-index to
  // an active turn.
  useEffect(() => {
    if (turns.length === 0) return;
    // Scope the scroll-element lookup to OUR own subtree (review N3).
    // `document.querySelector` would pick the first .ask-conversation-
    // scroll in the document, which may belong to a sibling
    // ChatSidePanel when both panels are mounted with content. Walk
    // up to the nearest relative wrapper that contains both us and
    // the scroller, then query down — guaranteed to land on the
    // scroller this navigator is anchored to.
    const root = navRef.current?.parentElement ?? null;
    const scrollEl =
      root?.querySelector<HTMLDivElement>(SCROLL_CONTAINER_SELECTOR) ?? null;
    if (!scrollEl) return;

    const recompute = () => {
      const scrollTop = scrollEl.scrollTop;
      const rows = scrollEl.querySelectorAll<HTMLElement>(
        "[data-message-index]",
      );
      let topVisibleIdx = -1;
      // Pick the LARGEST message-index whose row top is at or above
      // the viewport top (with 8px jitter slack). Defensive
      // `Math.max` instead of break-on-first-overflow (review N1) so
      // the spy still works if virtualizer ever renders rows out of
      // DOM-document order.
      for (const row of rows) {
        const rowTop = row.offsetTop;
        if (rowTop > scrollTop + 8) continue;
        const idxAttr = row.getAttribute("data-message-index");
        if (idxAttr === null) continue;
        const parsed = Number.parseInt(idxAttr, 10);
        if (!Number.isFinite(parsed)) continue;
        if (parsed > topVisibleIdx) topVisibleIdx = parsed;
      }
      if (topVisibleIdx < 0) {
        setActiveTurnIdx((prev) => (prev === -1 ? prev : -1));
        return;
      }
      const next = findActiveTurnIndex(turns, topVisibleIdx);
      setActiveTurnIdx((prev) => (prev === next ? prev : next));
    };

    const onScroll = () => {
      if (rafRef.current !== null) return;
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null;
        recompute();
      });
    };

    // Run once on mount so the initial active state matches the
    // current scroll position (e.g. session resume below the top).
    recompute();
    scrollEl.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      scrollEl.removeEventListener("scroll", onScroll);
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [turns]);

  if (turns.length < MIN_TURNS_TO_SHOW || !navApi) return null;

  return (
    <nav
      ref={navRef}
      aria-label="对话锚点"
      className="ask-conversation-navigator pointer-events-none absolute right-1 top-1/2 z-10 flex -translate-y-1/2 flex-col items-center gap-2 py-2"
    >
      <ol className="pointer-events-auto flex max-h-[70vh] flex-col items-center gap-2 overflow-y-auto py-1 pr-0.5">
        {turns.map((turn, idx) => (
          <NavigatorDot
            key={turn.messageId}
            turn={turn}
            index={idx}
            total={turns.length}
            active={idx === activeTurnIdx}
            onJump={() => navApi.scrollToMessageId(turn.messageId)}
          />
        ))}
      </ol>
    </nav>
  );
}

interface NavigatorDotProps {
  turn: NavigatorTurn;
  index: number;
  total: number;
  active: boolean;
  onJump: () => void;
}

function NavigatorDot({ turn, index, total, active, onJump }: NavigatorDotProps) {
  const label = turn.preview
    ? `第 ${index + 1} 轮 · ${turn.preview}`
    : `第 ${index + 1} 轮`;
  return (
    <li>
      <button
        type="button"
        onClick={onJump}
        title={label}
        aria-label={label}
        aria-current={active ? "true" : undefined}
        data-active={active ? "true" : "false"}
        className="ask-nav-dot block rounded-full transition-all"
      >
        <span className="sr-only">
          跳到第 {index + 1} / {total} 轮
        </span>
      </button>
    </li>
  );
}

/**
 * Slice E19 — Conversation navigator.
 *
 * Pure derivations for the right-edge dot timeline that lets users
 * jump between turns in a long Ask chat. Mirrors Gemini's right-side
 * navigator: one dot per user message, tooltip with a content
 * preview, click-to-scroll.
 *
 * Buddy positioning: long sessions accumulate; without a way to skim
 * past turns, you lose the thread. The navigator is a thin overlay
 * (no extra UI noise on short chats) that scales linearly with turn
 * count. Pure functions here so the logic is testable without
 * mounting React.
 */

import type { ConversationMessage } from "@/features/common/message-types";

const PREVIEW_MAX_CHARS = 32;
const PREVIEW_ELLIPSIS_CUT = 30;

export interface NavigatorTurn {
  /** Stable id; matches `ConversationMessage.id` of the user message. */
  messageId: string;
  /** Index in the original flat `messages` array. Used by scroll-spy
   * to map "topmost-visible message id" back to a turn. */
  messageIndex: number;
  /** Wallclock ms; useful for tooltip / future "jump to N min ago" UX. */
  timestamp: number;
  /** First few chars of the user message, single-line, ellipsised.
   * Empty string when the user message had no text content. */
  preview: string;
}

/**
 * Extract one navigator turn per user-text message. Tool results,
 * system messages, and assistant replies are skipped — they belong
 * to the turn that the preceding user message opened.
 */
export function extractNavigatorTurns(
  messages: ReadonlyArray<ConversationMessage>,
): NavigatorTurn[] {
  const turns: NavigatorTurn[] = [];
  for (let i = 0; i < messages.length; i += 1) {
    const m = messages[i];
    if (m.role !== "user" || m.type !== "text") continue;
    turns.push({
      messageId: m.id,
      messageIndex: i,
      timestamp: m.timestamp,
      preview: previewSnippet(m.content),
    });
  }
  return turns;
}

/**
 * Given the message-id of the topmost row currently visible in the
 * scroll viewport, return the index in `turns` of the user message
 * that "owns" that row — i.e. the most recent user message at or
 * before that point in the messages array. Used by the navigator to
 * highlight the active dot as the user scrolls.
 *
 * Returns `-1` when no turn is active yet (visible row precedes any
 * user message — rare, but possible during streaming setup).
 */
export function findActiveTurnIndex(
  turns: ReadonlyArray<NavigatorTurn>,
  topVisibleMessageIndex: number,
): number {
  if (turns.length === 0) return -1;
  // Walk from the last turn backwards — recent turns dominate scroll
  // position in a long convo, so backwards-from-end converges in O(1)
  // amortised for the common "user is reading near the bottom" case.
  for (let i = turns.length - 1; i >= 0; i -= 1) {
    if (turns[i].messageIndex <= topVisibleMessageIndex) return i;
  }
  return -1;
}

function previewSnippet(content: string): string {
  // Collapse all whitespace runs to a single space — multi-line user
  // pastes turn into one readable line for the tooltip.
  const trimmed = content.trim().replace(/\s+/g, " ");
  if (trimmed.length === 0) return "";
  if (trimmed.length <= PREVIEW_MAX_CHARS) return trimmed;
  return `${trimmed.slice(0, PREVIEW_ELLIPSIS_CUT)}…`;
}

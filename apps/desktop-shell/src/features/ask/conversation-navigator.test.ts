/**
 * Slice E19.1 — Conversation navigator pure-function tests.
 *
 * Ambient-vitest contract: type-checks via `tsc --noEmit`, runs
 * verbatim once vitest is wired up. Mirrors `roi-pulse.test.ts`.
 */

import {
  extractNavigatorTurns,
  findActiveTurnIndex,
  type NavigatorTurn,
} from "./conversation-navigator";
import type { ConversationMessage } from "@/features/common/message-types";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
};

function msg(over: Partial<ConversationMessage>): ConversationMessage {
  return {
    id: over.id ?? "x",
    role: over.role ?? "user",
    type: over.type ?? "text",
    content: over.content ?? "",
    timestamp: over.timestamp ?? 0,
    ...over,
  };
}

describe("extractNavigatorTurns", () => {
  it("returns one turn per user-text message in order", () => {
    const turns = extractNavigatorTurns([
      msg({ id: "u1", role: "user", type: "text", content: "first" }),
      msg({ id: "a1", role: "assistant", type: "text", content: "ok" }),
      msg({ id: "u2", role: "user", type: "text", content: "second" }),
      msg({ id: "a2", role: "assistant", type: "text", content: "ok" }),
    ]);
    expect(turns.length).toBe(2);
    expect(turns[0].messageId).toBe("u1");
    expect(turns[0].messageIndex).toBe(0);
    expect(turns[1].messageId).toBe("u2");
    expect(turns[1].messageIndex).toBe(2);
  });

  it("skips tool messages, system messages, and assistant text", () => {
    const turns = extractNavigatorTurns([
      msg({ id: "s1", role: "system", type: "text", content: "sys" }),
      msg({ id: "u1", role: "user", type: "text", content: "ask" }),
      msg({ id: "a1", role: "assistant", type: "tool_use", content: "" }),
      msg({ id: "t1", role: "assistant", type: "tool_result", content: "" }),
      msg({ id: "a2", role: "assistant", type: "text", content: "answer" }),
    ]);
    expect(turns.length).toBe(1);
    expect(turns[0].messageId).toBe("u1");
  });

  it("collapses multi-line user content into a single-line preview", () => {
    const turns = extractNavigatorTurns([
      msg({ role: "user", type: "text", content: "line one\n\nline two" }),
    ]);
    expect(turns[0].preview).toBe("line one line two");
  });

  it("ellipsises previews longer than 32 chars", () => {
    const turns = extractNavigatorTurns([
      msg({
        role: "user",
        type: "text",
        content: "this is a deliberately long user message",
      }),
    ]);
    // 30 chars + ellipsis
    expect(turns[0].preview).toBe("this is a deliberately long u…");
  });

  it("returns [] for an empty message list", () => {
    expect(extractNavigatorTurns([])).toEqual([]);
  });
});

describe("findActiveTurnIndex", () => {
  const turns: NavigatorTurn[] = [
    { messageId: "u1", messageIndex: 0, timestamp: 0, preview: "" },
    { messageId: "u2", messageIndex: 4, timestamp: 0, preview: "" },
    { messageId: "u3", messageIndex: 9, timestamp: 0, preview: "" },
  ];

  it("returns the last turn at or before the visible message index", () => {
    expect(findActiveTurnIndex(turns, 0)).toBe(0);
    expect(findActiveTurnIndex(turns, 3)).toBe(0);
    expect(findActiveTurnIndex(turns, 4)).toBe(1);
    expect(findActiveTurnIndex(turns, 7)).toBe(1);
    expect(findActiveTurnIndex(turns, 9)).toBe(2);
    expect(findActiveTurnIndex(turns, 99)).toBe(2);
  });

  it("returns -1 when no user turn precedes the visible row", () => {
    // Visible row is before the first user message — possible during
    // a session that opens with a system prompt.
    const offset: NavigatorTurn[] = [
      { messageId: "u1", messageIndex: 5, timestamp: 0, preview: "" },
    ];
    expect(findActiveTurnIndex(offset, 0)).toBe(-1);
    expect(findActiveTurnIndex(offset, 4)).toBe(-1);
    expect(findActiveTurnIndex(offset, 5)).toBe(0);
  });

  it("returns -1 for an empty turns list", () => {
    expect(findActiveTurnIndex([], 0)).toBe(-1);
  });
});

import { create } from "zustand";

/**
 * Streaming state — holds only the accumulated text from TextDelta SSE
 * events. The "is the session running?" question is answered by
 * `session.turn_state === "running"` read from React Query cache, NOT
 * by a separate boolean in this store.
 *
 * Rationale: having both `isStreaming` and `session.turn_state` led to
 * drift (e.g., isStreaming flipped true/false per Message event while
 * turn_state stayed Running, causing UI flicker). See audit-lessons L-07.
 *
 * ── Performance note ─────────────────────────────────────────────
 * `appendStreamingContent` batches writes via `requestAnimationFrame`
 * to avoid triggering a Zustand subscriber re-render on every SSE
 * `text_delta` event. At high token rates (100+/s), per-event updates
 * caused noticeable jank. The RAF batch coalesces all chunks received
 * within a single frame (~16.67ms) into one `set` call, capping UI
 * updates at ~60Hz regardless of token arrival rate.
 */
export interface StreamingState {
  /** Accumulated text content from TextDelta SSE events. */
  streamingContent: string;
  /**
   * Accumulated thinking / reasoning summary for the current turn.
   *
   * A5 forward-compatibility: the backend SSE vocabulary today only
   * emits text_delta for the assistant's visible reply. When Worker A
   * adds a `thinking_delta` event (carrying a safe summary — never raw
   * chain-of-thought), `useAskSSE` will route it through
   * `appendStreamingThinking` and `StreamingMessage` will render a
   * collapsible summary via its existing `thinkingContent` prop. Until
   * that lands, the field stays "" and the UI shows only the phased
   * shimmer. We never synthesize fake reasoning text.
   */
  streamingThinking: string;
  /** Whether the session is in Plan Mode (read-only exploration). */
  isPlanMode: boolean;
  /** Append a text chunk to the streaming buffer (batched via RAF). */
  appendStreamingContent: (chunk: string) => void;
  /** Append a thinking-summary chunk (forward-compatible; see above). */
  appendStreamingThinking: (chunk: string) => void;
  /** Clear both streaming buffers without changing other state. */
  clearStreamingContent: () => void;
  /** Toggle plan mode state. */
  setPlanMode: (value: boolean) => void;
}

// ── RAF batching ────────────────────────────────────────────────────
// Chunks received within a single animation frame are accumulated in
// these module-level buffers and flushed once per frame. Content and
// thinking are batched independently so a burst of text_delta doesn't
// delay a thinking_delta (or vice versa) past the next paint.
let pendingContentBuffer = "";
let pendingThinkingBuffer = "";
let rafHandle: number | null = null;

/** Test-only: synchronously flush pending chunks. */
function flushPendingChunks() {
  const contentChunk = pendingContentBuffer;
  const thinkingChunk = pendingThinkingBuffer;
  pendingContentBuffer = "";
  pendingThinkingBuffer = "";
  rafHandle = null;
  if (contentChunk.length === 0 && thinkingChunk.length === 0) return;
  useStreamingStore.setState((state) => ({
    streamingContent: state.streamingContent + contentChunk,
    streamingThinking: state.streamingThinking + thinkingChunk,
  }));
}

/**
 * Schedule a flush on the next animation frame. Safe to call from any
 * context — if RAF isn't available (SSR / test), falls back to setTimeout.
 */
function scheduleFlush() {
  if (rafHandle !== null) return;
  if (typeof requestAnimationFrame === "function") {
    rafHandle = requestAnimationFrame(() => flushPendingChunks());
  } else {
    // Fallback for non-browser environments (tests, SSR).
    rafHandle = setTimeout(flushPendingChunks, 16) as unknown as number;
  }
}

export const useStreamingStore = create<StreamingState>((set) => ({
  streamingContent: "",
  streamingThinking: "",
  isPlanMode: false,
  appendStreamingContent: (chunk) => {
    pendingContentBuffer += chunk;
    scheduleFlush();
  },
  appendStreamingThinking: (chunk) => {
    pendingThinkingBuffer += chunk;
    scheduleFlush();
  },
  clearStreamingContent: () => {
    // Clearing must be synchronous so an incoming message arrival
    // immediately blanks the streaming buffer (prevents the last
    // chunk from appearing as a ghost after the complete message).
    pendingContentBuffer = "";
    pendingThinkingBuffer = "";
    if (rafHandle !== null) {
      if (typeof cancelAnimationFrame === "function") {
        cancelAnimationFrame(rafHandle);
      } else {
        clearTimeout(rafHandle as unknown as ReturnType<typeof setTimeout>);
      }
      rafHandle = null;
    }
    set({ streamingContent: "", streamingThinking: "" });
  },
  setPlanMode: (value) => set({ isPlanMode: value }),
}));

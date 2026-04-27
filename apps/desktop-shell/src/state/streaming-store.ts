import { create } from "zustand";

/**
 * Streaming state — holds the backend text buffer and the visible text
 * reveal. The "is the session running?" question is answered by
 * `session.turn_state === "running"` read from React Query cache, NOT
 * by a separate boolean in this store.
 *
 * Rationale: having both `isStreaming` and `session.turn_state` led to
 * drift (e.g., isStreaming flipped true/false per Message event while
 * turn_state stayed Running, causing UI flicker). See audit-lessons L-07.
 *
 * ── Performance note ─────────────────────────────────────────────
 * `appendStreamingContent` writes only to `streamingBuffer`, which UI
 * components do not render directly. `useStreamingReveal` drains that
 * buffer into `streamingContent` at a model-agnostic cadence so providers
 * with different chunk patterns still feel like one product.
 */
export interface StreamingState {
  /** Backend-accurate text content from TextDelta SSE events. */
  streamingBuffer: string;
  /** User-visible text content, throttled by useStreamingReveal. */
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
  /** Append a text chunk to the backend buffer. */
  appendStreamingContent: (chunk: string) => void;
  /** Immediately reveal all currently buffered text. */
  flushStreamingContent: () => void;
  /** Append a thinking-summary chunk (forward-compatible; see above). */
  appendStreamingThinking: (chunk: string) => void;
  /** Clear both streaming buffers without changing other state. */
  clearStreamingContent: () => void;
  /** Toggle plan mode state. */
  setPlanMode: (value: boolean) => void;
}

// ── Thinking RAF batching ───────────────────────────────────────────
// Thinking remains a forward-compatible side channel. It is unrelated to
// the visible text reveal loop and can keep the lightweight RAF coalesce.
let pendingThinkingBuffer = "";
let rafHandle: number | null = null;

/** Test-only: synchronously flush pending thinking chunks. */
function flushPendingThinking() {
  const thinkingChunk = pendingThinkingBuffer;
  pendingThinkingBuffer = "";
  rafHandle = null;
  if (thinkingChunk.length === 0) return;
  useStreamingStore.setState((state) => ({
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
    rafHandle = requestAnimationFrame(() => flushPendingThinking());
  } else {
    // Fallback for non-browser environments (tests, SSR).
    rafHandle = setTimeout(flushPendingThinking, 16) as unknown as number;
  }
}

export const useStreamingStore = create<StreamingState>((set) => ({
  streamingBuffer: "",
  streamingContent: "",
  streamingThinking: "",
  isPlanMode: false,
  appendStreamingContent: (chunk) => {
    set((state) => ({
      streamingBuffer: state.streamingBuffer + chunk,
    }));
  },
  flushStreamingContent: () => {
    set((state) => ({
      streamingContent: state.streamingBuffer,
    }));
  },
  appendStreamingThinking: (chunk) => {
    pendingThinkingBuffer += chunk;
    scheduleFlush();
  },
  clearStreamingContent: () => {
    // Clearing must be synchronous so an incoming message arrival
    // immediately blanks the streaming buffer (prevents the last
    // chunk from appearing as a ghost after the complete message).
    pendingThinkingBuffer = "";
    if (rafHandle !== null) {
      if (typeof cancelAnimationFrame === "function") {
        cancelAnimationFrame(rafHandle);
      } else {
        clearTimeout(rafHandle as unknown as ReturnType<typeof setTimeout>);
      }
      rafHandle = null;
    }
    set({ streamingBuffer: "", streamingContent: "", streamingThinking: "" });
  },
  setPlanMode: (value) => set({ isPlanMode: value }),
}));

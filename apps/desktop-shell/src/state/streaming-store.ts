import { create } from "zustand";

/**
 * Streaming state for the agentic loop's real-time text output.
 *
 * When the backend sends TextDelta SSE events during an agentic turn,
 * the streaming content is accumulated here for live rendering.
 * On message completion (or turn end), the buffer is cleared.
 */
export interface StreamingState {
  /** Whether the backend is currently streaming a response. */
  isStreaming: boolean;
  /** Accumulated text content from TextDelta SSE events. */
  streamingContent: string;
  /** Set streaming on/off. Clears content when set to false. */
  setStreaming: (value: boolean) => void;
  /** Append a text chunk to the streaming buffer. */
  appendStreamingContent: (chunk: string) => void;
  /** Clear the streaming buffer without changing isStreaming. */
  clearStreamingContent: () => void;
}

export const useStreamingStore = create<StreamingState>((set) => ({
  isStreaming: false,
  streamingContent: "",
  setStreaming: (value) =>
    set({
      isStreaming: value,
      ...(value ? {} : { streamingContent: "" }),
    }),
  appendStreamingContent: (chunk) =>
    set((state) => ({
      streamingContent: state.streamingContent + chunk,
    })),
  clearStreamingContent: () => set({ streamingContent: "" }),
}));

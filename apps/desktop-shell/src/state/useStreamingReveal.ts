import { useEffect } from "react";
import { useStreamingStore } from "./streaming-store";

const TARGET_CHARS_PER_SEC = 40;
const FRAMES_PER_SEC = 60;
const CHARS_PER_FRAME = TARGET_CHARS_PER_SEC / FRAMES_PER_SEC;

let activeConsumers = 0;
let sharedRafHandle: number | null = null;
let sharedFraction = 0;

function streamingRevealTick() {
  const state = useStreamingStore.getState();
  const buffer = state.streamingBuffer;
  const visible = state.streamingContent;

  if (buffer.length < visible.length) {
    useStreamingStore.setState({ streamingContent: buffer });
  } else if (buffer.length > visible.length) {
    const lag = buffer.length - visible.length;
    let charsThisFrame = CHARS_PER_FRAME;

    if (lag > 500) {
      charsThisFrame *= 3;
    } else if (lag > 100) {
      charsThisFrame *= 1.5;
    }

    sharedFraction += charsThisFrame;
    const wholeChars = Math.floor(sharedFraction);
    sharedFraction -= wholeChars;

    if (wholeChars > 0) {
      const newVisibleLen = Math.min(visible.length + wholeChars, buffer.length);
      useStreamingStore.setState({
        streamingContent: buffer.substring(0, newVisibleLen),
      });
    }
  }

  sharedRafHandle = requestAnimationFrame(streamingRevealTick);
}

/**
 * Cross-model streaming throttle.
 *
 * Backend events from any provider append to `streamingBuffer`.
 * This hook drains buffer -> streamingContent at a steady visual cadence,
 * so provider-specific chunk timing does not become provider-specific UI.
 */
export function useStreamingReveal() {
  useEffect(() => {
    activeConsumers += 1;
    if (sharedRafHandle === null) {
      sharedRafHandle = requestAnimationFrame(streamingRevealTick);
    }
    return () => {
      activeConsumers = Math.max(0, activeConsumers - 1);
      if (activeConsumers === 0 && sharedRafHandle !== null) {
        cancelAnimationFrame(sharedRafHandle);
        sharedRafHandle = null;
        sharedFraction = 0;
      }
    };
  }, []);
}

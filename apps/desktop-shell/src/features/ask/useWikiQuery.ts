/**
 * useWikiQuery — hook for POST /api/wiki/query SSE consumption.
 * Per technical-design.md §2.2.
 *
 * SSE event sequence (all events share event name `skill`); parsing is
 * chunk-boundary safe because a single event may span multiple reads.
 *   data: { type: "query_chunk", delta, source_refs }
 *     — streamed incrementally; each delta is appended to the answer.
 *   data: { type: "query_done",  sources: QuerySource[], total_tokens?: number }
 *     — emitted exactly once when the backend query_wiki task finishes
 *       successfully. `sources` feeds QuerySourcesCard. `total_tokens`
 *       is currently ignored by the UI but accepted for forward compat.
 *   data: { type: "query_error", error: string }
 *     — emitted instead of query_done if query_wiki returns Err OR if
 *       the backend query task panics/is cancelled. The already-streamed
 *       partial answer is preserved; `isQuerying` flips to false and
 *       `error` is exposed for the renderer.
 */

import { useCallback, useRef, useState } from "react";
import type { QuerySource } from "@/api/wiki/types";

interface WikiQueryState {
  isQuerying: boolean;
  question: string;
  answer: string;
  sources: QuerySource[];
  error: string | null;
}

const INITIAL_STATE: WikiQueryState = {
  isQuerying: false,
  question: "",
  answer: "",
  sources: [],
  error: null,
};

export function useWikiQuery() {
  const [state, setState] = useState<WikiQueryState>(INITIAL_STATE);
  const abortRef = useRef<AbortController | null>(null);

  const queryWiki = useCallback(async (question: string) => {
    // Abort any previous in-flight query.
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setState({
      isQuerying: true,
      question,
      answer: "",
      sources: [],
      error: null,
    });

    try {
      const { getDesktopApiBase } = await import("@/lib/desktop/bootstrap");
      const base = await getDesktopApiBase();

      const response = await fetch(`${base}/api/wiki/query`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ question, max_sources: 5 }),
        signal: controller.signal,
      });

      if (!response.ok) {
        const errBody = await response.text().catch(() => "");
        throw new Error(`/query failed (${response.status}): ${errBody}`);
      }

      if (!response.body) {
        throw new Error("/query response has no body");
      }

      // Parse SSE stream.
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      let dataLines: string[] = [];
      let accumulatedAnswer = "";
      // Track whether the stream ended with an explicit query_error so
      // the post-loop fallthrough doesn't clobber the error message.
      let sawError = false;

      const processWikiQueryEvent = (event: {
        type?: string;
        delta?: unknown;
        sources?: unknown;
        error?: unknown;
      }) => {
        if (event.type === "query_chunk" && typeof event.delta === "string") {
          accumulatedAnswer += event.delta;
          setState((prev) => ({
            ...prev,
            answer: accumulatedAnswer,
          }));
        } else if (event.type === "query_done") {
          // Final event on the success path. Copy sources so the
          // QuerySourcesCard has what to render; total_tokens
          // is accepted but not currently displayed.
          const nextSources: QuerySource[] = Array.isArray(event.sources)
            ? (event.sources as QuerySource[])
            : [];
          setState((prev) => ({
            ...prev,
            sources: nextSources,
            isQuerying: false,
          }));
        } else if (event.type === "query_error") {
          // Final event on the failure path. Preserve any partial
          // answer already streamed so the user sees what the model
          // got through before failing.
          sawError = true;
          const msg =
            typeof event.error === "string" && event.error.length > 0
              ? event.error
              : "wiki query failed";
          setState((prev) => ({
            ...prev,
            error: msg,
            isQuerying: false,
          }));
        }
      };

      const flushEvent = () => {
        if (dataLines.length === 0) return;

        const jsonStr = dataLines.join("\n");
        dataLines = [];
        try {
          processWikiQueryEvent(JSON.parse(jsonStr));
        } catch (err) {
          console.warn("Ignoring malformed wiki query SSE event", err);
        }
      };

      const processSseLine = (rawLine: string) => {
        const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;

        if (line === "") {
          flushEvent();
          return;
        }

        if (line.startsWith("data:")) {
          let data = line.slice(5);
          if (data.startsWith(" ")) data = data.slice(1);
          dataLines.push(data);
        }
      };

      while (true) {
        const { done, value } = await reader.read();
        if (done) {
          const tail = decoder.decode();
          if (tail) buffer += tail;

          if (buffer.length > 0) {
            const lines = buffer.split("\n");
            buffer = "";
            for (const line of lines) {
              processSseLine(line);
            }
          }

          // Be lenient at EOF in case the server omits the final blank line.
          flushEvent();
          break;
        }

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        // Keep the last incomplete line in the buffer.
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          processSseLine(line);
        }
      }

      // Fallthrough — if the stream ended without an explicit
      // query_done or query_error (shouldn't happen with the fixed
      // backend, but guard anyway), clear the spinner. Don't touch
      // `error` if a query_error already populated it.
      if (!sawError) {
        setState((prev) =>
          prev.isQuerying ? { ...prev, isQuerying: false } : prev,
        );
      }
    } catch (err) {
      if ((err as Error).name === "AbortError") return;
      setState((prev) => ({
        ...prev,
        isQuerying: false,
        error: err instanceof Error ? err.message : String(err),
      }));
    }
  }, []);

  const reset = useCallback(() => {
    abortRef.current?.abort();
    setState(INITIAL_STATE);
  }, []);

  return { ...state, queryWiki, reset };
}

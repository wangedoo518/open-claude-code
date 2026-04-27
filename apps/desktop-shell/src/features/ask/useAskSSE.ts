/**
 * useAskSSE — subscribes to the SSE endpoint for a running session and
 * dispatches text_delta events to the streaming store, permission_request
 * events to the permissions store, and snapshot/message events to React
 * Query cache.
 *
 * Replaces the 1-second polling approach with real-time streaming.
 * Falls back gracefully if the SSE endpoint is unavailable (backend
 * returns non-200 or connection drops).
 */

import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { subscribeToSessionEvents } from "./api/client";
import { useStreamingStore } from "@/state/streaming-store";
import { usePermissionsStore } from "@/state/permissions-store";
import type { DesktopSessionEvent, DesktopSessionDetail } from "@/lib/tauri";

function isUsableSessionDetail(
  detail: DesktopSessionDetail | undefined,
): detail is DesktopSessionDetail {
  return Array.isArray(detail?.session?.messages);
}

export function useAskSSE(
  sessionId: string | null,
  isRunning: boolean,
) {
  const queryClient = useQueryClient();
  const controllerRef = useRef<AbortController | null>(null);

  useEffect(() => {
    // Only subscribe when a session is actively running
    if (!sessionId || !isRunning) {
      // If we had a subscription, abort it and clear streaming buffer
      if (controllerRef.current) {
        controllerRef.current.abort();
        controllerRef.current = null;
        useStreamingStore.getState().clearStreamingContent();
      }
      return;
    }

    // Already subscribed to this session
    if (controllerRef.current) return;

    const controller = subscribeToSessionEvents(
      sessionId,
      (event: DesktopSessionEvent) => {
        switch (event.type) {
          case "text_delta":
            useStreamingStore.getState().appendStreamingContent(event.content);
            break;

          case "permission_request":
            usePermissionsStore.getState().setPendingPermission({
              id: event.request_id,
              toolName: event.tool_name,
              toolInput: (() => {
                try { return JSON.parse(event.tool_input) as Record<string, unknown>; }
                catch {
                  console.warn("[ask-sse] failed to parse tool_input JSON, using raw string");
                  return { raw_input: event.tool_input } as Record<string, unknown>;
                }
              })(),
              riskLevel: "high",
            });
            break;

          case "snapshot":
            // A1 integration — `context_basis` (and `enrich_status`) are
            // transient side-channels: the backend stamps them only on
            // the snapshot fired right after `append_user_message`, and
            // subsequent snapshots (running→idle transitions, late bg
            // enrich, etc.) carry `None`. Plain setQueryData would let
            // the idle snapshot overwrite the running one and the UI
            // label would disappear as soon as the turn finishes.
            //
            // Sticky-preserve rule: when the incoming snapshot omits a
            // side-channel field but the previous cache had it set,
            // keep the previous value. This preserves the turn-local
            // basis/enrich across the running→idle flip.
            queryClient.setQueryData(
              ["clawwiki", "ask", "session", sessionId],
              (prev: DesktopSessionDetail | undefined) => {
                const next = event.session;
                if (!isUsableSessionDetail(next)) return prev ?? next;
                if (!isUsableSessionDetail(prev)) return next;
                return {
                  ...next,
                  context_basis: next.context_basis ?? prev.context_basis ?? null,
                  enrich_status: next.enrich_status ?? prev.enrich_status ?? null,
                };
              },
            );
            break;

          case "message":
            // A complete message has arrived. Reveal any buffered text once,
            // then clear the transient stream after one paint so the final
            // persisted message can take over without leaving stale chunks.
            useStreamingStore.getState().flushStreamingContent();
            useStreamingStore.setState({ streamingBuffer: "" });
            // Invalidate the session query to pick up the new message
            queryClient.setQueryData(
              ["clawwiki", "ask", "session", sessionId],
              (prev: DesktopSessionDetail | undefined) => {
                if (!isUsableSessionDetail(prev)) return prev;
                const previousMessages = Array.isArray(prev.session.messages)
                  ? prev.session.messages
                  : [];
                return {
                  ...prev,
                  session: {
                    ...prev.session,
                    messages: [...previousMessages, event.message],
                  },
                };
              },
            );
            setTimeout(() => {
              const state = useStreamingStore.getState();
              if (state.streamingBuffer.length === 0) {
                state.clearStreamingContent();
              }
            }, 50);
            break;
        }
      },
      (error) => {
        console.warn("[ask-sse] connection error, falling back to polling", error.message);
        if (sessionId) {
          void queryClient.invalidateQueries({
            queryKey: ["clawwiki", "ask", "session", sessionId],
          });
        }
        useStreamingStore.getState().clearStreamingContent();
        toast.warning("实时流式连接失败，已退化为轮询模式", { duration: 4000 });
        controllerRef.current = null;
      },
    );

    controllerRef.current = controller;

    return () => {
      controller.abort();
      controllerRef.current = null;
      useStreamingStore.getState().clearStreamingContent();
    };
  }, [sessionId, isRunning, queryClient]);
}

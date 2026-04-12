/**
 * useAskSession — drive a single Desktop session from the Ask page.
 *
 * S3 MVP strategy (per canonical §6.1): we reuse the existing
 * `DesktopState` session engine (Phase 1-2 of the pre-cut code) and
 * wrap it with a React Query hook that:
 *
 *   1. Resolves or creates the active Ask session on first mount.
 *      The session id is persisted in localStorage under
 *      `clawwiki:ask:activeSessionId` so the page remembers which
 *      conversation you were in between refreshes.
 *
 *   2. Holds a React Query cache entry for `getSession(id)` and
 *      refetches it on a 1-second interval **only while the turn is
 *      running**. When the backend reports `turn_state: "idle"` we
 *      stop polling to avoid busy-looping on an idle session.
 *
 *   3. Exposes an `onSend` that POSTs to `/sessions/:id/messages`
 *      and then eagerly invalidates the detail query so the poll
 *      restarts (the append response returns the new session
 *      snapshot, so one refetch is usually enough).
 *
 * S4 will graduate this to an SSE subscription driving the
 * `streaming-store` so streaming tokens render character-by-character
 * instead of block-by-block. S3 intentionally sticks with polling
 * because it needs ZERO new backend surface and matches the shape
 * `AskWorkbench` already accepts.
 *
 * Prior art: the pre-cut `features/session-workbench/useSession.ts`
 * did the same thing against `/api/desktop/sessions/:id`. We are
 * essentially reimplementing that hook under the Ask namespace.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  appendMessage,
  createSession,
  getSession,
} from "./api/client";
import type { DesktopSessionDetail } from "@/lib/tauri";

const ACTIVE_SESSION_STORAGE_KEY = "clawwiki:ask:activeSessionId";

const askSessionKeys = {
  all: ["clawwiki", "ask", "session"] as const,
  detail: (id: string | null) =>
    ["clawwiki", "ask", "session", id ?? "none"] as const,
};

function readActiveSessionId(): string | null {
  try {
    const raw = window.localStorage.getItem(ACTIVE_SESSION_STORAGE_KEY);
    return raw && raw.length > 0 ? raw : null;
  } catch {
    return null;
  }
}

function writeActiveSessionId(id: string | null) {
  try {
    if (id) {
      window.localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, id);
    } else {
      window.localStorage.removeItem(ACTIVE_SESSION_STORAGE_KEY);
    }
  } catch {
    // localStorage can throw on private browsing / quota — tolerated.
  }
}

export interface UseAskSessionResult {
  /** Currently-active session id, or `null` before the first resolve. */
  sessionId: string | null;
  /** Full session detail from the backend, or `null` while loading. */
  session: DesktopSessionDetail | null;
  /** True while the initial create / load is in flight. */
  isLoadingSession: boolean;
  /** True while the append mutation is in flight. */
  isSending: boolean;
  /**
   * True while the backend reports the turn is running OR we are
   * currently appending a message.
   */
  isTurnActive: boolean;
  /** Human-readable error from the most recent failed call, if any. */
  errorMessage?: string;
  /** Send a message on the current session. */
  onSend: (text: string) => Promise<void>;
  /**
   * Forget the current session and start a new one on the next
   * render. Clears the persisted active id.
   */
  onResetSession: () => void;
  /** Switch to an existing session by id. */
  onSwitchSession: (id: string) => void;
}

/**
 * Main hook. Idempotent: mounting twice in the same shell is a
 * no-op because React Query dedupes the underlying queries.
 */
export function useAskSession(): UseAskSessionResult {
  const queryClient = useQueryClient();
  // Read from localStorage on init. ensureMutation validates on first mount.
  // If the stored ID is stale (404), ensureMutation clears it and creates new.
  const [activeId, setActiveId] = useState<string | null>(() => readActiveSessionId());
  const [errorMessage, setErrorMessage] = useState<string | undefined>();

  // Step 1 — ensure an active session exists. If localStorage had a
  // stale id that the backend rejects (404) we clear it and let the
  // next render retry.
  const ensureMutation = useMutation({
    mutationFn: async () => {
      // Fast path: an id is already in localStorage and backend
      // confirms it exists.
      const persisted = readActiveSessionId();
      if (persisted) {
        try {
          const session = await getSession(persisted);
          return session;
        } catch (err) {
          // 404 or other failure — fall through to create a new one.
          console.warn("[ask] stored session not found, recreating", err);
        }
      }
      // Create a fresh session at the canonical wiki root. The
      // backend defaults `project_path` to cwd so we leave it empty.
      const created = await createSession({
        title: "Ask · new conversation",
      });
      return created.session;
    },
    onSuccess: (session) => {
      writeActiveSessionId(session.id);
      setActiveId(session.id);
      queryClient.setQueryData(askSessionKeys.detail(session.id), session);
      setErrorMessage(undefined);
    },
    onError: (err) => {
      setErrorMessage(
        err instanceof Error ? err.message : "Failed to resolve session"
      );
    },
  });

  // Kick off the ensure on first mount. We deliberately don't guard
  // against re-entry with a ref — React Query's mutation is already
  // idempotent when mutate() is called from an effect, and the
  // strict-mode double-invoke on dev just creates two sessions the
  // first time (acceptable for MVP).
  useEffect(() => {
    if (!activeId && !ensureMutation.isPending) {
      ensureMutation.mutate();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mutate is stable
  }, [activeId]);

  // Step 2 — poll the session detail. Only refetch on an interval
  // while the turn is active; idle turns use the standard staleTime
  // behavior and don't hammer the backend.
  // Track whether we just transitioned from running → idle so we can
  // do one final refetch to ensure the last message batch is captured.
  const prevTurnStateRef = useRef<string | undefined>(undefined);

  const detailQuery = useQuery({
    queryKey: askSessionKeys.detail(activeId),
    queryFn: async () => {
      if (!activeId) {
        throw new Error("no active session");
      }
      try {
        return await getSession(activeId);
      } catch (err) {
        // Session not found (404) — clear stale ID and create new
        console.warn("[ask] session not found, clearing stale ID");
        writeActiveSessionId(null);
        setActiveId(null);
        throw err;
      }
    },
    enabled: activeId !== null,
    staleTime: 5_000,
    retry: false, // Don't retry 404s — let ensureMutation handle it
    refetchOnMount: "always",
    refetchInterval: (q) => {
      const data = q.state.data as DesktopSessionDetail | undefined;
      const currentState = data?.turn_state;
      const wasRunning = prevTurnStateRef.current === "running";
      prevTurnStateRef.current = currentState;

      if (currentState === "running") return 1000;
      // Just transitioned to idle — do one more poll to capture final messages
      if (wasRunning && currentState === "idle") return 1000;
      return false;
    },
  });

  // Step 3 — send message mutation. On success, optimistically
  // replace the cached session with the snapshot the append route
  // returned; detail poll will pick up the turn-state flip to idle
  // on its next tick.
  const sendMutation = useMutation({
    mutationFn: async (text: string) => {
      if (!activeId) throw new Error("no active session");
      return appendMessage(activeId, text);
    },
    onSuccess: (response) => {
      if (activeId) {
        queryClient.setQueryData(
          askSessionKeys.detail(activeId),
          response.session
        );
      }
      setErrorMessage(undefined);
    },
    onError: (err) => {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    },
  });

  const onSend = useCallback(
    async (text: string) => {
      if (!activeId) {
        setErrorMessage("Session not ready yet — try again in a moment.");
        return;
      }
      await sendMutation.mutateAsync(text);
    },
    [activeId, sendMutation]
  );

  const onResetSession = useCallback(() => {
    writeActiveSessionId(null);
    setActiveId(null);
    setErrorMessage(undefined);
    queryClient.removeQueries({ queryKey: askSessionKeys.all });
  }, [queryClient]);

  /** Switch to an existing session by id. */
  const onSwitchSession = useCallback((id: string) => {
    writeActiveSessionId(id);
    setActiveId(id);
    setErrorMessage(undefined);
  }, []);

  const session = detailQuery.data ?? null;
  const isLoadingSession =
    ensureMutation.isPending ||
    (detailQuery.isLoading && activeId !== null);
  const isSending = sendMutation.isPending;
  const isTurnActive = session?.turn_state === "running" || isSending;

  return useMemo(
    () => ({
      sessionId: activeId,
      session,
      isLoadingSession,
      isSending,
      isTurnActive,
      errorMessage,
      onSend,
      onResetSession,
      onSwitchSession,
    }),
    [
      activeId,
      session,
      isLoadingSession,
      isSending,
      isTurnActive,
      errorMessage,
      onSend,
      onResetSession,
      onSwitchSession,
    ]
  );
}

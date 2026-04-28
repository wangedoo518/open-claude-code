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
// Regression fix for 2026-04 "empty conversation pile-up": no auto-create on mount.
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  appendMessage,
  bindSourceToSession,
  createSession,
  getSession,
} from "./api/client";
// A5.2 — pull the canonical project path from `/api/desktop/settings` so
// the backend's providers.json lookup (scoped by project_path) resolves
// against the same directory the user's settings UI already points to.
// Without this, new sessions inherit `default_project_path()` (OnceLock
// cwd), which on the gray build is the outer repo root and therefore
// never contains `.claw/providers.json` — so `model_label` stays on the
// "Opus 4.6" placeholder forever. See corrections.jsonl 2026-04-21.
import { getSettings } from "@/api/desktop/settings";
import type {
  ContextMode,
  DesktopSessionDetail,
  SourceRef,
} from "@/lib/tauri";

const ACTIVE_SESSION_STORAGE_KEY = "clawwiki:ask:activeSessionId";

const askSessionKeys = {
  all: ["clawwiki", "ask", "session"] as const,
  detail: (id: string | null) =>
    ["clawwiki", "ask", "session", id ?? "none"] as const,
};

function isUsableSessionDetail(detail: unknown): detail is DesktopSessionDetail {
  if (!detail || typeof detail !== "object") return false;
  const candidate = detail as { session?: { messages?: unknown } };
  return Array.isArray(candidate.session?.messages);
}

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
  /**
   * Send a message on the current session. A1 sprint — optional
   * `options.mode` carries the per-turn context mode (follow_up /
   * source_first / combine) decided by the Composer's classifier
   * (possibly overridden by the user). Legacy callers that omit the
   * second arg keep working; the backend treats a missing `mode` as
   * its default (typically `follow_up`).
   */
  onSend: (
    text: string,
    options?: { mode?: ContextMode },
  ) => Promise<void>;
  /**
   * Forget the current session and start a new one on the next
   * render. Clears the persisted active id.
   */
  onResetSession: () => void;
  /** Switch to an existing session by id. */
  onSwitchSession: (id: string) => void;
  /**
   * A2 sprint — ensure a session exists (lazy-create if needed), then
   * POST `/api/desktop/sessions/{id}/bind` with the given `SourceRef`
   * and return the resulting session detail. Reuses the same
   * ensure-session ladder as `onSend` so the "no auto-create on mount"
   * invariant is preserved — session creation still only happens on
   * explicit user action (bind handoff counts as one).
   */
  onEnsureAndBind: (source: SourceRef) => Promise<DesktopSessionDetail>;
}

/**
 * Main hook. Session lifecycle policy (regression guard — see commit
 * history for the "8 empty conversations" bug from 2026-04):
 *
 *   We NEVER auto-create a session on mount. Creation happens only
 *   when the user actually performs an action — either sending their
 *   first message or clicking "新建对话" and then sending. Mounting
 *   the hook (from AskPage, ChatSidePanel, route re-entry, strict-mode
 *   double-invoke, etc.) just reads the persisted id from localStorage
 *   and is a pure read: zero side effects to the backend.
 *
 *   Fallout of a stale/missing id: detailQuery gets a 404, we clear
 *   the id, and the UI falls back to the WelcomeScreen until the user
 *   sends. This is intentional — silently recreating was the source
 *   of the empty-conversation pile-up, because any 404 (including
 *   ones caused by React concurrent rendering re-mounting the hook
 *   before it finished a prior create) triggered another create.
 *
 *   Use `onResetSession` to intentionally start fresh; the actual
 *   POST still waits for the first message.
 */
export function useAskSession(): UseAskSessionResult {
  const queryClient = useQueryClient();
  // Read from localStorage on init. No mutate on mount — creation is
  // deferred until the first user action (send/reset-then-send).
  const [activeId, setActiveId] = useState<string | null>(() => readActiveSessionId());
  const [errorMessage, setErrorMessage] = useState<string | undefined>();

  // Step 1 — ensure an active session exists. Triggered ONLY by
  // `onSend` when activeId is null. Not fired automatically on mount,
  // to avoid the strict-mode + re-entry session pile-up bug.
  const ensureMutation = useMutation({
    mutationFn: async () => {
      // Fast path: an id is already in localStorage and backend
      // confirms it exists. `readActiveSessionId()` is read again here
      // because another hook instance (e.g. ChatSidePanel ↔ AskPage
      // hand-off) may have written a fresh id since this hook mounted.
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
      // A5.2 — resolve the canonical project_path before creating.
      // `ensureQueryData` hits the shared ["desktop","settings"] cache
      // that AskPage / ChatSidePanel / SettingsPage already populate,
      // so in the happy path this is a cache read, not a network call.
      // If the settings fetch fails we degrade to omitting `project_path`
      // (same as pre-A5.2 behavior) rather than blocking the user's
      // first send — the model_label regression is strictly worse than
      // keeping Ask usable.
      let projectPath: string | undefined;
      try {
        const settingsRes = await queryClient.ensureQueryData({
          queryKey: ["desktop", "settings"],
          queryFn: getSettings,
          staleTime: 5 * 60 * 1000,
        });
        projectPath = settingsRes.settings?.project_path || undefined;
      } catch (err) {
        console.warn(
          "[ask] settings fetch failed; creating session without project_path",
          err,
        );
      }
      const created = await createSession({
        title: "Ask · new conversation",
        ...(projectPath ? { project_path: projectPath } : {}),
      });
      return created.session;
    },
    onSuccess: (session) => {
      if (!isUsableSessionDetail(session)) {
        console.warn("[ask] create/ensure returned malformed session detail");
        setErrorMessage("Session response was malformed");
        return;
      }
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
        // Session not found (404) — clear stale ID. We do NOT
        // recreate automatically; the UI falls back to WelcomeScreen
        // until the user sends their first message.
        console.warn("[ask] session not found, clearing stale ID");
        writeActiveSessionId(null);
        setActiveId(null);
        throw err;
      }
    },
    enabled: activeId !== null,
    staleTime: 5_000,
    retry: false,
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

  // Step 3 — send message mutation. The mutation takes the session id
  // as a parameter rather than closing over `activeId`, so `onSend`
  // can hand in a freshly-created id (from ensureMutation.mutateAsync)
  // in the same call without waiting for a React re-render.
  const sendMutation = useMutation({
    mutationFn: async ({
      sessionId,
      text,
      mode,
    }: {
      sessionId: string;
      text: string;
      mode?: ContextMode;
    }) => {
      return appendMessage(sessionId, text, mode ? { mode } : undefined);
    },
    onSuccess: (response, variables) => {
      if (!isUsableSessionDetail(response.session)) {
        console.warn("[ask] append returned malformed session detail", {
          sessionId: variables.sessionId,
        });
        void queryClient.invalidateQueries({
          queryKey: askSessionKeys.detail(variables.sessionId),
        });
        return;
      }
      queryClient.setQueryData(
        askSessionKeys.detail(variables.sessionId),
        response.session
      );
      setErrorMessage(undefined);
    },
    onError: (err) => {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    },
  });

  const onSend = useCallback(
    async (text: string, options?: { mode?: ContextMode }) => {
      // Lazy-create path: only reach createSession when the user
      // actually wants to send something and has no session yet.
      //
      // CRITICAL (regression guard from 2026-04): this branch owns the
      // "5x route toggle creates 0 new session" invariant. Do NOT add
      // early-exits that short-circuit createSession based on the new
      // `options.mode` — the mode field is orthogonal to session
      // creation and must never influence whether a session is made.
      let idToUse = activeId;
      if (!idToUse) {
        try {
          const created = await ensureMutation.mutateAsync();
          idToUse = created.id;
        } catch (err) {
          setErrorMessage(
            err instanceof Error ? err.message : "Failed to create session"
          );
          return;
        }
      }
      await sendMutation.mutateAsync({
        sessionId: idToUse,
        text,
        mode: options?.mode,
      });
    },
    [activeId, sendMutation, ensureMutation]
  );

  const onEnsureAndBind = useCallback(
    async (source: SourceRef): Promise<DesktopSessionDetail> => {
      // Parallel to `onSend`'s lazy-create ladder — but intentionally
      // separate so the bind path does not touch the message-send
      // Critical invariant. Same rules apply: no auto-create except on
      // an explicit user action (bind handoff is one).
      let idToUse = activeId;
      if (!idToUse) {
        const created = await ensureMutation.mutateAsync();
        idToUse = created.id;
      }
      const next = await bindSourceToSession(idToUse, source);
      if (isUsableSessionDetail(next)) {
        queryClient.setQueryData(askSessionKeys.detail(idToUse), next);
      } else {
        void queryClient.invalidateQueries({
          queryKey: askSessionKeys.detail(idToUse),
        });
      }
      setErrorMessage(undefined);
      return next;
    },
    [activeId, ensureMutation, queryClient],
  );

  const onResetSession = useCallback(() => {
    // Clear — no immediate POST. Next user message creates the session.
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

  // P1 soft-recovery: when backend `turn_state` is stuck at `running`
  // for > 30 s past a non-trivial assistant message, the finalize SSE
  // event was almost certainly dropped (tokio task panic, bug, network
  // hiccup, or provider-side truncation that never closed the stream).
  // Without this the Composer replaces Send with Stop forever and the
  // user can't send the next message — the "ChatSidePanel 永久 disabled"
  // symptom reported in the P1 bug bash. We treat the session as idle
  // locally so the UI unblocks; a future sprint can wire an explicit
  // server-side reconciler for the mid-process (no-restart) case. See
  // `memory/corrections.jsonl` 2026-04-20 ChatSidePanel-stale-recovery.
  //
  // NOTE: we intentionally do NOT send a cancel / reset request to the
  // backend. If the real turn is actually still running (rare but
  // possible under heavy load with very long model latency), letting
  // it finish naturally is safe — our local "idle" is cosmetic.
  const isStale = useMemo(() => {
    if (!session || session.turn_state !== "running") return false;
    const msgs = session.session?.messages ?? [];
    const last = msgs[msgs.length - 1];
    if (!last || last.role !== "assistant") return false;
    // Only trust the "stale" verdict if the assistant actually produced
    // real content — a finalize-dropped stream with only a placeholder
    // could just be the natural "still thinking" phase.
    const lastText = (last.blocks ?? [])
      .map((b) => (b as { text?: string }).text ?? "")
      .join("");
    if (lastText.length < 10) return false;
    return Date.now() - session.updated_at > 30_000;
  }, [session]);

  // Edge-triggered warning so logs don't spam on every poll tick while
  // the session remains stale.
  const staleLoggedRef = useRef(false);
  useEffect(() => {
    if (isStale && !staleLoggedRef.current && session) {
      console.warn(
        "[ask] session",
        session.id,
        "marked stale — turn_state stuck at running, last assistant msg > 30s old",
        {
          updated_at: session.updated_at,
          ageMs: Date.now() - session.updated_at,
        },
      );
      staleLoggedRef.current = true;
    }
    if (!isStale) {
      staleLoggedRef.current = false;
    }
  }, [isStale, session]);

  const effectiveTurnState: "idle" | "running" = isStale
    ? "idle"
    : (session?.turn_state ?? "idle");
  const isTurnActive = effectiveTurnState === "running" || isSending;

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
      onEnsureAndBind,
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
      onEnsureAndBind,
    ]
  );
}

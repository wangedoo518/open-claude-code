// S0.4 ask client — minimal session-lifecycle HTTP wrappers.
//
// History: this file was extracted from
// `features/session-workbench/api/client.ts` on cut day. Only the
// functions that have at least one live consumer in the post-cut tree
// are kept here:
//   - session lifecycle (get/create/cancel/delete/rename/resume/append)
//   - forwardPermissionDecision  (AskWorkbench permission flow)
//
// Dropped on cut day because they no longer have any consumer:
//   - listWorkspaceSkills / WorkspaceSkill   (WorkspaceSkillsPanel deleted)
//   - forkSession / setSessionFlagged        (Inbox flow deferred to S4)
//   - setSessionLifecycleStatus              (Inbox flow deferred to S4)
//   - subscribeToSessionEvents               (S3 will rewire via ask_runtime)
//   - compactSession                         (slash commands cut)
//   - writePermissionModeToDisk              (moved to features/permission/permission-mode-client.ts)
//   - readPermissionModeFromDisk             (same)
//
// S3 will replace these wrappers with the typed `ask_runtime` client
// once that crate lands.

// Neutral API module; the old feature path re-exports this file.
import { fetchJson } from "@/lib/desktop/transport";
import type {
  AppendDesktopMessageResponse,
  ContextMode,
  CreateDesktopSessionResponse,
  DesktopSessionDetail,
  DesktopSessionEvent,
  DesktopSessionsResponse,
  SourceRef,
} from "@/api/contracts/desktop";

export async function listSessions(): Promise<DesktopSessionsResponse> {
  return fetchJson<DesktopSessionsResponse>("/api/desktop/sessions");
}

export async function getSession(
  sessionId: string,
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(`/api/desktop/sessions/${sessionId}`);
}

export async function createSession(payload: {
  title?: string;
  project_name?: string;
  project_path?: string;
}): Promise<CreateDesktopSessionResponse> {
  return fetchJson<CreateDesktopSessionResponse>("/api/desktop/sessions", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function appendMessage(
  sessionId: string,
  message: string,
  options?: { mode?: ContextMode },
): Promise<AppendDesktopMessageResponse> {
  // A1 sprint — optional `mode` field added to the body when the caller
  // provides one. Worker A's contract treats the field as optional and
  // falls back to `follow_up` / `Default` when omitted, so legacy
  // callers keep working.
  const body: Record<string, unknown> = { message };
  if (options?.mode) body.mode = options.mode;
  return fetchJson<AppendDesktopMessageResponse>(
    `/api/desktop/sessions/${sessionId}/messages`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

export async function cancelSession(
  sessionId: string,
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/cancel`,
    { method: "POST", body: JSON.stringify({}) },
  );
}

export async function deleteSession(
  sessionId: string,
): Promise<{ deleted: boolean }> {
  return fetchJson<{ deleted: boolean }>(
    `/api/desktop/sessions/${sessionId}`,
    { method: "DELETE" },
  );
}

/**
 * Delete every empty (zero-message) idle session in one call. Pass
 * `except` to preserve the session the user is currently staring at.
 * Added as one-click recovery for the pre-fix useAskSession bug that
 * piled up empty "Ask · new conversation" entries.
 */
export async function cleanupEmptySessions(
  except?: string | null,
): Promise<{ deleted_ids: string[]; deleted_count: number }> {
  return fetchJson<{ deleted_ids: string[]; deleted_count: number }>(
    "/api/desktop/sessions/cleanup-empty",
    {
      method: "POST",
      body: JSON.stringify(except ? { except } : {}),
    },
  );
}

export async function renameSession(
  sessionId: string,
  title: string,
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/title`,
    { method: "POST", body: JSON.stringify({ title }) },
  );
}

export async function resumeSession(
  sessionId: string,
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/resume`,
    { method: "POST", body: JSON.stringify({}) },
  );
}

export async function compactSession(
  sessionId: string,
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/compact`,
    { method: "POST", body: JSON.stringify({}) },
  );
}

/**
 * Subscribe to Server-Sent Events for a session.
 * Returns an AbortController to cancel the subscription.
 * The `onEvent` callback receives parsed DesktopSessionEvent objects.
 */
export function subscribeToSessionEvents(
  sessionId: string,
  onEvent: (event: DesktopSessionEvent) => void,
  onError?: (error: Error) => void,
): AbortController {
  const controller = new AbortController();

  (async () => {
    const { getDesktopApiBase } = await import("@/lib/desktop/bootstrap");
    const base = await getDesktopApiBase();
    const url = `${base}/api/desktop/sessions/${sessionId}/events`;

    try {
      const response = await fetch(url, {
        signal: controller.signal,
        headers: { Accept: "text/event-stream" },
      });

      if (!response.ok || !response.body) {
        onError?.(new Error(`SSE failed: ${response.status}`));
        return;
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        let dataLines: string[] = [];
        for (const line of lines) {
          if (line.startsWith("data: ")) {
            dataLines.push(line.slice(6));
          } else if (line === "" && dataLines.length > 0) {
            // End of event — parse accumulated data lines
            const jsonStr = dataLines.join("\n");
            dataLines = [];
            try {
              const event = JSON.parse(jsonStr) as DesktopSessionEvent;
              onEvent(event);
            } catch (parseErr) {
              console.warn("[sse] dropped malformed event:", jsonStr.slice(0, 200), parseErr);
            }
          }
        }
      }
    } catch (err) {
      if ((err as Error).name !== "AbortError") {
        onError?.(err instanceof Error ? err : new Error(String(err)));
      }
    }
  })();

  return controller;
}

/**
 * A2 sprint — bind a source (raw / wiki / inbox) to the given session.
 * The backend treats this as the authoritative context for every
 * subsequent turn until a `clearSourceBinding` call lands.
 *
 * Wire contract owned by Worker A:
 *   POST /api/desktop/sessions/{id}/bind
 *   body = SourceRef (tagged enum, snake_case)
 *   response = 200 + DesktopSessionDetail (with `source_binding` populated)
 */
export async function bindSourceToSession(
  sessionId: string,
  source: SourceRef,
): Promise<DesktopSessionDetail> {
  // A2 integration fix: Worker A's Rust handler deserializes
  // `BindSourceBody { source: SourceRef, reason?: Option<String> }`,
  // NOT a bare SourceRef — sending SourceRef directly yields 422.
  // Wrap in `{ source }` to match the canonical Rust body shape.
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/bind`,
    {
      method: "POST",
      body: JSON.stringify({ source }),
    },
  );
}

/**
 * A2 sprint — clear any active source binding on the given session.
 * After this call the Ask pipeline reverts to its default per-turn
 * source-resolution policy (URL enrich, etc.).
 *
 *   DELETE /api/desktop/sessions/{id}/bind
 *   response = 200 + DesktopSessionDetail (with `source_binding = null`)
 */
export async function clearSourceBinding(
  sessionId: string,
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/bind`,
    { method: "DELETE" },
  );
}

export async function forwardPermissionDecision(
  sessionId: string,
  payload: {
    requestId: string;
    decision: string;
  },
): Promise<{ forwarded: boolean }> {
  return fetchJson<{ forwarded: boolean }>(
    `/api/desktop/sessions/${sessionId}/permission`,
    {
      method: "POST",
      body: JSON.stringify(payload),
    },
  );
}

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

import { fetchJson } from "@/lib/desktop/transport";
import type {
  AppendDesktopMessageResponse,
  CreateDesktopSessionResponse,
  DesktopSessionDetail,
  DesktopSessionsResponse,
} from "@/lib/tauri";

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
): Promise<AppendDesktopMessageResponse> {
  return fetchJson<AppendDesktopMessageResponse>(
    `/api/desktop/sessions/${sessionId}/messages`,
    {
      method: "POST",
      body: JSON.stringify({ message }),
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
  onEvent: (event: import("@/lib/tauri").DesktopSessionEvent) => void,
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
              const event = JSON.parse(jsonStr) as import("@/lib/tauri").DesktopSessionEvent;
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

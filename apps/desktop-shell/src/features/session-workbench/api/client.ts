import { getDesktopApiBase } from "@/lib/desktop/bootstrap";
import { fetchJson } from "@/lib/desktop/transport";
import type {
  AppendDesktopMessageResponse,
  CreateDesktopSessionResponse,
  DesktopSessionDetail,
  DesktopSessionEvent,
  RuntimeConversationMessage,
} from "@/lib/tauri";

export async function getSession(
  sessionId: string
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
  message: string
): Promise<AppendDesktopMessageResponse> {
  return fetchJson<AppendDesktopMessageResponse>(
    `/api/desktop/sessions/${sessionId}/messages`,
    {
      method: "POST",
      body: JSON.stringify({ message }),
    }
  );
}

export async function cancelSession(
  sessionId: string
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/cancel`,
    { method: "POST", body: JSON.stringify({}) }
  );
}

export async function deleteSession(
  sessionId: string
): Promise<{ deleted: boolean }> {
  return fetchJson<{ deleted: boolean }>(
    `/api/desktop/sessions/${sessionId}`,
    { method: "DELETE" }
  );
}

export async function renameSession(
  sessionId: string,
  title: string
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/title`,
    { method: "POST", body: JSON.stringify({ title }) }
  );
}

export async function resumeSession(
  sessionId: string
): Promise<DesktopSessionDetail> {
  return fetchJson<DesktopSessionDetail>(
    `/api/desktop/sessions/${sessionId}/resume`,
    { method: "POST", body: JSON.stringify({}) }
  );
}

export async function forkSession(
  sessionId: string,
  messageIndex?: number
): Promise<{ session: DesktopSessionDetail }> {
  return fetchJson<{ session: DesktopSessionDetail }>(
    `/api/desktop/sessions/${sessionId}/fork`,
    {
      method: "POST",
      body: JSON.stringify({ message_index: messageIndex }),
    }
  );
}

/**
 * Update a session's lifecycle status (Inbox workflow).
 * Backed by L-09 follow-up: Todo → InProgress → NeedsReview → Done → Archived.
 */
export async function setSessionLifecycleStatus(
  sessionId: string,
  status: "todo" | "in_progress" | "needs_review" | "done" | "archived",
): Promise<{ session: DesktopSessionDetail }> {
  return fetchJson<{ session: DesktopSessionDetail }>(
    `/api/desktop/sessions/${sessionId}/lifecycle`,
    {
      method: "POST",
      body: JSON.stringify({ status }),
    },
  );
}

/** Toggle the flagged bit on a session. */
export async function setSessionFlagged(
  sessionId: string,
  flagged: boolean,
): Promise<{ session: DesktopSessionDetail }> {
  return fetchJson<{ session: DesktopSessionDetail }>(
    `/api/desktop/sessions/${sessionId}/flag`,
    {
      method: "POST",
      body: JSON.stringify({ flagged }),
    },
  );
}

/** Write the permission mode to the project's settings.json on disk. */
export async function writePermissionModeToDisk(
  projectPath: string,
  mode: "default" | "acceptEdits" | "bypassPermissions" | "plan",
): Promise<{ ok: boolean; mode: string }> {
  return fetchJson<{ ok: boolean; mode: string }>(
    `/api/desktop/settings/permission-mode`,
    {
      method: "POST",
      body: JSON.stringify({ project_path: projectPath, mode }),
    },
  );
}

/** Read the current permission mode from the project's settings.json. */
export async function readPermissionModeFromDisk(
  projectPath: string,
): Promise<{ mode: string }> {
  const params = new URLSearchParams({ project_path: projectPath });
  return fetchJson<{ mode: string }>(
    `/api/desktop/settings/permission-mode?${params.toString()}`,
    { method: "GET" },
  );
}

export async function compactSession(
  sessionId: string
): Promise<{ compacted: boolean }> {
  return fetchJson<{ compacted: boolean }>(
    `/api/desktop/sessions/${sessionId}/compact`,
    { method: "POST", body: JSON.stringify({}) }
  );
}

export async function forwardPermissionDecision(
  sessionId: string,
  payload: {
    requestId: string;
    decision: string;
  }
): Promise<{ forwarded: boolean }> {
  return fetchJson<{ forwarded: boolean }>(
    `/api/desktop/sessions/${sessionId}/permission`,
    {
      method: "POST",
      body: JSON.stringify(payload),
    }
  );
}

/** Payload for a permission_request SSE event from the agentic loop. */
export interface PermissionRequestPayload {
  session_id: string;
  request_id: string;
  tool_name: string;
  tool_input: string;
}

/** Payload for a text_delta SSE event from the agentic loop. */
export interface TextDeltaPayload {
  session_id: string;
  content: string;
}

export async function subscribeToSessionEvents(
  sessionId: string,
  handlers: {
    onSnapshot?: (session: DesktopSessionDetail) => void;
    onMessage?: (sessionId: string, message: RuntimeConversationMessage) => void;
    onPermissionRequest?: (payload: PermissionRequestPayload) => void;
    onTextDelta?: (payload: TextDeltaPayload) => void;
    onError?: (error: Error) => void;
  }
): Promise<() => void> {
  const base = await getDesktopApiBase();
  const source = new EventSource(`${base}/api/desktop/sessions/${sessionId}/events`);

  source.addEventListener("snapshot", (event) => {
    const payload = JSON.parse(
      (event as MessageEvent<string>).data
    ) as DesktopSessionEvent;
    if (payload.type === "snapshot") {
      handlers.onSnapshot?.(payload.session);
    }
  });

  source.addEventListener("message", (event) => {
    const payload = JSON.parse(
      (event as MessageEvent<string>).data
    ) as DesktopSessionEvent;
    if (payload.type === "message") {
      handlers.onMessage?.(payload.session_id, payload.message);
    }
  });

  source.addEventListener("permission_request", (event) => {
    const payload = JSON.parse(
      (event as MessageEvent<string>).data
    ) as PermissionRequestPayload;
    handlers.onPermissionRequest?.(payload);
  });

  source.addEventListener("text_delta", (event) => {
    const payload = JSON.parse(
      (event as MessageEvent<string>).data
    ) as TextDeltaPayload;
    handlers.onTextDelta?.(payload);
  });

  source.onerror = () => {
    // Close the connection explicitly to prevent the browser from
    // auto-reconnecting forever on flaky networks. The caller is
    // responsible for re-subscribing if needed.
    handlers.onError?.(new Error("Session event stream disconnected"));
    source.close();
  };

  return () => {
    source.close();
  };
}

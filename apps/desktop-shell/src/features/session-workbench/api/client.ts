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
    handlers.onError?.(new Error("Session event stream disconnected"));
  };

  return () => {
    source.close();
  };
}

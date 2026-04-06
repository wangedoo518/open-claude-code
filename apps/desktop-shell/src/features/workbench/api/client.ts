import { fetchJson } from "@/lib/desktop/transport";
import type {
  DesktopBootstrap,
  DesktopDispatchItemResponse,
  DesktopDispatchPriority,
  DesktopDispatchResponse,
  DesktopDispatchStatus,
  DesktopScheduledResponse,
  DesktopScheduledSchedule,
  DesktopScheduledTaskResponse,
  DesktopWorkbench,
  SearchDesktopSessionsResponse,
} from "@/lib/tauri";

export async function getBootstrap(): Promise<DesktopBootstrap> {
  return fetchJson<DesktopBootstrap>("/api/desktop/bootstrap");
}

export async function getWorkbench(): Promise<DesktopWorkbench> {
  return fetchJson<DesktopWorkbench>("/api/desktop/workbench");
}

export async function searchSessions(
  query: string
): Promise<SearchDesktopSessionsResponse> {
  return fetchJson<SearchDesktopSessionsResponse>(
    `/api/desktop/search?q=${encodeURIComponent(query)}`
  );
}

export async function getScheduled(): Promise<DesktopScheduledResponse> {
  return fetchJson<DesktopScheduledResponse>("/api/desktop/scheduled");
}

export async function createScheduledTask(payload: {
  title: string;
  prompt: string;
  project_name?: string;
  project_path?: string;
  target_session_id?: string | null;
  schedule: DesktopScheduledSchedule;
}): Promise<DesktopScheduledTaskResponse> {
  return fetchJson<DesktopScheduledTaskResponse>("/api/desktop/scheduled", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function updateScheduledTaskEnabled(
  taskId: string,
  enabled: boolean
): Promise<DesktopScheduledTaskResponse> {
  return fetchJson<DesktopScheduledTaskResponse>(
    `/api/desktop/scheduled/${taskId}/enabled`,
    {
      method: "POST",
      body: JSON.stringify({ enabled }),
    }
  );
}

export async function runScheduledTaskNow(
  taskId: string
): Promise<DesktopScheduledTaskResponse> {
  return fetchJson<DesktopScheduledTaskResponse>(
    `/api/desktop/scheduled/${taskId}/run`,
    {
      method: "POST",
      body: JSON.stringify({}),
    }
  );
}

export async function deleteScheduledTask(
  taskId: string
): Promise<{ deleted: boolean }> {
  return fetchJson<{ deleted: boolean }>(
    `/api/desktop/scheduled/${taskId}`,
    { method: "DELETE" }
  );
}

export async function updateScheduledTask(
  taskId: string,
  payload: {
    title?: string;
    prompt?: string;
    schedule?: DesktopScheduledSchedule;
    target_session_id?: string | null;
    enabled?: boolean;
  }
): Promise<DesktopScheduledTaskResponse> {
  return fetchJson<DesktopScheduledTaskResponse>(
    `/api/desktop/scheduled/${taskId}`,
    {
      method: "POST",
      body: JSON.stringify(payload),
    }
  );
}

export async function getDispatch(): Promise<DesktopDispatchResponse> {
  return fetchJson<DesktopDispatchResponse>("/api/desktop/dispatch");
}

export async function createDispatchItem(payload: {
  title: string;
  body: string;
  project_name?: string;
  project_path?: string;
  target_session_id?: string | null;
  priority: DesktopDispatchPriority;
}): Promise<DesktopDispatchItemResponse> {
  return fetchJson<DesktopDispatchItemResponse>("/api/desktop/dispatch", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function updateDispatchItemStatus(
  itemId: string,
  status: DesktopDispatchStatus
): Promise<DesktopDispatchItemResponse> {
  return fetchJson<DesktopDispatchItemResponse>(
    `/api/desktop/dispatch/items/${itemId}/status`,
    {
      method: "POST",
      body: JSON.stringify({ status }),
    }
  );
}

export async function deliverDispatchItem(
  itemId: string
): Promise<DesktopDispatchItemResponse> {
  return fetchJson<DesktopDispatchItemResponse>(
    `/api/desktop/dispatch/items/${itemId}/deliver`,
    {
      method: "POST",
      body: JSON.stringify({}),
    }
  );
}

export async function deleteDispatchItem(
  itemId: string
): Promise<{ deleted: boolean }> {
  return fetchJson<{ deleted: boolean }>(
    `/api/desktop/dispatch/items/${itemId}`,
    { method: "DELETE" }
  );
}

export async function updateDispatchItem(
  itemId: string,
  payload: {
    title?: string;
    body?: string;
    priority?: DesktopDispatchPriority;
    target_session_id?: string | null;
  }
): Promise<DesktopDispatchItemResponse> {
  return fetchJson<DesktopDispatchItemResponse>(
    `/api/desktop/dispatch/items/${itemId}`,
    {
      method: "POST",
      body: JSON.stringify(payload),
    }
  );
}

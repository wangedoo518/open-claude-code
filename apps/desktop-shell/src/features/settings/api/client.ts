import { fetchJson } from "@/lib/desktop/transport";
import type {
  DesktopCodexAuthOverviewResponse,
  DesktopCodexLoginSessionResponse,
  DesktopCodexRuntimeResponse,
  DesktopCustomizeResponse,
  DesktopManagedAuthAccountsResponse,
  DesktopManagedAuthLoginSessionResponse,
  DesktopManagedAuthProvidersResponse,
  DesktopSettingsResponse,
} from "@/lib/tauri";

export async function getCustomize(): Promise<DesktopCustomizeResponse> {
  return fetchJson<DesktopCustomizeResponse>("/api/desktop/customize");
}

export async function getSettings(): Promise<DesktopSettingsResponse> {
  return fetchJson<DesktopSettingsResponse>("/api/desktop/settings");
}

export async function getManagedAuthProviders(): Promise<DesktopManagedAuthProvidersResponse> {
  return fetchJson<DesktopManagedAuthProvidersResponse>("/api/desktop/auth/providers");
}

export async function getManagedAuthAccounts(
  providerId: string
): Promise<DesktopManagedAuthAccountsResponse> {
  return fetchJson<DesktopManagedAuthAccountsResponse>(
    `/api/desktop/auth/providers/${providerId}/accounts`
  );
}

export async function importManagedAuthAccounts(
  providerId: string
): Promise<DesktopManagedAuthAccountsResponse> {
  return fetchJson<DesktopManagedAuthAccountsResponse>(
    `/api/desktop/auth/providers/${providerId}/import`,
    {
      method: "POST",
    }
  );
}

export async function beginManagedAuthLogin(
  providerId: string
): Promise<DesktopManagedAuthLoginSessionResponse> {
  return fetchJson<DesktopManagedAuthLoginSessionResponse>(
    `/api/desktop/auth/providers/${providerId}/login`,
    {
      method: "POST",
    }
  );
}

export async function pollManagedAuthLogin(
  providerId: string,
  sessionId: string
): Promise<DesktopManagedAuthLoginSessionResponse> {
  return fetchJson<DesktopManagedAuthLoginSessionResponse>(
    `/api/desktop/auth/providers/${providerId}/login/${sessionId}`
  );
}

export async function setManagedAuthDefaultAccount(
  providerId: string,
  accountId: string
): Promise<DesktopManagedAuthAccountsResponse> {
  return fetchJson<DesktopManagedAuthAccountsResponse>(
    `/api/desktop/auth/providers/${providerId}/accounts/${accountId}/default`,
    {
      method: "POST",
    }
  );
}

export async function refreshManagedAuthAccount(
  providerId: string,
  accountId: string
): Promise<DesktopManagedAuthAccountsResponse> {
  return fetchJson<DesktopManagedAuthAccountsResponse>(
    `/api/desktop/auth/providers/${providerId}/accounts/${accountId}/refresh`,
    {
      method: "POST",
    }
  );
}

export async function removeManagedAuthAccount(
  providerId: string,
  accountId: string
): Promise<DesktopManagedAuthAccountsResponse> {
  return fetchJson<DesktopManagedAuthAccountsResponse>(
    `/api/desktop/auth/providers/${providerId}/accounts/${accountId}`,
    {
      method: "DELETE",
    }
  );
}

export async function getCodexRuntime(): Promise<DesktopCodexRuntimeResponse> {
  return fetchJson<DesktopCodexRuntimeResponse>("/api/desktop/codex/runtime");
}

export async function getCodexAuthOverview(): Promise<DesktopCodexAuthOverviewResponse> {
  return fetchJson<DesktopCodexAuthOverviewResponse>("/api/desktop/codex/auth");
}

export async function importCodexAuthProfile(): Promise<DesktopCodexAuthOverviewResponse> {
  return fetchJson<DesktopCodexAuthOverviewResponse>("/api/desktop/codex/auth/import", {
    method: "POST",
  });
}

export async function activateCodexAuthProfile(
  profileId: string
): Promise<DesktopCodexAuthOverviewResponse> {
  return fetchJson<DesktopCodexAuthOverviewResponse>(
    `/api/desktop/codex/auth/profiles/${profileId}/activate`,
    {
      method: "POST",
    }
  );
}

export async function refreshCodexAuthProfile(
  profileId: string
): Promise<DesktopCodexAuthOverviewResponse> {
  return fetchJson<DesktopCodexAuthOverviewResponse>(
    `/api/desktop/codex/auth/profiles/${profileId}/refresh`,
    {
      method: "POST",
    }
  );
}

export async function removeCodexAuthProfile(
  profileId: string
): Promise<DesktopCodexAuthOverviewResponse> {
  return fetchJson<DesktopCodexAuthOverviewResponse>(
    `/api/desktop/codex/auth/profiles/${profileId}`,
    {
      method: "DELETE",
    }
  );
}

export async function beginCodexLogin(): Promise<DesktopCodexLoginSessionResponse> {
  return fetchJson<DesktopCodexLoginSessionResponse>("/api/desktop/codex/auth/login", {
    method: "POST",
  });
}

export async function pollCodexLogin(
  sessionId: string
): Promise<DesktopCodexLoginSessionResponse> {
  return fetchJson<DesktopCodexLoginSessionResponse>(
    `/api/desktop/codex/auth/login/${sessionId}`
  );
}

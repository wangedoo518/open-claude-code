import { fetchJson } from "@/lib/desktop/transport";

export type CodexAccountStatus = "fresh" | "expiring" | "expired";

export interface CloudAccountInput {
  codex_user_id: string;
  alias: string;
  access_token: string;
  refresh_token: string;
  token_expires_at_epoch: number;
  subscription_id?: number | null;
  cloud_account_id?: number | null;
}

export interface CloudAccountPublic {
  codex_user_id: string;
  alias: string;
  token_expires_at_epoch: number;
  subscription_id: number | null;
  cloud_account_id: number | null;
  status: CodexAccountStatus;
}

export interface CloudAccountsListResponse {
  accounts: CloudAccountPublic[];
}

export interface BrokerPublicStatus {
  pool_size: number;
  fresh_count: number;
  expiring_count: number;
  expired_count: number;
  requests_today: number;
  next_refresh_at_epoch: number | null;
}

export async function syncCloudCodexAccounts(
  accounts: CloudAccountInput[],
): Promise<{ ok: boolean; pool_size: number }> {
  return fetchJson<{ ok: boolean; pool_size: number }>(
    "/api/desktop/cloud/codex-accounts",
    {
      method: "POST",
      body: JSON.stringify({ accounts }),
    },
  );
}

export async function listCloudCodexAccounts(): Promise<CloudAccountsListResponse> {
  return fetchJson<CloudAccountsListResponse>(
    "/api/desktop/cloud/codex-accounts",
  );
}

export async function clearCloudCodexAccounts(): Promise<{ ok: boolean }> {
  return fetchJson<{ ok: boolean }>(
    "/api/desktop/cloud/codex-accounts/clear",
    { method: "POST" },
  );
}

export async function getBrokerStatus(): Promise<BrokerPublicStatus> {
  return fetchJson<BrokerPublicStatus>("/api/broker/status");
}

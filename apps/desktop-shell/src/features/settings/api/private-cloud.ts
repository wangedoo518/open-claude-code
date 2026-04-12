import { fetchJson } from "@/lib/desktop/transport";

// ── S2: Codex broker (subscription pool, private-cloud only) ──────
//
// Wire protocol for the 4 S2 routes in `desktop-server/src/lib.rs`:
//   POST /api/desktop/cloud/codex-accounts         sync
//   GET  /api/desktop/cloud/codex-accounts         list (redacted)
//   POST /api/desktop/cloud/codex-accounts/clear   clear
//   GET  /api/broker/status                        public status
//
// Types match `desktop_core::codex_broker::{CloudAccountInput,
// CloudAccountPublic, BrokerPublicStatus}` byte-for-byte. The pool
// itself lives inside the Rust process; this TS layer only ever sees
// redacted views and aggregate counts. OSS builds hide this surface
// from the UI and do not register the backing routes.

export type CodexAccountStatus = "fresh" | "expiring" | "expired";

/** Raw tokens the frontend pushes into the broker. Sensitive —
 *  comes from `billing/cloud-accounts-sync` and goes directly into
 *  the sync POST body. Never persisted in localStorage, never echoed
 *  back to the frontend on subsequent reads. */
export interface CloudAccountInput {
  codex_user_id: string;
  alias: string;
  access_token: string;
  refresh_token: string;
  token_expires_at_epoch: number;
  subscription_id?: number | null;
  cloud_account_id?: number | null;
}

/** Redacted view returned by GET. Has NO token fields. */
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

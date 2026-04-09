import { fetchJson } from "@/lib/desktop/transport";
import type {
  DesktopBootstrap,
  DesktopCodexAuthOverviewResponse,
  DesktopCodexLoginSessionResponse,
  DesktopCodexRuntimeResponse,
  DesktopCustomizeResponse,
  DesktopManagedAuthAccountsResponse,
  DesktopManagedAuthLoginSessionResponse,
  DesktopManagedAuthProvidersResponse,
  DesktopSettingsResponse,
} from "@/lib/tauri";

// S0.4: getBootstrap moved here from features/workbench/api/client.ts
// (deleted on cut day). It's the only function from that file with a
// surviving consumer (`features/settings/SettingsPage.tsx`).
export async function getBootstrap(): Promise<DesktopBootstrap> {
  return fetchJson<DesktopBootstrap>("/api/desktop/bootstrap");
}

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

// ── Phase 3-5 multi-provider registry: DELETED on S0.4 cut day ────
//
// The Phase 3-5 multi-provider catalogue was removed per ClawWiki
// canonical §11.1 cut #6. ClawWiki uses a single managed Codex pool
// from S2 onward — users do not pick a provider, do not paste API
// keys, and do not see this surface. See the S2 block below instead.

// ── S2: Codex broker (subscription pool) ──────────────────────────
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
// redacted views and aggregate counts.

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

// ── Phase 6: WeChat account management ────────────────────────────
//
// These routes pair with the WeChat iLink backend to let the user
// add/remove WeChat accounts entirely from the desktop UI (no CLI).
//
// See `rust/crates/desktop-server/src/lib.rs` handlers
// `list_wechat_accounts_handler`, `start_wechat_login_handler`,
// `wechat_login_status_handler`, `cancel_wechat_login_handler`,
// `delete_wechat_account_handler`.

export type WeChatAccountStatus =
  | "connected"
  | "disconnected"
  | "session_expired";

export interface WeChatAccountSummary {
  id: string;
  display_name: string;
  base_url: string;
  /** First 6 / last 4 chars of the bot token plus length, for display only. */
  bot_token_preview: string;
  /** ISO-8601 string of the cursor timestamp (when the monitor last saw traffic). */
  last_active_at: string | null;
  status: WeChatAccountStatus;
}

export interface WeChatAccountsResponse {
  accounts: WeChatAccountSummary[];
}

export interface WeChatLoginStartRequest {
  base_url?: string;
}

export interface WeChatLoginStartResponse {
  handle: string;
  /** Full data URI (`data:image/png;base64,...`) safe to set as <img src>. */
  qr_image_base64: string;
  expires_at: string;
}

export type WeChatLoginStatus =
  | "waiting"
  | "scanned"
  | "confirmed"
  | "failed"
  | "cancelled"
  | "expired";

export interface WeChatLoginStatusResponse {
  status: WeChatLoginStatus;
  account_id?: string | null;
  error?: string | null;
}

export async function listWeChatAccounts(): Promise<WeChatAccountsResponse> {
  return fetchJson<WeChatAccountsResponse>("/api/desktop/wechat/accounts");
}

export async function startWeChatLogin(
  baseUrl?: string
): Promise<WeChatLoginStartResponse> {
  const body: WeChatLoginStartRequest = baseUrl ? { base_url: baseUrl } : {};
  return fetchJson<WeChatLoginStartResponse>(
    "/api/desktop/wechat/login/start",
    {
      method: "POST",
      body: JSON.stringify(body),
    }
  );
}

export async function getWeChatLoginStatus(
  handle: string
): Promise<WeChatLoginStatusResponse> {
  return fetchJson<WeChatLoginStatusResponse>(
    `/api/desktop/wechat/login/${encodeURIComponent(handle)}/status`
  );
}

export async function cancelWeChatLogin(
  handle: string
): Promise<{ ok: boolean }> {
  return fetchJson<{ ok: boolean }>(
    `/api/desktop/wechat/login/${encodeURIComponent(handle)}/cancel`,
    { method: "POST" }
  );
}

export async function deleteWeChatAccount(
  id: string
): Promise<{ ok: boolean }> {
  return fetchJson<{ ok: boolean }>(
    `/api/desktop/wechat/accounts/${encodeURIComponent(id)}`,
    { method: "DELETE" }
  );
}

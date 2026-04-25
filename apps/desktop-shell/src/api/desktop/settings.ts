// Neutral API module; the old feature path re-exports this file.
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
} from "@/api/contracts/desktop";

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

// ── Channel B: Official WeChat Customer Service (kefu) ──────────────

export interface KefuConfigRequest {
  corpid: string;
  secret: string;
  token: string;
  encoding_aes_key: string;
  account_name?: string;
}

export interface KefuConfigSummary {
  corpid: string;
  secret_preview: string;
  token_preview: string;
  open_kfid: string | null;
  contact_url: string | null;
  account_name: string | null;
  saved_at: string | null;
  has_aes_key: boolean;
  configured?: boolean;
}

export interface KefuCapabilities {
  text: boolean;
  url: boolean;
  query: boolean;
  commands: string[];
  file: boolean;
  image: boolean;
  card: boolean;
  share: boolean;
}

export interface KefuStatus {
  configured: boolean;
  account_created: boolean;
  monitor_running: boolean;
  last_poll_unix_ms: number | null;
  last_inbound_unix_ms: number | null;
  consecutive_failures: number;
  last_error: string | null;
  capabilities?: KefuCapabilities;
}

export async function saveKefuConfig(
  config: KefuConfigRequest
): Promise<{ ok: boolean }> {
  return fetchJson("/api/desktop/wechat-kefu/config", {
    method: "POST",
    body: JSON.stringify(config),
  });
}

export async function loadKefuConfig(): Promise<KefuConfigSummary> {
  return fetchJson("/api/desktop/wechat-kefu/config");
}

export async function createKefuAccount(
  name: string = "ClaudeWiki助手"
): Promise<{ ok: boolean; open_kfid: string }> {
  return fetchJson("/api/desktop/wechat-kefu/account/create", {
    method: "POST",
    body: JSON.stringify({ name }),
  });
}

export async function getKefuContactUrl(): Promise<{ url: string }> {
  return fetchJson("/api/desktop/wechat-kefu/contact-url");
}

export async function getKefuStatus(): Promise<KefuStatus> {
  return fetchJson("/api/desktop/wechat-kefu/status");
}

export async function startKefuMonitor(): Promise<{ ok: boolean }> {
  return fetchJson("/api/desktop/wechat-kefu/monitor/start", {
    method: "POST",
  });
}

export async function stopKefuMonitor(): Promise<{ ok: boolean }> {
  return fetchJson("/api/desktop/wechat-kefu/monitor/stop", {
    method: "POST",
  });
}

// ── Pipeline ────────────────────────────────────────────────────────

export interface PipelinePhaseState {
  phase: "cf_register" | "worker_deploy" | "wecom_auth" | "callback_config" | "kefu_create";
  status: "pending" | "running" | "waiting_scan" | "skipped" | "done" | "failed";
  message: string | null;
  error: string | null;
}

export interface KefuPipelineState {
  phases: PipelinePhaseState[];
  logs: string[];
  current_phase: string | null;
  started_at: string | null;
  finished_at: string | null;
  contact_url: string | null;
  qr_data: string | null;
  active: boolean;
}

export async function startKefuPipeline(opts: {
  skip_cf_register?: boolean;
  cf_api_token?: string;
  skip_callback_config?: boolean;
  corpid?: string;
  secret?: string;
}): Promise<{ ok: boolean }> {
  return fetchJson("/api/desktop/wechat-kefu/pipeline/start", {
    method: "POST",
    body: JSON.stringify(opts),
  });
}

export async function getKefuPipelineStatus(): Promise<KefuPipelineState> {
  return fetchJson("/api/desktop/wechat-kefu/pipeline/status");
}

export async function cancelKefuPipeline(): Promise<{ ok: boolean }> {
  return fetchJson("/api/desktop/wechat-kefu/pipeline/cancel", {
    method: "POST",
  });
}

// ── Multi-provider registry (generic compatible gateways) ──────────

export type ProviderKind = "anthropic" | "openai_compat";

export interface DesktopProviderSummary {
  id: string;
  kind: ProviderKind;
  display_name: string | null;
  base_url: string;
  api_key_display: string;
  api_key_length: number;
  model: string;
  max_tokens: number;
}

export interface DesktopProvidersListResponse {
  version: number;
  active: string;
  providers: DesktopProviderSummary[];
}

export interface UpsertProviderRequest {
  id: string;
  project_path?: string;
  entry: {
    kind: ProviderKind;
    display_name?: string;
    base_url?: string;
    api_key: string;
    model: string;
    max_tokens?: number;
  };
}

export interface UpsertProviderResponse {
  ok: boolean;
  id: string;
  active: string;
  entry: DesktopProviderSummary;
}

export interface DesktopProviderTemplate {
  id: string;
  display_name: string;
  kind: ProviderKind;
  base_url: string;
  default_model: string;
  max_tokens: number;
  description: string;
  api_key_url: string;
}

export interface DesktopProviderTemplatesResponse {
  templates: DesktopProviderTemplate[];
}

export async function listProviders(
  projectPath?: string
): Promise<DesktopProvidersListResponse> {
  const query = projectPath
    ? `?project_path=${encodeURIComponent(projectPath)}`
    : "";
  return fetchJson<DesktopProvidersListResponse>(
    `/api/desktop/providers${query}`
  );
}

export async function listProviderTemplates(): Promise<DesktopProviderTemplatesResponse> {
  return fetchJson<DesktopProviderTemplatesResponse>(
    "/api/desktop/providers/templates"
  );
}

export async function upsertProvider(
  request: UpsertProviderRequest
): Promise<UpsertProviderResponse> {
  return fetchJson<UpsertProviderResponse>("/api/desktop/providers", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function deleteProvider(
  id: string,
  projectPath?: string
): Promise<{ deleted: boolean; id: string; active: string }> {
  const query = projectPath
    ? `?project_path=${encodeURIComponent(projectPath)}`
    : "";
  return fetchJson<{ deleted: boolean; id: string; active: string }>(
    `/api/desktop/providers/${encodeURIComponent(id)}${query}`,
    { method: "DELETE" }
  );
}

export async function activateProvider(
  id: string,
  projectPath?: string
): Promise<{ ok: boolean; active: string }> {
  const query = projectPath
    ? `?project_path=${encodeURIComponent(projectPath)}`
    : "";
  return fetchJson<{ ok: boolean; active: string }>(
    `/api/desktop/providers/${encodeURIComponent(id)}/activate${query}`,
    { method: "POST" }
  );
}

export interface ProviderTestResult {
  ok: boolean;
  latency_ms: number;
  error?: string | null;
  model_echo?: string | null;
}

export async function testProvider(
  id: string,
  projectPath?: string
): Promise<ProviderTestResult> {
  const query = projectPath
    ? `?project_path=${encodeURIComponent(projectPath)}`
    : "";
  return fetchJson<ProviderTestResult>(
    `/api/desktop/providers/${encodeURIComponent(id)}/test${query}`,
    { method: "POST" }
  );
}

import { invoke } from "@tauri-apps/api/core";

const DEFAULT_API_BASE = "http://127.0.0.1:4357";

let apiBasePromise: Promise<string> | null = null;

export type DesktopTabKind =
  | "home"
  | "search"
  | "scheduled"
  | "dispatch"
  | "customize"
  | "open_claw"
  | "settings"
  | "code_session";

export interface DesktopTopTab {
  id: string;
  label: string;
  kind: DesktopTabKind;
  closable: boolean;
}

export interface DesktopLaunchpadItem {
  id: string;
  label: string;
  description: string;
  accent: string;
  tab_id: string;
}

export interface DesktopSettingsGroup {
  id: string;
  label: string;
  description: string;
}

export interface DesktopBootstrap {
  product_name: string;
  code_label: string;
  top_tabs: DesktopTopTab[];
  launchpad_items: DesktopLaunchpadItem[];
  settings_groups: DesktopSettingsGroup[];
}

export interface DesktopSidebarAction {
  id: string;
  label: string;
  icon: string;
  target_tab_id: string;
  kind: DesktopTabKind;
}

export interface DesktopSessionSummary {
  id: string;
  title: string;
  preview: string;
  bucket: "today" | "yesterday" | "older";
  created_at: number;
  updated_at: number;
  project_name: string;
  project_path: string;
  environment_label: string;
  model_label: string;
  turn_state: "idle" | "running";
}

export interface DesktopSessionSection {
  id: string;
  label: string;
  sessions: DesktopSessionSummary[];
}

export interface DesktopComposerState {
  permission_mode_label: string;
  environment_label: string;
  model_label: string;
  send_label: string;
}

export interface DesktopWorkbench {
  primary_actions: DesktopSidebarAction[];
  secondary_actions: DesktopSidebarAction[];
  project_label: string;
  project_name: string;
  session_sections: DesktopSessionSection[];
  active_session_id: string | null;
  update_banner: {
    version: string;
    cta_label: string;
    body: string;
  };
  account: {
    name: string;
    plan_label: string;
    shortcut_label: string;
  };
  composer: DesktopComposerState;
}

export interface ContentBlockText {
  type: "text";
  text: string;
}

export interface ContentBlockToolUse {
  type: "tool_use";
  id: string;
  name: string;
  input: string;
}

export interface ContentBlockToolResult {
  type: "tool_result";
  tool_use_id: string;
  tool_name: string;
  output: string;
  is_error: boolean;
}

export type ContentBlock =
  | ContentBlockText
  | ContentBlockToolUse
  | ContentBlockToolResult;

export interface RuntimeConversationMessage {
  role: "system" | "user" | "assistant" | "tool";
  blocks: ContentBlock[];
}

export interface RuntimeSession {
  version: number;
  messages: RuntimeConversationMessage[];
}

export interface DesktopSessionDetail {
  id: string;
  title: string;
  preview: string;
  created_at: number;
  updated_at: number;
  project_name: string;
  project_path: string;
  environment_label: string;
  model_label: string;
  turn_state: "idle" | "running";
  session: RuntimeSession;
}

export interface DesktopProviderSetting {
  id: string;
  label: string;
  base_url: string;
  auth_status: string;
}

export interface DesktopProviderModel {
  model_id: string;
  display_name: string;
  context_window: number | null;
  max_output_tokens: number | null;
  billing_kind: string | null;
  capability_tags: string[];
}

export type DesktopProviderRuntimeTarget = "open_claw" | "codex";

export interface DesktopProviderPreset {
  id: string;
  name: string;
  runtime_target: DesktopProviderRuntimeTarget;
  category: string;
  provider_type: string;
  billing_category: string;
  protocol: string;
  base_url: string;
  official_verified: boolean;
  website_url: string | null;
  description: string | null;
  icon: string | null;
  icon_color: string | null;
  models: DesktopProviderModel[];
}

export interface DesktopManagedProvider {
  id: string;
  name: string;
  runtime_target: DesktopProviderRuntimeTarget;
  category: string;
  provider_type: string;
  billing_category: string;
  protocol: string;
  base_url: string;
  api_key_masked: string | null;
  has_api_key: boolean;
  enabled: boolean;
  official_verified: boolean;
  preset_id: string | null;
  website_url: string | null;
  description: string | null;
  models: DesktopProviderModel[];
  created_at_epoch: number;
  updated_at_epoch: number;
}

export interface DesktopOpenclawDefaultModel {
  primary: string | null;
  fallbacks: string[];
}

export interface DesktopOpenclawLiveProvider {
  id: string;
  base_url: string;
  protocol: string;
  model_count: number;
  has_api_key: boolean;
}

export interface DesktopOpenclawRuntimeState {
  config_path: string;
  live_provider_ids: string[];
  live_providers: DesktopOpenclawLiveProvider[];
  default_model: DesktopOpenclawDefaultModel;
  model_catalog_count: number;
  env: Record<string, string>;
  env_keys: string[];
  tools: Record<string, unknown>;
  tool_keys: string[];
  health_warnings: string[];
}

export interface DesktopCodexRuntimeState {
  config_dir: string;
  auth_path: string;
  config_path: string;
  active_provider_key: string | null;
  model: string | null;
  base_url: string | null;
  provider_count: number;
  has_api_key: boolean;
  has_chatgpt_tokens: boolean;
  auth_mode: string | null;
  auth_profile_label: string | null;
  auth_plan_type: string | null;
  live_providers: DesktopCodexLiveProvider[];
  health_warnings: string[];
}

export interface DesktopCodexLiveProvider {
  id: string;
  name: string | null;
  base_url: string | null;
  wire_api: string | null;
  requires_openai_auth: boolean;
  model: string | null;
  is_active: boolean;
}

export type DesktopCodexAuthSource = "imported_auth_json" | "browser_login";

export interface DesktopCodexProfileSummary {
  id: string;
  email: string;
  display_label: string;
  chatgpt_account_id: string | null;
  chatgpt_user_id: string | null;
  chatgpt_plan_type: string | null;
  auth_source: DesktopCodexAuthSource;
  active: boolean;
  applied_to_codex: boolean;
  last_refresh_epoch: number | null;
  access_token_expires_at_epoch: number | null;
  updated_at_epoch: number;
}

export interface DesktopCodexInstallationRecord {
  target_id: string;
  target_label: string;
  installed: boolean;
  path: string | null;
  auth_path: string;
}

export interface DesktopCodexAuthOverview {
  profiles: DesktopCodexProfileSummary[];
  installations: DesktopCodexInstallationRecord[];
  active_profile_id: string | null;
  auth_path: string;
  auth_mode: string | null;
  has_chatgpt_tokens: boolean;
  updated_at_epoch: number;
}

export type DesktopCodexLoginSessionStatus =
  | "pending"
  | "completed"
  | "failed"
  | "cancelled";

export interface DesktopCodexLoginSessionSnapshot {
  session_id: string;
  status: DesktopCodexLoginSessionStatus;
  authorize_url: string;
  redirect_uri: string;
  error: string | null;
  profile: DesktopCodexProfileSummary | null;
  created_at_epoch: number;
  updated_at_epoch: number;
}

export interface DesktopProviderSyncResult {
  provider_id: string;
  runtime_target: DesktopProviderRuntimeTarget;
  config_path: string;
  auth_path: string | null;
  model_count: number;
  primary_applied: string | null;
}

export interface DesktopProviderDeleteResult {
  deleted: boolean;
  provider_id: string;
  runtime_target: DesktopProviderRuntimeTarget;
  live_config_removed: boolean;
}

export type DesktopProviderConnectionStatus =
  | "success"
  | "warning"
  | "auth_error"
  | "error";

export interface DesktopProviderConnectionTestResult {
  status: DesktopProviderConnectionStatus;
  checked_url: string;
  http_status: number | null;
  message: string;
  response_excerpt: string | null;
  used_stored_api_key: boolean;
}

export interface CodeToolsTerminalConfig {
  id: string;
  name: string;
  customPath?: string | null;
}

export interface CodeToolSelectedModelPayload {
  providerId: string;
  providerName: string;
  providerType: string;
  runtimeTarget: DesktopProviderRuntimeTarget;
  baseUrl: string;
  protocol: string;
  modelId: string;
  displayName: string;
  managedProviderId: string | null;
  presetId: string | null;
  hasStoredCredential: boolean;
}

export interface RunCodeToolPayload {
  cliTool: string;
  directory: string;
  terminal: string;
  autoUpdateToLatest: boolean;
  environmentVariables: Record<string, string>;
  selectedModel: CodeToolSelectedModelPayload | null;
}

export interface CodeToolRunResult {
  success: boolean;
  message: string | null;
}

export interface DesktopOpenclawConfigWriteResult {
  config_path: string;
  changed: boolean;
}

export interface DesktopStorageLocation {
  label: string;
  path: string;
  description: string;
}

export interface DesktopSettingsState {
  project_path: string;
  config_home: string;
  desktop_session_store_path: string;
  oauth_credentials_path: string | null;
  providers: DesktopProviderSetting[];
  storage_locations: DesktopStorageLocation[];
  warnings: string[];
}

export interface DesktopCustomizeSummary {
  loaded_config_count: number;
  mcp_server_count: number;
  plugin_count: number;
  enabled_plugin_count: number;
  plugin_tool_count: number;
  pre_tool_hook_count: number;
  post_tool_hook_count: number;
}

export interface DesktopConfigFile {
  source: string;
  path: string;
}

export interface DesktopHookConfigView {
  pre_tool_use: string[];
  post_tool_use: string[];
}

export interface DesktopMcpServer {
  name: string;
  scope: string;
  transport: string;
  target: string;
}

export interface DesktopPluginView {
  id: string;
  name: string;
  version: string;
  description: string;
  kind: string;
  source: string;
  root_path: string | null;
  enabled: boolean;
  default_enabled: boolean;
  tool_count: number;
  pre_tool_hook_count: number;
  post_tool_hook_count: number;
}

export interface DesktopCustomizeState {
  project_path: string;
  model_id: string;
  model_label: string;
  permission_mode: string;
  summary: DesktopCustomizeSummary;
  loaded_configs: DesktopConfigFile[];
  hooks: DesktopHookConfigView;
  mcp_servers: DesktopMcpServer[];
  plugins: DesktopPluginView[];
  warnings: string[];
}

export interface CreateDesktopSessionResponse {
  session: DesktopSessionDetail;
}

export interface AppendDesktopMessageResponse {
  session: DesktopSessionDetail;
}

export interface DesktopCustomizeResponse {
  customize: DesktopCustomizeState;
}

export interface DesktopSettingsResponse {
  settings: DesktopSettingsState;
}

export interface DesktopProviderPresetsResponse {
  presets: DesktopProviderPreset[];
}

export interface DesktopManagedProvidersResponse {
  providers: DesktopManagedProvider[];
}

export interface DesktopManagedProviderResponse {
  provider: DesktopManagedProvider;
}

export interface DesktopProviderImportResponse {
  providers: DesktopManagedProvider[];
}

export interface DesktopProviderSyncResponse {
  result: DesktopProviderSyncResult;
}

export interface DesktopProviderDeleteResponse {
  result: DesktopProviderDeleteResult;
}

export interface DesktopProviderConnectionTestResponse {
  result: DesktopProviderConnectionTestResult;
}

export interface DesktopOpenclawRuntimeResponse {
  runtime: DesktopOpenclawRuntimeState;
}

export interface DesktopCodexRuntimeResponse {
  runtime: DesktopCodexRuntimeState;
}

export interface DesktopCodexAuthOverviewResponse {
  overview: DesktopCodexAuthOverview;
}

export interface DesktopCodexLoginSessionResponse {
  session: DesktopCodexLoginSessionSnapshot;
}

export interface DesktopOpenclawConfigWriteResponse {
  result: DesktopOpenclawConfigWriteResult;
}

export interface DesktopSearchHit {
  session_id: string;
  title: string;
  project_name: string;
  project_path: string;
  bucket: "today" | "yesterday" | "older";
  preview: string;
  snippet: string;
  updated_at: number;
}

export interface SearchDesktopSessionsResponse {
  results: DesktopSearchHit[];
}

export type DesktopWeekday =
  | "monday"
  | "tuesday"
  | "wednesday"
  | "thursday"
  | "friday"
  | "saturday"
  | "sunday";

export type DesktopScheduledTaskStatus = "idle" | "running";
export type DesktopScheduledRunStatus = "success" | "error";
export type DesktopScheduledTaskTargetKind = "new_session" | "existing_session";

export interface DesktopScheduledSummary {
  total_task_count: number;
  enabled_task_count: number;
  running_task_count: number;
  blocked_task_count: number;
  due_task_count: number;
}

export interface DesktopScheduledTaskTarget {
  kind: DesktopScheduledTaskTargetKind;
  session_id: string | null;
  label: string;
}

export type DesktopScheduledSchedule =
  | {
      kind: "hourly";
      interval_hours: number;
    }
  | {
      kind: "weekly";
      days: DesktopWeekday[];
      hour: number;
      minute: number;
    };

export interface DesktopScheduledTask {
  id: string;
  title: string;
  prompt: string;
  project_name: string;
  project_path: string;
  schedule: DesktopScheduledSchedule;
  schedule_label: string;
  target: DesktopScheduledTaskTarget;
  enabled: boolean;
  blocked_reason: string | null;
  status: DesktopScheduledTaskStatus;
  created_at: number;
  updated_at: number;
  last_run_at: number | null;
  next_run_at: number | null;
  last_run_status: DesktopScheduledRunStatus | null;
  last_outcome: string | null;
}

export interface DesktopScheduledState {
  project_path: string;
  summary: DesktopScheduledSummary;
  tasks: DesktopScheduledTask[];
  trusted_project_paths: string[];
  warnings: string[];
}

export interface DesktopScheduledResponse {
  scheduled: DesktopScheduledState;
}

export interface DesktopScheduledTaskResponse {
  task: DesktopScheduledTask;
}

export type DesktopDispatchSourceKind =
  | "local_inbox"
  | "remote_bridge"
  | "scheduled";
export type DesktopDispatchTargetKind = "new_session" | "existing_session";
export type DesktopDispatchPriority = "low" | "normal" | "high";
export type DesktopDispatchStatus =
  | "unread"
  | "read"
  | "delivering"
  | "delivered"
  | "archived"
  | "error";

export interface DesktopDispatchSummary {
  total_item_count: number;
  unread_item_count: number;
  pending_item_count: number;
  delivered_item_count: number;
  archived_item_count: number;
}

export interface DesktopDispatchSource {
  kind: DesktopDispatchSourceKind;
  label: string;
}

export interface DesktopDispatchTarget {
  kind: DesktopDispatchTargetKind;
  session_id: string | null;
  label: string;
}

export interface DesktopDispatchItem {
  id: string;
  title: string;
  body: string;
  project_name: string;
  project_path: string;
  source: DesktopDispatchSource;
  priority: DesktopDispatchPriority;
  target: DesktopDispatchTarget;
  status: DesktopDispatchStatus;
  created_at: number;
  updated_at: number;
  delivered_at: number | null;
  last_outcome: string | null;
}

export interface DesktopDispatchState {
  project_path: string;
  summary: DesktopDispatchSummary;
  items: DesktopDispatchItem[];
  warnings: string[];
}

export interface DesktopDispatchResponse {
  dispatch: DesktopDispatchState;
}

export interface DesktopDispatchItemResponse {
  item: DesktopDispatchItem;
}

export type DesktopSessionEvent =
  | {
      type: "snapshot";
      session: DesktopSessionDetail;
    }
  | {
      type: "message";
      session_id: string;
      message: RuntimeConversationMessage;
    };

async function readError(response: Response): Promise<string> {
  try {
    const payload = (await response.json()) as { error?: string };
    if (payload.error) {
      return payload.error;
    }
  } catch {
    // Fall back to reading response text.
  }

  try {
    const text = await response.text();
    if (text) {
      return text;
    }
  } catch {
    // Ignore text parse failure too.
  }

  return `Request failed with status ${response.status}`;
}

export async function getDesktopApiBase(): Promise<string> {
  if (!apiBasePromise) {
    apiBasePromise = (async () => {
      try {
        return await invoke<string>("desktop_api_base");
      } catch {
        return DEFAULT_API_BASE;
      }
    })();
  }

  return apiBasePromise;
}

async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const base = await getDesktopApiBase();
  const response = await fetch(`${base}${path}`, {
    ...init,
    headers: {
      Accept: "application/json",
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...(init?.headers ?? {}),
    },
  });

  if (!response.ok) {
    throw new Error(await readError(response));
  }

  return (await response.json()) as T;
}

export async function getBootstrap(): Promise<DesktopBootstrap> {
  return fetchJson<DesktopBootstrap>("/api/desktop/bootstrap");
}

export async function getWorkbench(): Promise<DesktopWorkbench> {
  return fetchJson<DesktopWorkbench>("/api/desktop/workbench");
}

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

export async function getCustomize(): Promise<DesktopCustomizeResponse> {
  return fetchJson<DesktopCustomizeResponse>("/api/desktop/customize");
}

export async function getSettings(): Promise<DesktopSettingsResponse> {
  return fetchJson<DesktopSettingsResponse>("/api/desktop/settings");
}

export async function getProviderPresets(): Promise<DesktopProviderPresetsResponse> {
  return fetchJson<DesktopProviderPresetsResponse>("/api/desktop/providers/presets");
}

export async function getManagedProviders(): Promise<DesktopManagedProvidersResponse> {
  return fetchJson<DesktopManagedProvidersResponse>("/api/desktop/providers");
}

export async function upsertManagedProvider(payload: {
  id?: string | null;
  name: string;
  runtime_target: DesktopProviderRuntimeTarget;
  category: string;
  provider_type: string;
  billing_category: string;
  protocol: string;
  base_url: string;
  api_key?: string | null;
  enabled: boolean;
  official_verified: boolean;
  preset_id?: string | null;
  website_url?: string | null;
  description?: string | null;
  models: DesktopProviderModel[];
}): Promise<DesktopManagedProviderResponse> {
  return fetchJson<DesktopManagedProviderResponse>("/api/desktop/providers", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function testManagedProviderConnection(payload: {
  id?: string | null;
  protocol: string;
  base_url: string;
  api_key?: string | null;
}): Promise<DesktopProviderConnectionTestResponse> {
  return fetchJson<DesktopProviderConnectionTestResponse>("/api/desktop/providers/test", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function deleteManagedProvider(
  providerId: string
): Promise<DesktopProviderDeleteResponse> {
  return fetchJson<DesktopProviderDeleteResponse>(
    `/api/desktop/providers/${providerId}`,
    {
      method: "DELETE",
    }
  );
}

export async function importLiveProviders(payload?: {
  provider_ids?: string[];
}): Promise<DesktopProviderImportResponse> {
  return fetchJson<DesktopProviderImportResponse>("/api/desktop/providers/import-live", {
    method: "POST",
    body: JSON.stringify({
      provider_ids: payload?.provider_ids ?? null,
    }),
  });
}

export async function importCodexLiveProviders(payload?: {
  provider_ids?: string[];
}): Promise<DesktopProviderImportResponse> {
  return fetchJson<DesktopProviderImportResponse>("/api/desktop/codex/import-live", {
    method: "POST",
    body: JSON.stringify({
      provider_ids: payload?.provider_ids ?? null,
    }),
  });
}

export async function syncManagedProvider(
  providerId: string,
  payload?: {
    set_primary?: boolean;
  }
): Promise<DesktopProviderSyncResponse> {
  return fetchJson<DesktopProviderSyncResponse>(
    `/api/desktop/providers/${providerId}/sync`,
    {
      method: "POST",
      body: JSON.stringify({
        set_primary: payload?.set_primary ?? false,
      }),
    }
  );
}

export async function getOpenclawRuntime(): Promise<DesktopOpenclawRuntimeResponse> {
  return fetchJson<DesktopOpenclawRuntimeResponse>("/api/desktop/openclaw/runtime");
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

export async function updateOpenclawEnv(payload: {
  env: Record<string, string>;
}): Promise<DesktopOpenclawConfigWriteResponse> {
  return fetchJson<DesktopOpenclawConfigWriteResponse>("/api/desktop/openclaw/env", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function updateOpenclawTools(payload: {
  tools: Record<string, unknown>;
}): Promise<DesktopOpenclawConfigWriteResponse> {
  return fetchJson<DesktopOpenclawConfigWriteResponse>("/api/desktop/openclaw/tools", {
    method: "POST",
    body: JSON.stringify(payload),
  });
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

export async function subscribeToSessionEvents(
  sessionId: string,
  handlers: {
    onSnapshot?: (session: DesktopSessionDetail) => void;
    onMessage?: (sessionId: string, message: RuntimeConversationMessage) => void;
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

  source.onerror = () => {
    handlers.onError?.(new Error("Session event stream disconnected"));
  };

  return () => {
    source.close();
  };
}

// ---------------------------------------------------------------------------
// Agent pipeline commands (OpenClaw install / start / uninstall)
// ---------------------------------------------------------------------------

import type {
  AgentId,
  AgentPipelineAction,
  AgentPipelineStatus,
  OpenclawConnectStatus,
  OpenclawRuntimeSnapshot,
  SetupProductOverview,
  OpenclawServiceControlResult,
} from "@/types/agent";

/**
 * Start an agent pipeline action (install, start, or uninstall).
 * The backend spawns an async task and returns the initial status.
 * Frontend should poll `agentPipelineStatus` to track progress.
 */
export async function agentPipelineStart(
  agentId: AgentId,
  action: AgentPipelineAction
): Promise<AgentPipelineStatus> {
  return invoke<AgentPipelineStatus>("agent_pipeline_start", {
    agentId,
    action,
  });
}

/**
 * Get the current status of an agent pipeline action.
 * Returns running/finished/success flags, logs, and hints.
 */
export async function agentPipelineStatus(
  agentId: AgentId,
  action: AgentPipelineAction
): Promise<AgentPipelineStatus> {
  return invoke<AgentPipelineStatus>("agent_pipeline_status", {
    agentId,
    action,
  });
}

/**
 * Check OpenClaw installation and connection status.
 * Runs binary detection, version check, and health probe.
 */
export async function openclawConnectStatus(): Promise<OpenclawConnectStatus> {
  return invoke<OpenclawConnectStatus>("openclaw_connect_status");
}

/**
 * Get OpenClaw runtime snapshot (process info, memory, uptime).
 * Checks if the gateway process is running on port 18790.
 */
export async function openclawRuntimeSnapshot(): Promise<OpenclawRuntimeSnapshot> {
  return invoke<OpenclawRuntimeSnapshot>("openclaw_runtime_snapshot");
}

/**
 * Get OpenClaw setup product overview (installed, running, version).
 * Reads install state file + live status.
 */
export async function openclawSetupOverview(): Promise<SetupProductOverview> {
  return invoke<SetupProductOverview>("openclaw_setup_overview");
}

/**
 * Control the OpenClaw service (currently only "stop" is supported).
 * Kills the gateway process.
 */
export async function openclawServiceControl(
  action: "stop"
): Promise<OpenclawServiceControlResult> {
  return invoke<OpenclawServiceControlResult>("openclaw_service_control", {
    action,
  });
}

/**
 * Open a URL in the system's default browser.
 * Used to open the OpenClaw dashboard page.
 */
export async function openDashboardUrl(url: string): Promise<void> {
  return invoke<void>("open_dashboard_url", { url });
}

// ---------------------------------------------------------------------------
// OpenClaw simplified commands (cherry-studio compatible)
// ---------------------------------------------------------------------------

export interface OpenclawInstallCheck {
  installed: boolean;
  path: string | null;
  needsMigration: boolean;
}

export interface OpenclawGatewayStatusResult {
  status: "stopped" | "running";
  port: number;
}

/**
 * Check if OpenClaw binary is installed on the system.
 * Returns installed flag and binary path.
 */
export async function openclawCheckInstalled(): Promise<OpenclawInstallCheck> {
  return invoke<OpenclawInstallCheck>("openclaw_check_installed");
}

/**
 * Get the current gateway status (stopped/running) and port.
 */
export async function openclawGetStatus(): Promise<OpenclawGatewayStatusResult> {
  return invoke<OpenclawGatewayStatusResult>("openclaw_get_status");
}

/**
 * Get the OpenClaw dashboard URL for embedding as MinApp webview.
 */
export async function openclawGetDashboardUrl(): Promise<string> {
  return invoke<string>("openclaw_get_dashboard_url");
}

// ---------------------------------------------------------------------------
// Code tools commands
// ---------------------------------------------------------------------------

export async function isBinaryExist(binaryName: string): Promise<boolean> {
  return invoke<boolean>("is_binary_exist", { binaryName });
}

export async function installBunBinary(): Promise<void> {
  return invoke<void>("install_bun_binary");
}

export async function getCodeToolAvailableTerminals(): Promise<
  CodeToolsTerminalConfig[]
> {
  return invoke<CodeToolsTerminalConfig[]>("code_tools_get_available_terminals");
}

export async function runCodeTool(
  payload: RunCodeToolPayload
): Promise<CodeToolRunResult> {
  return invoke<CodeToolRunResult>("code_tools_run", {
    payload: {
      cliTool: payload.cliTool,
      directory: payload.directory,
      terminal: payload.terminal,
      autoUpdateToLatest: payload.autoUpdateToLatest,
      environmentVariables: payload.environmentVariables,
      selectedModel: payload.selectedModel,
    },
  });
}

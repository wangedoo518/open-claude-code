import { invoke } from "@tauri-apps/api/core";
export { getDesktopApiBase } from "@/lib/desktop/bootstrap";
export * from "@/features/workbench/api/client";
export * from "@/features/session-workbench/api/client";
export * from "@/features/settings/api/client";
export * from "@/features/code-tools/api/client";

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

export type DesktopManagedAuthProviderKind = "codex_openai" | "qwen_code";

export type DesktopManagedAuthSource =
  | "imported_auth_json"
  | "browser_login"
  | "device_code";

export type DesktopManagedAuthAccountStatus =
  | "ready"
  | "expiring"
  | "expired"
  | "needs_reauth";

export type DesktopManagedAuthLoginSessionStatus =
  | "pending"
  | "completed"
  | "failed"
  | "cancelled";

export interface DesktopManagedAuthRuntimeBinding {
  runtime_name: string;
  auth_path: string | null;
  config_path: string | null;
  synced: boolean;
  synced_account_id: string | null;
}

export interface DesktopManagedAuthProvider {
  id: string;
  name: string;
  kind: DesktopManagedAuthProviderKind;
  website_url: string | null;
  description: string | null;
  models: DesktopProviderModel[];
  default_model_id: string | null;
  account_count: number;
  default_account_id: string | null;
  default_account_label: string | null;
  runtime: DesktopManagedAuthRuntimeBinding;
}

export interface DesktopManagedAuthAccount {
  id: string;
  provider_id: string;
  email: string | null;
  subject: string | null;
  display_label: string;
  plan_label: string | null;
  auth_source: DesktopManagedAuthSource;
  status: DesktopManagedAuthAccountStatus;
  is_default: boolean;
  applied_to_runtime: boolean;
  created_at_epoch: number;
  updated_at_epoch: number;
  last_refresh_epoch: number | null;
  access_token_expires_at_epoch: number | null;
  resource_url: string | null;
}

export interface DesktopManagedAuthLoginSessionSnapshot {
  session_id: string;
  provider_id: string;
  status: DesktopManagedAuthLoginSessionStatus;
  authorize_url: string | null;
  verification_uri: string | null;
  verification_uri_complete: string | null;
  user_code: string | null;
  redirect_uri: string | null;
  error: string | null;
  account: DesktopManagedAuthAccount | null;
  created_at_epoch: number;
  updated_at_epoch: number;
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
  baseUrl: string;
  protocol: string;
  modelId: string;
  displayName: string;
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

export interface DesktopManagedAuthProvidersResponse {
  providers: DesktopManagedAuthProvider[];
}

export interface DesktopManagedAuthAccountsResponse {
  provider: DesktopManagedAuthProvider;
  accounts: DesktopManagedAuthAccount[];
}

export interface DesktopManagedAuthLoginSessionResponse {
  session: DesktopManagedAuthLoginSessionSnapshot;
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

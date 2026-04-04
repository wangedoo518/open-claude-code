/**
 * OpenClaw Agent Management Type System
 *
 * Ported from clawhub123/src/types.ts and clawhub123/src/v2/features/agents/types.ts
 * Defines the complete type hierarchy for agent lifecycle management:
 * install → start → dashboard → stop → uninstall
 */

// ---------------------------------------------------------------------------
// Core identifiers
// ---------------------------------------------------------------------------

export type AgentId = "openclaw";
export type AgentPipelineAction = "install" | "start" | "uninstall";

// ---------------------------------------------------------------------------
// Pipeline status (returned by agent_pipeline_start / agent_pipeline_status)
// ---------------------------------------------------------------------------

export interface AgentPipelineStatus {
  run_key: string; // "openclaw:install"
  agent_id: string; // "openclaw"
  action: string; // "install" | "start" | "uninstall"
  running: boolean;
  finished: boolean;
  success: boolean;
  logs: string[];
  dashboard_url?: string | null;
  hint?: string | null;
  updated_at_epoch: number;
}

// ---------------------------------------------------------------------------
// OpenClaw connect status (returned by openclaw_connect_status)
// ---------------------------------------------------------------------------

export interface OpenclawConnectStatus {
  connected: boolean;
  installed: boolean;
  command_path: string | null;
  version: string | null;
  install_mode: string | null;
  managed_by_warwolf: boolean;
  node_version: string | null;
  provider_exists: boolean;
  model_count: number;
  error: string | null;
}

// ---------------------------------------------------------------------------
// Runtime snapshot (returned by openclaw_runtime_snapshot)
// ---------------------------------------------------------------------------

export interface OpenclawRuntimeSnapshot {
  running: boolean;
  pid: number | null;
  memory_bytes: number | null;
  uptime_seconds: number | null;
  activity_state: "idle" | "busy" | "unknown";
  os: string;
  config_initialized: boolean;
}

// ---------------------------------------------------------------------------
// Product overview (returned by openclaw_setup_overview)
// ---------------------------------------------------------------------------

export interface SetupProductOverview {
  installed: boolean;
  connected: boolean;
  service_running: boolean;
  model_count: number;
  install_mode: "managed_native" | "reuse_existing" | null;
  version: string | null;
  command_path: string | null;
  managed_by_warwolf: boolean;
  node_version: string | null;
}

// ---------------------------------------------------------------------------
// Service control result
// ---------------------------------------------------------------------------

export interface OpenclawServiceControlResult {
  success: boolean;
}

// ---------------------------------------------------------------------------
// Presentation types (view models)
// ---------------------------------------------------------------------------

export type HomeTone = "default" | "info" | "success" | "warning" | "error";

export interface AgentStatusNotice {
  tone: HomeTone;
  message: string;
}

export interface AgentKeyValueItem {
  label: string;
  value: string;
}

export interface AgentRuntimeMetric {
  label: string;
  value: string;
  note?: string;
}

export interface AgentLifecycleStep {
  id: "install" | "start";
  title: string;
  description: string;
  statusLabel: string;
  statusTone: HomeTone;
  rows: AgentKeyValueItem[];
  hint?: string | null;
  logs: string[];
  emptyText: string;
  defaultExpanded?: boolean;
}

// ---------------------------------------------------------------------------
// Composite agent detail (assembled from parallel fetches)
// ---------------------------------------------------------------------------

export interface OpenclawAgentDetail {
  connectStatus: OpenclawConnectStatus;
  product: SetupProductOverview;
  runtimeSnapshot: OpenclawRuntimeSnapshot;
  installStatus: AgentPipelineStatus;
  serviceStatus: AgentPipelineStatus;
  uninstallStatus: AgentPipelineStatus;
  fetchErrors?: string[];
}

// ---------------------------------------------------------------------------
// Agent workbench state (computed view model for UI rendering)
// ---------------------------------------------------------------------------

export interface AgentWorkbenchSupported {
  kind: "supported";
  detail: OpenclawAgentDetail;
  statusLabel: string;
  statusTone: HomeTone;
  statusNotice: AgentStatusNotice;
  primaryActionLabel: string;
  heroSummary: string[];
  runtimeMetrics: AgentRuntimeMetric[];
  environmentItems: AgentKeyValueItem[];
  lifecycleSteps: AgentLifecycleStep[];
  uninstallActionLabel: string;
}

export interface AgentWorkbenchLoading {
  kind: "loading";
  statusLabel: string;
  statusTone: HomeTone;
  statusNotice: AgentStatusNotice;
  primaryActionLabel: string;
}

export interface AgentWorkbenchError {
  kind: "error";
  statusLabel: string;
  statusTone: HomeTone;
  statusNotice: AgentStatusNotice;
  primaryActionLabel: string;
  errorMessage: string;
}

export type AgentWorkbenchState =
  | AgentWorkbenchSupported
  | AgentWorkbenchLoading
  | AgentWorkbenchError;

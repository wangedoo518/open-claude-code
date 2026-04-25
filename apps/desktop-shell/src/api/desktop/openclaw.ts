import { invoke } from "@tauri-apps/api/core";

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
 *
 * Falls back to `window.open` when running in a plain browser
 * (dev mode via `npm run dev`) where Tauri's IPC bridge is not
 * available. The Tauri `invoke` function exists (it's imported)
 * but throws when the Tauri runtime isn't present.
 */
export async function openDashboardUrl(url: string): Promise<void> {
  try {
    await invoke<void>("open_dashboard_url", { url });
  } catch {
    // Tauri IPC not available (browser dev mode) — fall back to
    // opening the URL in a new tab.
    window.open(url, "_blank", "noopener,noreferrer");
  }
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

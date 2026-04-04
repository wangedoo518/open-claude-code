#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agents;
mod pipeline;

use std::env;
use serde::{Deserialize, Serialize};
use tauri::State;
use pipeline::PipelineStore;

// ---------------------------------------------------------------------------
// Shared types (mirrored in TypeScript)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenclawConnectStatus {
    connected: bool,
    installed: bool,
    command_path: Option<String>,
    version: Option<String>,
    install_mode: Option<String>,
    managed_by_warwolf: bool,
    node_version: Option<String>,
    provider_exists: bool,
    model_count: u32,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenclawRuntimeSnapshot {
    running: bool,
    pid: Option<u32>,
    memory_bytes: Option<u64>,
    uptime_seconds: Option<f64>,
    activity_state: String,
    os: String,
    config_initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetupProductOverview {
    installed: bool,
    connected: bool,
    service_running: bool,
    model_count: u32,
    install_mode: Option<String>,
    version: Option<String>,
    command_path: Option<String>,
    managed_by_warwolf: bool,
    node_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceControlResult {
    success: bool,
}

// ---------------------------------------------------------------------------
// Original command
// ---------------------------------------------------------------------------

#[tauri::command]
fn desktop_api_base() -> String {
    env::var("OPEN_CLAUDE_CODE_DESKTOP_API_BASE")
        .unwrap_or_else(|_| "http://127.0.0.1:4357".to_string())
}

// ---------------------------------------------------------------------------
// Agent pipeline commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn agent_pipeline_start(
    store: State<'_, PipelineStore>,
    agent_id: String,
    action: String,
) -> Result<pipeline::AgentPipelineStatus, String> {
    if agent_id != "openclaw" {
        return Err(format!("Unknown agent: {}", agent_id));
    }

    let arc = store.arc_handle();

    match action.as_str() {
        "install" => {
            let s = arc.clone();
            tokio::spawn(async move {
                agents::openclaw_install::run_install_flow(s).await;
            });
        }
        "start" => {
            let s = arc.clone();
            tokio::spawn(async move {
                agents::openclaw_start::run_start_flow(s).await;
            });
        }
        "uninstall" => {
            let s = arc.clone();
            tokio::spawn(async move {
                agents::openclaw_lifecycle::run_uninstall_flow(s).await;
            });
        }
        _ => return Err(format!("Unknown action: {}", action)),
    }

    // Wait a moment for the pipeline to initialize
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let run_key = format!("{}:{}", agent_id, action);
    match store.get(&run_key).await {
        Some(status) => Ok(status),
        None => Ok(pipeline::AgentPipelineStatus::new_pending(&agent_id, &action)),
    }
}

#[tauri::command]
async fn agent_pipeline_status(
    store: State<'_, PipelineStore>,
    agent_id: String,
    action: String,
) -> Result<pipeline::AgentPipelineStatus, String> {
    let run_key = format!("{}:{}", agent_id, action);
    match store.get(&run_key).await {
        Some(status) => Ok(status),
        None => Ok(pipeline::AgentPipelineStatus::new_pending(&agent_id, &action)),
    }
}

// ---------------------------------------------------------------------------
// OpenClaw status commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn openclaw_connect_status() -> Result<OpenclawConnectStatus, String> {
    let binary_path = agents::openclaw_cli::find_openclaw_binary();
    let version = binary_path
        .as_ref()
        .and_then(|p| agents::openclaw_cli::get_openclaw_version(p));
    let node_version = agents::openclaw_cli::get_node_version();
    let installed = binary_path.is_some() && version.is_some();

    // Read install state for install_mode
    let install_state = read_install_state();
    let install_mode = install_state
        .as_ref()
        .and_then(|s| s.get("install_mode"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let managed = install_state
        .as_ref()
        .and_then(|s| s.get("managed_by_warwolf"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let error = if binary_path.is_some() && !installed {
        Some("command health check failed".to_string())
    } else {
        None
    };

    Ok(OpenclawConnectStatus {
        connected: false,
        installed,
        command_path: binary_path,
        version,
        install_mode,
        managed_by_warwolf: managed,
        node_version,
        provider_exists: false,
        model_count: 0,
        error,
    })
}

#[tauri::command]
async fn openclaw_runtime_snapshot() -> Result<OpenclawRuntimeSnapshot, String> {
    // Check if gateway is running by probing port 18790
    let running = match reqwest::get("http://127.0.0.1:18790").await {
        Ok(resp) => resp.status().as_u16() < 500,
        Err(_) => false,
    };

    Ok(OpenclawRuntimeSnapshot {
        running,
        pid: None,       // TODO: detect PID from process list
        memory_bytes: None,
        uptime_seconds: None,
        activity_state: if running { "idle".to_string() } else { "unknown".to_string() },
        os: std::env::consts::OS.to_string(),
        config_initialized: false,
    })
}

#[tauri::command]
async fn openclaw_setup_overview() -> Result<SetupProductOverview, String> {
    let connect = openclaw_connect_status().await?;
    let runtime = openclaw_runtime_snapshot().await?;

    Ok(SetupProductOverview {
        installed: connect.installed,
        connected: connect.connected,
        service_running: runtime.running,
        model_count: connect.model_count,
        install_mode: connect.install_mode,
        version: connect.version,
        command_path: connect.command_path,
        managed_by_warwolf: connect.managed_by_warwolf,
        node_version: connect.node_version,
    })
}

#[tauri::command]
async fn openclaw_service_control(action: String) -> Result<ServiceControlResult, String> {
    match action.as_str() {
        "stop" => {
            agents::openclaw_lifecycle::stop_service()
                .map(|_| ServiceControlResult { success: true })
        }
        _ => Err(format!("Unknown action: {}", action)),
    }
}

#[tauri::command]
async fn open_dashboard_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

// ---------------------------------------------------------------------------
// Simplified OpenClaw commands (cherry-studio compatible)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenclawInstallCheck {
    installed: bool,
    path: Option<String>,
    #[serde(rename = "needsMigration")]
    needs_migration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenclawGatewayStatusResult {
    status: String,  // "stopped" | "running"
    port: u16,
}

/// Check installation matching cherry-studio's checkInstalled():
/// - Managed binary (~/.warwolf/bin/openclaw) → installed: true
/// - Found in PATH only → needsMigration: true (old npm install)
/// - Not found → installed: false
#[tauri::command]
async fn openclaw_check_installed() -> Result<OpenclawInstallCheck, String> {
    let (installed, path, needs_migration) = agents::openclaw_cli::check_installed();

    Ok(OpenclawInstallCheck {
        installed,
        path,
        needs_migration,
    })
}

const OPENCLAW_GATEWAY_PORT: u16 = 18790;

#[tauri::command]
async fn openclaw_get_status() -> Result<OpenclawGatewayStatusResult, String> {
    let url = format!("http://127.0.0.1:{}", OPENCLAW_GATEWAY_PORT);
    let running = match reqwest::get(&url).await {
        Ok(resp) => resp.status().as_u16() < 500,
        Err(_) => false,
    };

    Ok(OpenclawGatewayStatusResult {
        status: if running { "running".to_string() } else { "stopped".to_string() },
        port: OPENCLAW_GATEWAY_PORT,
    })
}

#[tauri::command]
async fn openclaw_get_dashboard_url() -> Result<String, String> {
    Ok(format!(
        "http://127.0.0.1:{}/chat?session=agent%3Amain%3Amain",
        OPENCLAW_GATEWAY_PORT
    ))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_install_state() -> Option<serde_json::Value> {
    let home = dirs::home_dir()?;
    let state_file = home.join(".warwolf").join("openclaw-install-state.json");
    let content = std::fs::read_to_string(state_file).ok()?;
    serde_json::from_str(&content).ok()
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .manage(PipelineStore::new())
        .invoke_handler(tauri::generate_handler![
            desktop_api_base,
            agent_pipeline_start,
            agent_pipeline_status,
            openclaw_connect_status,
            openclaw_runtime_snapshot,
            openclaw_setup_overview,
            openclaw_service_control,
            open_dashboard_url,
            openclaw_check_installed,
            openclaw_get_status,
            openclaw_get_dashboard_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenClaudeCode desktop shell");
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agents;
mod pipeline;

use std::env;
use std::collections::HashMap;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use pipeline::PipelineStore;
use tauri::State;

const DEFAULT_DESKTOP_API_BASE: &str = "http://127.0.0.1:4357";
const DEFAULT_DESKTOP_SERVER_ADDR: &str = "127.0.0.1:4357";
type DesktopServerHandle = Arc<Mutex<Option<Child>>>;

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
    desired_desktop_api_base()
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeToolsTerminalConfig {
    id: String,
    name: String,
    custom_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeToolSelectedModelPayload {
    provider_id: String,
    provider_name: String,
    provider_type: String,
    runtime_target: String,
    base_url: String,
    protocol: String,
    model_id: String,
    display_name: String,
    managed_provider_id: Option<String>,
    preset_id: Option<String>,
    has_stored_credential: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeToolsRunPayload {
    cli_tool: String,
    directory: String,
    terminal: String,
    auto_update_to_latest: bool,
    environment_variables: HashMap<String, String>,
    selected_model: Option<CodeToolSelectedModelPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeToolRunResult {
    success: bool,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderHubStore {
    providers: Vec<StoredManagedProviderSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredManagedProviderSecret {
    id: String,
    api_key: Option<String>,
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
// Code tools commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn is_binary_exist(binary_name: String) -> Result<bool, String> {
    Ok(binary_exists(&binary_name))
}

#[tauri::command]
async fn install_bun_binary() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "irm bun.sh/install.ps1 | iex",
            ])
            .status()
            .map_err(|error| format!("Failed to start Bun installer: {error}"))?;

        if status.success() {
            return Ok(());
        }

        return Err("Bun installer exited with a non-zero status".to_string());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("sh")
            .args(["-c", "curl -fsSL https://bun.sh/install | bash"])
            .status()
            .map_err(|error| format!("Failed to start Bun installer: {error}"))?;

        if status.success() {
            return Ok(());
        }

        Err("Bun installer exited with a non-zero status".to_string())
    }
}

#[tauri::command]
async fn code_tools_get_available_terminals() -> Result<Vec<CodeToolsTerminalConfig>, String> {
    Ok(detect_available_terminals())
}

#[tauri::command]
async fn code_tools_run(payload: CodeToolsRunPayload) -> Result<CodeToolRunResult, String> {
    let directory = PathBuf::from(&payload.directory);
    if !directory.exists() {
        return Ok(CodeToolRunResult {
            success: false,
            message: Some("工作目录不存在".to_string()),
        });
    }

    if !binary_exists("bun") {
        return Ok(CodeToolRunResult {
            success: false,
            message: Some("请先安装 Bun 环境".to_string()),
        });
    }

    let package_name = package_name_for_cli(&payload.cli_tool)?;
    let executable_name = executable_name_for_cli(&payload.cli_tool)?;
    let mut env_map = payload.environment_variables.clone();

    if let Some(selected_model) = &payload.selected_model {
        inject_model_environment(&payload.cli_tool, &mut env_map, selected_model);
        inject_managed_provider_api_key(&payload.cli_tool, &mut env_map, selected_model);
    }

    let shell_command = build_cli_shell_command(
        &payload.cli_tool,
        package_name,
        executable_name,
        payload.auto_update_to_latest,
        &env_map,
        &payload.directory,
        payload.selected_model.as_ref(),
    );

    spawn_code_tool_terminal(&payload.terminal, &shell_command, &payload.directory)?;

    Ok(CodeToolRunResult {
        success: true,
        message: Some("启动成功".to_string()),
    })
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

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn provider_hub_path() -> PathBuf {
    workspace_root().join("warwolf-provider-hub.json")
}

fn read_provider_hub_store() -> Option<ProviderHubStore> {
    let content = fs::read_to_string(provider_hub_path()).ok()?;
    serde_json::from_str(&content).ok()
}

fn binary_exists(binary_name: &str) -> bool {
    which_binary(binary_name).is_some()
}

fn which_binary(binary_name: &str) -> Option<PathBuf> {
    let path_value = env::var_os("PATH")?;
    env::split_paths(&path_value).find_map(|dir| {
        let candidate = dir.join(binary_name);
        if candidate.exists() {
            return Some(candidate);
        }

        #[cfg(target_os = "windows")]
        {
            let exe_candidate = dir.join(format!("{binary_name}.exe"));
            if exe_candidate.exists() {
                return Some(exe_candidate);
            }
        }

        None
    })
}

fn detect_available_terminals() -> Vec<CodeToolsTerminalConfig> {
    #[cfg(target_os = "macos")]
    {
        let mut terminals = vec![CodeToolsTerminalConfig {
            id: "Terminal".to_string(),
            name: "Terminal".to_string(),
            custom_path: None,
        }];

        let iterm_path = PathBuf::from("/Applications/iTerm.app");
        if iterm_path.exists() {
            terminals.push(CodeToolsTerminalConfig {
                id: "iTerm2".to_string(),
                name: "iTerm2".to_string(),
                custom_path: None,
            });
        }

        return terminals;
    }

    #[cfg(target_os = "windows")]
    {
        let mut terminals = vec![
            CodeToolsTerminalConfig {
                id: "CMD".to_string(),
                name: "Command Prompt".to_string(),
                custom_path: None,
            },
            CodeToolsTerminalConfig {
                id: "PowerShell".to_string(),
                name: "PowerShell".to_string(),
                custom_path: None,
            },
        ];
        if binary_exists("wt") {
            terminals.push(CodeToolsTerminalConfig {
                id: "WindowsTerminal".to_string(),
                name: "Windows Terminal".to_string(),
                custom_path: None,
            });
        }
        return terminals;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        vec![CodeToolsTerminalConfig {
            id: "system".to_string(),
            name: "Terminal".to_string(),
            custom_path: None,
        }]
    }
}

fn package_name_for_cli(cli_tool: &str) -> Result<&'static str, String> {
    match cli_tool {
        "claude-code" => Ok("@anthropic-ai/claude-code"),
        "qwen-code" => Ok("@qwen-code/qwen-code"),
        "gemini-cli" => Ok("@google/gemini-cli"),
        "openai-codex" => Ok("@openai/codex"),
        "iflow-cli" => Ok("@iflow-ai/iflow-cli"),
        "github-copilot-cli" => Ok("@github/copilot"),
        "kimi-cli" => Ok("kimi-cli"),
        "opencode" => Ok("opencode-ai"),
        _ => Err(format!("Unsupported CLI tool: {cli_tool}")),
    }
}

fn executable_name_for_cli(cli_tool: &str) -> Result<&'static str, String> {
    match cli_tool {
        "claude-code" => Ok("claude"),
        "qwen-code" => Ok("qwen"),
        "gemini-cli" => Ok("gemini"),
        "openai-codex" => Ok("codex"),
        "iflow-cli" => Ok("iflow"),
        "github-copilot-cli" => Ok("copilot"),
        "kimi-cli" => Ok("kimi"),
        "opencode" => Ok("opencode"),
        _ => Err(format!("Unsupported CLI tool: {cli_tool}")),
    }
}

fn inject_model_environment(
    cli_tool: &str,
    env_map: &mut HashMap<String, String>,
    model: &CodeToolSelectedModelPayload,
) {
    if cli_tool == "openai-codex" {
        return;
    }

    match model.protocol.as_str() {
        "anthropic-messages" => {
            env_map.insert("ANTHROPIC_BASE_URL".to_string(), model.base_url.clone());
            env_map.insert("ANTHROPIC_MODEL".to_string(), model.model_id.clone());
        }
        "gemini" => {
            env_map.insert("GEMINI_BASE_URL".to_string(), model.base_url.clone());
            env_map.insert(
                "GOOGLE_GEMINI_BASE_URL".to_string(),
                model.base_url.clone(),
            );
            env_map.insert("GEMINI_MODEL".to_string(), model.model_id.clone());
        }
        "openai-responses" => {
            env_map.insert("OPENAI_BASE_URL".to_string(), model.base_url.clone());
            env_map.insert("OPENAI_MODEL".to_string(), model.model_id.clone());
            env_map.insert(
                "OPENAI_MODEL_PROVIDER".to_string(),
                model.provider_id.clone(),
            );
            env_map.insert(
                "OPENAI_MODEL_PROVIDER_NAME".to_string(),
                model.provider_name.clone(),
            );
        }
        _ => {
            env_map.insert("OPENAI_BASE_URL".to_string(), model.base_url.clone());
            env_map.insert("OPENAI_MODEL".to_string(), model.model_id.clone());
            env_map.insert("IFLOW_BASE_URL".to_string(), model.base_url.clone());
            env_map.insert("IFLOW_MODEL_NAME".to_string(), model.model_id.clone());
            env_map.insert("KIMI_BASE_URL".to_string(), model.base_url.clone());
            env_map.insert("KIMI_MODEL_NAME".to_string(), model.model_id.clone());
            env_map.insert("OPENCODE_BASE_URL".to_string(), model.base_url.clone());
            env_map.insert(
                "OPENCODE_MODEL_NAME".to_string(),
                model.display_name.clone(),
            );
        }
    }
}

fn inject_managed_provider_api_key(
    cli_tool: &str,
    env_map: &mut HashMap<String, String>,
    model: &CodeToolSelectedModelPayload,
) {
    if cli_tool == "openai-codex" {
        return;
    }

    let Some(managed_provider_id) = &model.managed_provider_id else {
        return;
    };

    let Some(store) = read_provider_hub_store() else {
        return;
    };

    let Some(api_key) = store
        .providers
        .iter()
        .find(|provider| provider.id == *managed_provider_id)
        .and_then(|provider| provider.api_key.clone())
    else {
        return;
    };

    if api_key.trim().is_empty() {
        return;
    }

    match model.protocol.as_str() {
        "anthropic-messages" => {
            env_map
                .entry("ANTHROPIC_API_KEY".to_string())
                .or_insert(api_key);
        }
        "gemini" => {
            env_map.entry("GEMINI_API_KEY".to_string()).or_insert(api_key);
        }
        "openai-responses" | "openai-completions" => {
            env_map
                .entry("OPENAI_API_KEY".to_string())
                .or_insert(api_key.clone());
            env_map.entry("IFLOW_API_KEY".to_string()).or_insert(api_key.clone());
            env_map.entry("KIMI_API_KEY".to_string()).or_insert(api_key.clone());
        }
        _ => {}
    }
}

fn build_cli_shell_command(
    cli_tool: &str,
    package_name: &str,
    _executable_name: &str,
    auto_update_to_latest: bool,
    env_map: &HashMap<String, String>,
    directory: &str,
    selected_model: Option<&CodeToolSelectedModelPayload>,
) -> String {
    let env_reset_prefix = build_code_tool_env_reset_prefix(cli_tool, env_map);
    let exports = env_map
        .iter()
        .map(|(key, value)| format!("export {key}={};", shell_quote(value)))
        .collect::<Vec<_>>()
        .join(" ");
    let update_prefix = if auto_update_to_latest {
        format!(
            "bun add -g {package_name}@latest >/dev/null 2>&1 || true; "
        )
    } else {
        String::new()
    };
    let run_command = build_cli_run_command(cli_tool, package_name, selected_model);

    format!(
        "export PATH=\"$HOME/.bun/bin:$PATH\"; cd {}; {env_reset_prefix} {exports} {update_prefix}{run_command}; exit",
        shell_quote(directory)
    )
}

fn build_code_tool_env_reset_prefix(cli_tool: &str, env_map: &HashMap<String, String>) -> String {
    if cli_tool != "openai-codex" {
        return String::new();
    }

    // Codex CLI already manages its own ChatGPT/API-key auth in ~/.codex.
    // Clear inherited OPENAI_* vars unless the user explicitly set them in the form.
    [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "OPENAI_MODEL",
        "OPENAI_MODEL_PROVIDER",
        "OPENAI_MODEL_PROVIDER_NAME",
    ]
    .into_iter()
    .filter(|key| !env_map.contains_key(*key))
    .map(|key| format!("unset {key};"))
    .collect::<Vec<_>>()
    .join(" ")
}

fn build_cli_run_command(
    cli_tool: &str,
    package_name: &str,
    selected_model: Option<&CodeToolSelectedModelPayload>,
) -> String {
    match cli_tool {
        "openai-codex" => {
            let mut command = format!("bunx -y {package_name}");
            if let Some(model) = selected_model {
                command.push_str(" -m ");
                command.push_str(&shell_quote(&model.model_id));
            }
            command
        }
        _ => format!("bunx -y {package_name}"),
    }
}

fn spawn_code_tool_terminal(
    terminal: &str,
    shell_command: &str,
    _directory: &str,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let escaped_command = escape_for_applescript(shell_command);
        let script = if terminal == "iTerm2" && Path::new("/Applications/iTerm.app").exists() {
            format!(
                "tell application \"iTerm\" to create window with default profile command \"{escaped_command}\""
            )
        } else {
            format!(
                "tell application \"Terminal\" to activate\ntell application \"Terminal\" to do script \"{escaped_command}\""
            )
        };

        Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn()
            .map_err(|error| format!("Failed to open terminal: {error}"))?;

        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let status = if terminal == "WindowsTerminal" && binary_exists("wt") {
            Command::new("cmd")
                .args(["/C", "start", "wt", "-d", directory, "cmd", "/K", shell_command])
                .spawn()
        } else if terminal == "PowerShell" {
            Command::new("cmd")
                .args([
                    "/C",
                    "start",
                    "powershell",
                    "-NoExit",
                    "-Command",
                    shell_command,
                ])
                .spawn()
        } else {
            Command::new("cmd")
                .args(["/C", "start", "cmd", "/K", shell_command])
                .spawn()
        };

        status.map_err(|error| format!("Failed to open terminal: {error}"))?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Command::new("sh")
            .arg("-lc")
            .arg(shell_command)
            .current_dir(directory)
            .spawn()
            .map_err(|error| format!("Failed to start CLI command: {error}"))?;
        Ok(())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn escape_for_applescript(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
}

fn desired_desktop_api_base() -> String {
    env::var("OPEN_CLAUDE_CODE_DESKTOP_API_BASE")
        .unwrap_or_else(|_| DEFAULT_DESKTOP_API_BASE.to_string())
}

fn desired_desktop_server_addr() -> String {
    env::var("OPEN_CLAUDE_CODE_DESKTOP_ADDR")
        .unwrap_or_else(|_| DEFAULT_DESKTOP_SERVER_ADDR.to_string())
}

fn is_desktop_server_available(address: &str) -> bool {
    let socket = match address.parse::<SocketAddr>() {
        Ok(socket) => socket,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&socket, Duration::from_millis(200)).is_ok()
}

fn wait_for_desktop_server(address: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if is_desktop_server_available(address) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn desktop_server_binary_candidates(workspace_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![
        workspace_dir.join("target").join("debug").join("desktop-server"),
        workspace_dir.join("target").join("release").join("desktop-server"),
    ];

    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(exe_dir.join("desktop-server"));
            candidates.push(exe_dir.join("../Resources/desktop-server"));
            candidates.push(exe_dir.join("../Resources/bin/desktop-server"));
        }
    }

    candidates
}

fn spawn_desktop_server_process(address: &str) -> Result<Child, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.join("../../../rust");
    let mut command = if let Some(binary) = desktop_server_binary_candidates(&workspace_dir)
        .into_iter()
        .find(|candidate| candidate.exists())
    {
        let mut command = Command::new(binary);
        command.current_dir(&workspace_dir);
        command
    } else if cfg!(debug_assertions) {
        let mut command = Command::new("cargo");
        command
            .current_dir(&workspace_dir)
            .args(["run", "-p", "desktop-server"]);
        command
    } else {
        return Err(
            "Unable to locate desktop-server binary. Build desktop-server before launching Warwolf."
                .to_string(),
        );
    };

    command
        .env("OPEN_CLAUDE_CODE_DESKTOP_ADDR", address)
        .stdin(Stdio::null());

    if cfg!(debug_assertions) {
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    command
        .spawn()
        .map_err(|error| format!("Failed to launch desktop-server: {error}"))
}

fn shutdown_desktop_server(handle: &DesktopServerHandle) {
    let child = {
        let mut guard = handle.lock().expect("desktop server lock poisoned");
        guard.take()
    };

    if let Some(mut child) = child {
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

fn ensure_desktop_server(handle: &DesktopServerHandle) -> Result<(), String> {
    if env::var_os("OPEN_CLAUDE_CODE_DESKTOP_API_BASE").is_some() {
        return Ok(());
    }

    let address = desired_desktop_server_addr();
    if is_desktop_server_available(&address) {
        return Ok(());
    }

    let child = spawn_desktop_server_process(&address)?;
    {
        let mut guard = handle.lock().expect("desktop server lock poisoned");
        *guard = Some(child);
    }

    let timeout = if cfg!(debug_assertions) {
        Duration::from_secs(45)
    } else {
        Duration::from_secs(10)
    };

    if wait_for_desktop_server(&address, timeout) {
        return Ok(());
    }

    shutdown_desktop_server(handle);
    Err(format!(
        "desktop-server did not become ready at {address} before timeout"
    ))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let desktop_server = Arc::new(Mutex::new(None));
    if let Err(error) = ensure_desktop_server(&desktop_server) {
        eprintln!("failed to ensure desktop-server: {error}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            is_binary_exist,
            install_bun_binary,
            code_tools_get_available_terminals,
            code_tools_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenClaudeCode desktop shell");
}

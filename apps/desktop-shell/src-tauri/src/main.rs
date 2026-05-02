#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agents;
mod cc_switch_terminal;
mod pipeline;

use pipeline::PipelineStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::State;

const DEFAULT_DESKTOP_API_BASE: &str = "http://127.0.0.1:4357";
const DEFAULT_DESKTOP_SERVER_ADDR: &str = "127.0.0.1:4357";
const REQUIRED_DESKTOP_SERVER_ROUTE: &str = "/api/desktop/wechat-kefu/pipeline/status";
const CLAUDE_CODE_CLI_TOOL: &str = "claude-code";
const OPENAI_CODEX_CLI_TOOL: &str = "openai-codex";
const CODEX_OPENAI_PROVIDER_ID: &str = "codex-openai";
const QWEN_CODE_PROVIDER_ID: &str = "qwen-code";
/// Max time we wait for the desktop-server to honour a graceful
/// shutdown POST before we fall through to `Child::kill`. Tuned small
/// because the window-close path is user-visible: if the server is
/// genuinely wedged, force-killing still beats making the user stare
/// at an unresponsive desktop.
const SHUTDOWN_POST_TIMEOUT: Duration = Duration::from_millis(1_500);
/// Max time we wait for the child process to actually exit after the
/// POST succeeds. `SessionCleanupGuard::drop` + axum graceful drain
/// typically finish in well under a second; this bound is a backstop.
const SHUTDOWN_WAIT_TIMEOUT: Duration = Duration::from_secs(3);
type DesktopServerHandle = Arc<Mutex<Option<Child>>>;

/// Shared per-session secret the Tauri parent and desktop-server child
/// use to authenticate the `POST /internal/shutdown` call. Generated
/// once at `main()` startup, injected into the child via env var
/// (`OCL_SHUTDOWN_TOKEN`), and stashed in Tauri's managed state so the
/// window-close / exit-requested handlers can read it back.
#[derive(Clone)]
struct ShutdownToken(Arc<String>);

impl ShutdownToken {
    fn new() -> Self {
        Self(Arc::new(uuid::Uuid::new_v4().to_string()))
    }
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

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

#[tauri::command]
fn desktop_server_ensure(
    handle: State<'_, DesktopServerHandle>,
    token: State<'_, ShutdownToken>,
) -> Result<String, String> {
    ensure_desktop_server(handle.inner(), token.inner())?;
    Ok(desired_desktop_api_base())
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
        None => Ok(pipeline::AgentPipelineStatus::new_pending(
            &agent_id, &action,
        )),
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
        None => Ok(pipeline::AgentPipelineStatus::new_pending(
            &agent_id, &action,
        )),
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
        pid: None, // TODO: detect PID from process list
        memory_bytes: None,
        uptime_seconds: None,
        activity_state: if running {
            "idle".to_string()
        } else {
            "unknown".to_string()
        },
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
        "stop" => agents::openclaw_lifecycle::stop_service()
            .map(|_| ServiceControlResult { success: true }),
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
    status: String, // "stopped" | "running"
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
    base_url: String,
    protocol: String,
    model_id: String,
    display_name: String,
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
#[serde(rename_all = "camelCase")]
struct CodeToolLaunchProfileRequest {
    cli_tool: String,
    provider_id: String,
    model_id: String,
    desktop_api_base: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeToolLaunchProfilePayload {
    environment_variables: HashMap<String, String>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeToolLaunchProfileResponse {
    launch_profile: CodeToolLaunchProfilePayload,
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
        status: if running {
            "running".to_string()
        } else {
            "stopped".to_string()
        },
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
            .args(["-NoProfile", "-Command", "irm bun.sh/install.ps1 | iex"])
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
async fn code_tools_run(
    handle: State<'_, DesktopServerHandle>,
    token: State<'_, ShutdownToken>,
    payload: CodeToolsRunPayload,
) -> Result<CodeToolRunResult, String> {
    let directory = PathBuf::from(&payload.directory);
    if !directory.exists() {
        return Ok(CodeToolRunResult {
            success: false,
            message: Some("工作目录不存在".to_string()),
        });
    }

    let installed_binary = installed_binary_for_cli(&payload.cli_tool);

    if installed_binary.is_none() && !binary_exists("bun") {
        return Ok(CodeToolRunResult {
            success: false,
            message: Some("请先安装 Bun 环境".to_string()),
        });
    }

    ensure_desktop_server(handle.inner(), token.inner())?;

    let package_name = package_name_for_cli(&payload.cli_tool)?;
    let desktop_api_base = desired_desktop_api_base();
    let mut env_map = payload.environment_variables.clone();

    if tool_requires_model(&payload.cli_tool) && payload.selected_model.is_none() {
        return Ok(CodeToolRunResult {
            success: false,
            message: Some("请选择模型".to_string()),
        });
    }

    if tool_requires_model(&payload.cli_tool) {
        let selected_model = payload
            .selected_model
            .as_ref()
            .ok_or_else(|| "请选择模型".to_string())?;

        if !selected_model.has_stored_credential {
            return Ok(CodeToolRunResult {
                success: false,
                message: Some("所选模型服务尚未连接账号，请先在模型服务页完成 OAuth 登录。".to_string()),
            });
        }

        let selected_provider_matches_tool =
            tool_supports_provider(&payload.cli_tool, &selected_model.provider_id);

        if !selected_provider_matches_tool {
            return Ok(CodeToolRunResult {
                success: false,
                message: Some(match payload.cli_tool.as_str() {
                    OPENAI_CODEX_CLI_TOOL => {
                        "OpenAI Codex 当前仅支持 Codex OAuth 模型服务。".to_string()
                    }
                    _ => "当前代码工具只支持 Codex OAuth 或 Qwen OAuth 模型服务。".to_string(),
                }),
            });
        }

        match fetch_code_tool_launch_profile(
            &desktop_api_base,
            &payload.cli_tool,
            selected_model,
        )
        .await
        {
            Ok(launch_profile) => {
                env_map.extend(launch_profile.environment_variables);
            }
            Err(error) => {
                return Ok(CodeToolRunResult {
                    success: false,
                    message: Some(error),
                });
            }
        }
    }

    let shell_command = build_cli_shell_command(
        &payload.cli_tool,
        package_name,
        payload.auto_update_to_latest,
        &env_map,
        &payload.directory,
        payload.selected_model.as_ref(),
        installed_binary.as_deref(),
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

        if macos_app_exists("iTerm.app") {
            terminals.push(CodeToolsTerminalConfig {
                id: "iTerm2".to_string(),
                name: "iTerm2".to_string(),
                custom_path: None,
            });
        }

        if macos_app_exists("Ghostty.app") {
            terminals.push(CodeToolsTerminalConfig {
                id: "Ghostty".to_string(),
                name: "Ghostty".to_string(),
                custom_path: None,
            });
        }

        if macos_app_exists("kitty.app") {
            terminals.push(CodeToolsTerminalConfig {
                id: "kitty".to_string(),
                name: "kitty".to_string(),
                custom_path: None,
            });
        }

        if macos_app_exists("WezTerm.app") {
            terminals.push(CodeToolsTerminalConfig {
                id: "WezTerm".to_string(),
                name: "WezTerm".to_string(),
                custom_path: None,
            });
        }

        if macos_app_exists("Alacritty.app") {
            terminals.push(CodeToolsTerminalConfig {
                id: "Alacritty".to_string(),
                name: "Alacritty".to_string(),
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
        CLAUDE_CODE_CLI_TOOL => Ok("@anthropic-ai/claude-code"),
        OPENAI_CODEX_CLI_TOOL => Ok("@openai/codex"),
        _ => Err(format!("Unsupported CLI tool: {cli_tool}")),
    }
}

fn build_cli_shell_command(
    cli_tool: &str,
    package_name: &str,
    auto_update_to_latest: bool,
    env_map: &HashMap<String, String>,
    directory: &str,
    selected_model: Option<&CodeToolSelectedModelPayload>,
    installed_binary: Option<&str>,
) -> String {
    let env_reset_prefix = build_code_tool_env_reset_prefix(cli_tool, env_map, selected_model);
    let exports = env_map
        .iter()
        .map(|(key, value)| format!("export {key}={};", shell_quote(value)))
        .collect::<Vec<_>>()
        .join(" ");
    let update_prefix = if auto_update_to_latest && installed_binary.is_none() {
        format!("bun add -g {package_name}@latest >/dev/null 2>&1 || true; ")
    } else {
        String::new()
    };
    let run_command =
        build_cli_run_command(cli_tool, package_name, selected_model, installed_binary);

    format!(
        "export PATH=\"$HOME/.bun/bin:$PATH\"; cd {}; {env_reset_prefix} {exports} {update_prefix}{run_command}; exit",
        shell_quote(directory)
    )
}

fn build_code_tool_env_reset_prefix(
    cli_tool: &str,
    env_map: &HashMap<String, String>,
    _selected_model: Option<&CodeToolSelectedModelPayload>,
) -> String {
    let keys: &[&str] = match cli_tool {
        OPENAI_CODEX_CLI_TOOL => &[
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            "OPENAI_MODEL",
            "OPENAI_MODEL_PROVIDER",
            "OPENAI_MODEL_PROVIDER_NAME",
            "CODEX_HOME",
        ],
        CLAUDE_CODE_CLI_TOOL => &[
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
        ],
        _ => return String::new(),
    };

    keys.iter()
        .copied()
        .filter(|key| !env_map.contains_key(*key))
        .map(|key| format!("unset {key};"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_cli_run_command(
    cli_tool: &str,
    package_name: &str,
    selected_model: Option<&CodeToolSelectedModelPayload>,
    installed_binary: Option<&str>,
) -> String {
    if let Some(binary_path) = installed_binary {
        return build_installed_cli_run_command(cli_tool, binary_path, selected_model);
    }

    match cli_tool {
        OPENAI_CODEX_CLI_TOOL => {
            let mut command = format!("bunx -y {package_name}");
            if let Some(model) = selected_model {
                command.push_str(" -m ");
                command.push_str(&shell_quote(&model.model_id));
            }
            command
        }
        CLAUDE_CODE_CLI_TOOL => {
            let mut command = format!("bunx -y {package_name}");
            if let Some(model) = selected_model {
                command.push_str(" --model ");
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
    directory: &str,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return cc_switch_terminal::launch_terminal(
            terminal_target_for_ui(terminal),
            shell_command,
            Some(directory),
            None,
        );
    }

    #[cfg(target_os = "windows")]
    {
        let status = if terminal == "WindowsTerminal" && binary_exists("wt") {
            Command::new("cmd")
                .args([
                    "/C",
                    "start",
                    "wt",
                    "-d",
                    directory,
                    "cmd",
                    "/K",
                    shell_command,
                ])
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

fn desired_desktop_api_base() -> String {
    env::var("OPEN_CLAUDE_CODE_DESKTOP_API_BASE")
        .unwrap_or_else(|_| DEFAULT_DESKTOP_API_BASE.to_string())
}

fn tool_requires_model(cli_tool: &str) -> bool {
    matches!(cli_tool, CLAUDE_CODE_CLI_TOOL | OPENAI_CODEX_CLI_TOOL)
}

fn tool_supports_provider(cli_tool: &str, provider_id: &str) -> bool {
    match cli_tool {
        CLAUDE_CODE_CLI_TOOL => {
            matches!(provider_id, CODEX_OPENAI_PROVIDER_ID | QWEN_CODE_PROVIDER_ID)
        }
        OPENAI_CODEX_CLI_TOOL => provider_id == CODEX_OPENAI_PROVIDER_ID,
        _ => false,
    }
}

async fn fetch_code_tool_launch_profile(
    desktop_api_base: &str,
    cli_tool: &str,
    selected_model: &CodeToolSelectedModelPayload,
) -> Result<CodeToolLaunchProfilePayload, String> {
    let request = CodeToolLaunchProfileRequest {
        cli_tool: cli_tool.to_string(),
        provider_id: selected_model.provider_id.clone(),
        model_id: selected_model.model_id.clone(),
        desktop_api_base: desktop_api_base.to_string(),
    };
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/desktop/code-tools/launch-profile",
            desktop_api_base.trim_end_matches('/')
        ))
        .json(&request)
        .send()
        .await
        .map_err(|error| format!("获取代码工具启动配置失败: {error}"))?;

    if !response.status().is_success() {
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown launch profile error".to_string());
        return Err(format!("获取代码工具启动配置失败: {message}"));
    }

    response
        .json::<CodeToolLaunchProfileResponse>()
        .await
        .map(|payload| payload.launch_profile)
        .map_err(|error| format!("解析代码工具启动配置失败: {error}"))
}

fn build_installed_cli_run_command(
    cli_tool: &str,
    binary_path: &str,
    selected_model: Option<&CodeToolSelectedModelPayload>,
) -> String {
    let quoted_binary = shell_quote(binary_path);
    match cli_tool {
        CLAUDE_CODE_CLI_TOOL => {
            let mut command = quoted_binary;
            if let Some(model) = selected_model {
                command.push_str(" --model ");
                command.push_str(&shell_quote(&model.model_id));
            }
            command
        }
        OPENAI_CODEX_CLI_TOOL => {
            let mut command = quoted_binary;
            if let Some(model) = selected_model {
                command.push_str(" -m ");
                command.push_str(&shell_quote(&model.model_id));
            }
            command
        }
        _ => quoted_binary,
    }
}

fn installed_binary_for_cli(cli_tool: &str) -> Option<String> {
    match cli_tool {
        CLAUDE_CODE_CLI_TOOL => {
            which_binary("claude").map(|path| path.to_string_lossy().to_string())
        }
        OPENAI_CODEX_CLI_TOOL => {
            which_binary("codex").map(|path| path.to_string_lossy().to_string())
        }
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn terminal_target_for_ui(terminal: &str) -> &'static str {
    match terminal {
        "iTerm2" => "iterm",
        "Ghostty" => "ghostty",
        "kitty" => "kitty",
        "WezTerm" => "wezterm",
        "Alacritty" => "alacritty",
        _ => "terminal",
    }
}

#[cfg(target_os = "macos")]
fn macos_app_exists(app_name: &str) -> bool {
    let mut candidate_paths = vec![PathBuf::from("/Applications").join(app_name)];

    if let Some(home) = env::var_os("HOME") {
        candidate_paths.push(PathBuf::from(home).join("Applications").join(app_name));
    }

    candidate_paths.into_iter().any(|path| path.exists())
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

fn desktop_server_supports_required_route(address: &str) -> bool {
    let socket = match address.parse::<SocketAddr>() {
        Ok(socket) => socket,
        Err(_) => return false,
    };

    let mut stream = match TcpStream::connect_timeout(&socket, Duration::from_millis(500)) {
        Ok(stream) => stream,
        Err(_) => return false,
    };

    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));

    let request = format!(
        "GET {REQUIRED_DESKTOP_SERVER_ROUTE} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return false;
    }

    response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200")
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
    // Windows binaries always have the .exe suffix on disk; Tauri's sidecar
    // bundler renames desktop-server-<triple>.exe to desktop-server.exe in
    // the install dir. Candidate paths must include the suffix or
    // candidate.exists() returns false and the shell falls through to
    // "Unable to locate desktop-server binary" — even when the file is
    // sitting next to the Tauri shell .exe.
    let bin = if cfg!(windows) {
        "desktop-server.exe"
    } else {
        "desktop-server"
    };

    let mut candidates = vec![
        workspace_dir.join("target").join("debug").join(bin),
        workspace_dir.join("target").join("release").join(bin),
    ];

    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(exe_dir.join(bin));
            candidates.push(exe_dir.join("../Resources").join(bin));
            candidates.push(exe_dir.join("../Resources/bin").join(bin));
        }
    }

    candidates
}

fn spawn_desktop_server_process(address: &str, shutdown_token: &str) -> Result<Child, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.join("../../../rust");
    eprintln!("starting desktop-server for address {address}");
    let mut command = if cfg!(debug_assertions) {
        eprintln!(
            "debug mode: launching desktop-server via `cargo run -p desktop-server` so Rust changes rebuild on every app restart"
        );
        let mut command = Command::new("cargo");
        command
            .current_dir(&workspace_dir)
            .args(["run", "-p", "desktop-server"]);
        command
    } else if let Some(binary) = desktop_server_binary_candidates(&workspace_dir)
        .into_iter()
        .find(|candidate| candidate.exists())
    {
        eprintln!("using desktop-server binary {}", binary.display());
        let mut command = Command::new(&binary);
        // workspace_dir is baked at compile time and points at the GH
        // Actions runner's checkout (e.g. /home/runner/work/buddy/.../rust)
        // which does NOT exist on a real user's machine. Setting cwd to
        // it makes spawn() fail with "system cannot find the path
        // specified" — the desktop-server child never starts and the
        // UI hangs on "Failed to fetch". Use the binary's own parent
        // dir (the install dir on Windows / Resources on macOS) as cwd:
        // it always exists post-install and gives the server a sensible
        // working directory to write logs and resolve relative paths.
        if let Some(parent) = binary.parent() {
            command.current_dir(parent);
        }
        command
    } else {
        return Err(
            "Unable to locate desktop-server binary. Build desktop-server before launching Warwolf."
                .to_string(),
        );
    };

    command
        .env("OPEN_CLAUDE_CODE_DESKTOP_ADDR", address)
        // Graceful-shutdown auth token: only this Tauri parent knows
        // it, so only this parent can hit POST /internal/shutdown.
        // Any re-spawn during `ensure_desktop_server` rotations picks
        // up the same token via this env var.
        .env("OCL_SHUTDOWN_TOKEN", shutdown_token)
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

fn terminate_desktop_server_listeners(address: &str) {
    let port = match address.parse::<SocketAddr>() {
        Ok(socket) => socket.port(),
        Err(_) => return,
    };

    let output = match Command::new("lsof")
        .args([
            "-nP",
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
            "-t",
        ])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            eprintln!("failed to inspect existing desktop-server listener: {error}");
            return;
        }
    };

    if !output.status.success() {
        return;
    }

    let pids = String::from_utf8_lossy(&output.stdout);
    for pid in pids.lines().filter_map(|line| line.trim().parse::<u32>().ok()) {
        let ps_output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output();
        let command_line = ps_output
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_default();
        if !command_line.contains("desktop-server") {
            eprintln!(
                "refusing to terminate non desktop-server listener on {address}: pid={pid} cmd={command_line}"
            );
            continue;
        }

        eprintln!("terminating stale desktop-server listener on {address}: pid={pid}");
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
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

fn ensure_desktop_server(
    handle: &DesktopServerHandle,
    shutdown_token: &ShutdownToken,
) -> Result<(), String> {
    if env::var_os("OPEN_CLAUDE_CODE_DESKTOP_API_BASE").is_some() {
        return Ok(());
    }

    let address = desired_desktop_server_addr();
    let mut server_available = is_desktop_server_available(&address);
    let has_owned_child = handle
        .lock()
        .expect("desktop server lock poisoned")
        .is_some();

    if cfg!(debug_assertions) && server_available && !has_owned_child {
        eprintln!(
            "debug mode: restarting existing desktop-server on {address} to pick up fresh Rust changes"
        );
        shutdown_desktop_server(handle);
        terminate_desktop_server_listeners(&address);
        std::thread::sleep(Duration::from_millis(250));
        server_available = false;
    }

    if server_available {
        if desktop_server_supports_required_route(&address) {
            return Ok(());
        }

        eprintln!(
            "desktop-server on {address} is reachable but missing required route {REQUIRED_DESKTOP_SERVER_ROUTE}; restarting"
        );
        shutdown_desktop_server(handle);
        terminate_desktop_server_listeners(&address);
        std::thread::sleep(Duration::from_millis(250));
    }

    let child = spawn_desktop_server_process(&address, shutdown_token.as_str())?;
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
        eprintln!("desktop-server ready at {address}");
        return Ok(());
    }

    shutdown_desktop_server(handle);
    Err(format!(
        "desktop-server did not become ready at {address} before timeout"
    ))
}

/// Try a graceful shutdown of the desktop-server child: POST
/// `/internal/shutdown` with the shared token; if it lands, wait up
/// to `SHUTDOWN_WAIT_TIMEOUT` for the child to actually exit; if
/// anything fails along the way, fall back to the pre-existing
/// `Child::kill` path. This is the new entry point on window close,
/// so it has to be robust and never hang the UI.
///
/// Runs synchronously on whatever thread Tauri hands us — we use
/// `reqwest::blocking` for that reason. The request has a 1.5s
/// connect+request timeout; the subsequent wait loop polls
/// `Child::try_wait` every 50ms.
fn graceful_shutdown_desktop_server(
    handle: &DesktopServerHandle,
    shutdown_token: &ShutdownToken,
) {
    // If we don't own the child (user launched with
    // `OPEN_CLAUDE_CODE_DESKTOP_API_BASE` pointing at an external
    // server), there's nothing to shut down here.
    let have_child = handle
        .lock()
        .expect("desktop server lock poisoned")
        .is_some();
    if !have_child {
        return;
    }

    let address = desired_desktop_server_addr();
    let url = format!("http://{address}/internal/shutdown");

    eprintln!("[shutdown] POST {url} (graceful)");
    let client_result = reqwest::blocking::Client::builder()
        .timeout(SHUTDOWN_POST_TIMEOUT)
        .build();
    let post_ok = match client_result {
        Ok(client) => match client
            .post(&url)
            .header("X-Shutdown-Token", shutdown_token.as_str())
            .send()
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    eprintln!("[shutdown] server accepted graceful shutdown ({status})");
                    true
                } else {
                    eprintln!(
                        "[shutdown] server refused graceful shutdown ({status}); falling back to kill"
                    );
                    false
                }
            }
            Err(err) => {
                eprintln!("[shutdown] graceful POST failed ({err}); falling back to kill");
                false
            }
        },
        Err(err) => {
            eprintln!("[shutdown] could not build http client ({err}); falling back to kill");
            false
        }
    };

    if post_ok {
        // Wait for the child to actually exit so SessionCleanupGuard
        // drops + the axum drain both complete before we force-kill.
        let deadline = Instant::now() + SHUTDOWN_WAIT_TIMEOUT;
        loop {
            // Hold the lock only for the try_wait call, not across
            // the sleep.
            let exited = {
                let mut guard = handle.lock().expect("desktop server lock poisoned");
                match guard.as_mut() {
                    Some(child) => matches!(child.try_wait(), Ok(Some(_))),
                    None => true, // someone else took it — treat as exited
                }
            };
            if exited {
                eprintln!("[shutdown] child exited cleanly");
                // Reap: take the child out of the Option so Drop runs.
                let _ = handle.lock().expect("desktop server lock poisoned").take();
                return;
            }
            if Instant::now() >= deadline {
                eprintln!(
                    "[shutdown] child did not exit within {:?} after graceful POST; killing",
                    SHUTDOWN_WAIT_TIMEOUT
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    // Fallback: the pre-existing force-kill path. This is the same
    // behaviour we had before the graceful-shutdown work landed, so
    // nothing regresses if the new path is unavailable.
    shutdown_desktop_server(handle);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let desktop_server = Arc::new(Mutex::new(None));
    // Fresh per-launch secret. Both the Tauri parent (this process)
    // and the spawned desktop-server child need the same value — the
    // child reads it from its env, we keep a clone in managed state
    // so window-close handlers can read it back.
    let shutdown_token = ShutdownToken::new();
    if let Err(error) = ensure_desktop_server(&desktop_server, &shutdown_token) {
        eprintln!("failed to ensure desktop-server: {error}");
    }

    // Clones captured by the window-close / exit-requested closures.
    // These can't use Tauri's `State<'_, ...>` extractor because
    // `RunEvent` callbacks receive an `AppHandle`, not a command
    // context, so we pre-clone and move directly.
    let desktop_server_for_window = Arc::clone(&desktop_server);
    let token_for_window = shutdown_token.clone();
    let desktop_server_for_exit = Arc::clone(&desktop_server);
    let token_for_exit = shutdown_token.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::clone(&desktop_server))
        .manage(shutdown_token.clone())
        .manage(PipelineStore::new())
        // Window-close path. Fires when the user clicks the red
        // X / presses Alt-F4 / closes the last window on macOS.
        // We let Tauri proceed with the close AND fire a graceful
        // shutdown of the child in parallel — the child's drain
        // runs while the window teardown animation plays, which
        // masks the ~50-500ms tail latency from the user.
        //
        // Tauri 2: `on_window_event` takes an `FnMut(&Window, &WindowEvent)`.
        .on_window_event(move |_window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                graceful_shutdown_desktop_server(&desktop_server_for_window, &token_for_window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop_api_base,
            desktop_server_ensure,
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
        .build(tauri::generate_context!())
        .expect("error while building OpenClaudeCode desktop shell")
        .run(move |_app_handle, event| {
            // macOS Cmd-Q and the "all windows closed" path go here
            // instead of (or in addition to) on_window_event. Handle
            // it idempotently — graceful_shutdown_desktop_server
            // takes the child out of the Option on success, so the
            // window-close variant above will be a no-op if the exit
            // path fired first (or vice versa).
            if let tauri::RunEvent::ExitRequested { .. } = event {
                graceful_shutdown_desktop_server(&desktop_server_for_exit, &token_for_exit);
            }
        });
}

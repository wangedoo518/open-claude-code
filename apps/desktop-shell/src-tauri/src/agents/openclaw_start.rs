//! OpenClaw start pipeline.
//!
//! Flow:
//! 1. Resolve binary path from install state
//! 2. Kill any existing openclaw gateway processes
//! 3. Launch: openclaw gateway run --port 18790 --allow-unconfigured --auth none
//! 4. Probe: poll http://127.0.0.1:18790 until 200 or timeout

use crate::agents::openclaw_cli;
use crate::pipeline::AgentPipelineStatus;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

const RUN_KEY: &str = "openclaw:start";
const OPENCLAW_GATEWAY_PORT: u16 = 18790;
const DASHBOARD_URL: &str = "http://127.0.0.1:18790/chat?session=agent%3Amain%3Amain";
const PROBE_TIMEOUT_SECS: u64 = 90;
const PROBE_INTERVAL_MS: u64 = 2000;

/// Run the start flow as an async task.
pub async fn run_start_flow(store: Arc<RwLock<HashMap<String, AgentPipelineStatus>>>) {
    // Mark as running
    {
        let mut map = store.write().await;
        map.insert(
            RUN_KEY.to_string(),
            AgentPipelineStatus::new_running("openclaw", "start"),
        );
    }

    append_log(&store, "开始启动 OpenClaw 服务...").await;

    // Step 1: Resolve binary
    append_log(&store, "查找 OpenClaw 二进制路径...").await;
    let binary_path = match resolve_binary_path() {
        Some(path) => {
            append_log(&store, &format!("使用: {}", path)).await;
            path
        }
        None => {
            append_log(&store, "[stderr] 未找到 OpenClaw 二进制文件").await;
            set_hint(&store, "请先安装 OpenClaw").await;
            finish_failed(&store).await;
            return;
        }
    };

    // Step 2: Kill existing processes
    append_log(&store, "清理已有 OpenClaw 进程...").await;
    set_hint(&store, "正在清理旧进程...").await;
    kill_existing_gateway();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Step 3: Launch
    append_log(&store, &format!(
        "启动命令: {} gateway run --port {} --allow-unconfigured --auth none",
        binary_path, OPENCLAW_GATEWAY_PORT
    )).await;
    set_hint(&store, "正在启动 OpenClaw gateway...").await;

    let launch_result = launch_gateway(&binary_path);
    match launch_result {
        Ok(pid) => {
            append_log(&store, &format!("进程已启动, PID: {}", pid)).await;
        }
        Err(e) => {
            append_log(&store, &format!("[stderr] 启动失败: {}", e)).await;
            set_hint(&store, "启动 OpenClaw gateway 失败").await;
            finish_failed(&store).await;
            return;
        }
    }

    // Step 4: Probe for readiness
    append_log(&store, &format!(
        "等待服务就绪 (http://127.0.0.1:{})...",
        OPENCLAW_GATEWAY_PORT
    )).await;
    set_hint(&store, "等待 OpenClaw 服务就绪...").await;

    let probe_url = format!("http://127.0.0.1:{}", OPENCLAW_GATEWAY_PORT);
    let start_time = std::time::Instant::now();

    loop {
        if start_time.elapsed().as_secs() > PROBE_TIMEOUT_SECS {
            append_log(&store, &format!(
                "[stderr] 等待超时 ({}秒)",
                PROBE_TIMEOUT_SECS
            )).await;
            set_hint(&store, "服务启动超时").await;
            finish_failed(&store).await;
            return;
        }

        match reqwest::get(&probe_url).await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() < 500 => {
                append_log(&store, &format!(
                    "✓ 服务已就绪 (状态码: {})",
                    resp.status()
                )).await;
                set_hint(&store, "OpenClaw 服务已启动").await;
                finish_success(&store, Some(DASHBOARD_URL.to_string())).await;
                return;
            }
            Ok(resp) => {
                append_log(&store, &format!(
                    "等待中... (状态码: {})",
                    resp.status()
                )).await;
            }
            Err(_) => {
                // Connection refused — server not yet ready
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(PROBE_INTERVAL_MS)).await;
    }
}

/// Try to find the openclaw binary path from install state or system PATH.
fn resolve_binary_path() -> Option<String> {
    // First: try install state file
    if let Some(home) = dirs::home_dir() {
        let state_file = home.join(".warwolf").join("openclaw-install-state.json");
        if let Ok(content) = std::fs::read_to_string(&state_file) {
            if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(path) = state.get("binary_path").and_then(|v| v.as_str()) {
                    if std::path::Path::new(path).exists() {
                        return Some(path.to_string());
                    }
                }
            }
        }
    }

    // Fallback: system PATH
    openclaw_cli::find_openclaw_binary()
}

/// Kill existing openclaw gateway processes.
fn kill_existing_gateway() {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "openclaw gateway"])
            .output();
    }

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "openclaw.exe"])
            .output();
    }
}

/// Launch the openclaw gateway as a background process.
/// Returns the PID on success.
fn launch_gateway(binary_path: &str) -> Result<u32, String> {
    #[cfg(unix)]
    {
        use std::process::{Command, Stdio};

        // Create log directory
        if let Some(home) = dirs::home_dir() {
            let _ = std::fs::create_dir_all(home.join(".warwolf"));
        }

        let log_path = dirs::home_dir()
            .map(|h| h.join(".warwolf").join("openclaw-gateway.log"))
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp/openclaw-gateway.log"));

        let log_file = std::fs::File::create(&log_path)
            .map_err(|e| format!("Cannot create log file: {}", e))?;
        let log_file_err = log_file
            .try_clone()
            .map_err(|e| format!("Cannot clone log file: {}", e))?;

        let child = Command::new(binary_path)
            .args([
                "gateway",
                "run",
                "--port",
                &OPENCLAW_GATEWAY_PORT.to_string(),
                "--allow-unconfigured",
                "--auth",
                "none",
            ])
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file_err))
            .spawn()
            .map_err(|e| format!("Failed to spawn: {}", e))?;

        Ok(child.id())
    }

    #[cfg(windows)]
    {
        use std::process::{Command, Stdio};

        let child = Command::new(binary_path)
            .args([
                "gateway",
                "run",
                "--port",
                &OPENCLAW_GATEWAY_PORT.to_string(),
                "--allow-unconfigured",
                "--auth",
                "none",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn: {}", e))?;

        Ok(child.id())
    }
}

// -- Helpers --

async fn append_log(store: &Arc<RwLock<HashMap<String, AgentPipelineStatus>>>, line: &str) {
    let mut map = store.write().await;
    if let Some(status) = map.get_mut(RUN_KEY) {
        status.logs.push(line.to_string());
        status.updated_at_epoch = epoch_secs();
    }
}

async fn set_hint(store: &Arc<RwLock<HashMap<String, AgentPipelineStatus>>>, hint: &str) {
    let mut map = store.write().await;
    if let Some(status) = map.get_mut(RUN_KEY) {
        status.hint = Some(hint.to_string());
        status.updated_at_epoch = epoch_secs();
    }
}

async fn finish_success(store: &Arc<RwLock<HashMap<String, AgentPipelineStatus>>>, dashboard_url: Option<String>) {
    let mut map = store.write().await;
    if let Some(status) = map.get_mut(RUN_KEY) {
        status.running = false;
        status.finished = true;
        status.success = true;
        status.dashboard_url = dashboard_url;
        status.updated_at_epoch = epoch_secs();
    }
}

async fn finish_failed(store: &Arc<RwLock<HashMap<String, AgentPipelineStatus>>>) {
    let mut map = store.write().await;
    if let Some(status) = map.get_mut(RUN_KEY) {
        status.running = false;
        status.finished = true;
        status.success = false;
        status.updated_at_epoch = epoch_secs();
    }
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

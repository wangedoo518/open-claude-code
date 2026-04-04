//! OpenClaw install pipeline.
//!
//! Flow:
//! 1. Check for existing openclaw binary → reuse if healthy
//! 2. Check for npm → install via `npm install -g openclaw`
//! 3. Verify installation
//! 4. Write install state

use crate::agents::openclaw_cli;
use crate::pipeline::{AgentPipelineStatus, PipelineStore};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

const RUN_KEY: &str = "openclaw:install";

/// Run the install flow as an async task.
/// Updates the pipeline store with progress.
pub async fn run_install_flow(store: Arc<RwLock<HashMap<String, AgentPipelineStatus>>>) {
    // Mark as running
    {
        let mut map = store.write().await;
        map.insert(
            RUN_KEY.to_string(),
            AgentPipelineStatus::new_running("openclaw", "install"),
        );
    }

    append_log(&store, "开始安装 OpenClaw...").await;
    set_hint(&store, "正在检测系统环境...").await;

    // Step 1: Check for existing binary
    append_log(&store, "检查已有 OpenClaw 安装...").await;
    if let Some(path) = openclaw_cli::find_openclaw_binary() {
        append_log(&store, &format!("发现已有 OpenClaw: {}", path)).await;

        if openclaw_cli::health_check(&path) {
            let version = openclaw_cli::get_openclaw_version(&path)
                .unwrap_or_else(|| "unknown".to_string());
            append_log(&store, &format!("健康检查通过，版本: {}", version)).await;
            set_hint(&store, "复用系统已有 OpenClaw").await;

            // Save install state
            let node_version = openclaw_cli::get_node_version();
            if let Some(nv) = &node_version {
                append_log(&store, &format!("Node.js 版本: {}", nv)).await;
            }

            save_install_state(&path, &version, "reuse_existing", false, node_version.as_deref());
            finish_success(&store, None).await;
            return;
        } else {
            append_log(&store, "健康检查失败，将尝试重新安装").await;
        }
    } else {
        append_log(&store, "未发现已有 OpenClaw").await;
    }

    // Step 2: Check npm
    append_log(&store, "检查 npm 是否可用...").await;
    set_hint(&store, "正在通过 npm 安装 OpenClaw...").await;

    let npm_available = std::process::Command::new("npm")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !npm_available {
        append_log(&store, "[stderr] npm 不可用，请先安装 Node.js").await;
        set_hint(&store, "安装失败：npm 不可用").await;
        finish_failed(&store).await;
        return;
    }

    // Step 3: Install via npm
    append_log(&store, "运行: npm install -g openclaw").await;

    match std::process::Command::new("npm")
        .args(["install", "-g", "openclaw"])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            for line in stdout.lines() {
                if !line.trim().is_empty() {
                    append_log(&store, line).await;
                }
            }
            for line in stderr.lines() {
                if !line.trim().is_empty() {
                    append_log(&store, &format!("[stderr] {}", line)).await;
                }
            }

            if !output.status.success() {
                append_log(&store, "[stderr] npm install 失败").await;
                set_hint(&store, "npm install -g openclaw 执行失败").await;
                finish_failed(&store).await;
                return;
            }
        }
        Err(e) => {
            append_log(&store, &format!("[stderr] 执行 npm 失败: {}", e)).await;
            set_hint(&store, "无法执行 npm 命令").await;
            finish_failed(&store).await;
            return;
        }
    }

    // Step 4: Verify installation
    append_log(&store, "验证安装...").await;
    set_hint(&store, "正在验证 OpenClaw 安装...").await;

    if let Some(path) = openclaw_cli::find_openclaw_binary() {
        if openclaw_cli::health_check(&path) {
            let version = openclaw_cli::get_openclaw_version(&path)
                .unwrap_or_else(|| "unknown".to_string());
            let node_version = openclaw_cli::get_node_version();

            append_log(&store, &format!("✓ 安装成功: {} (v{})", path, version)).await;
            set_hint(&store, "OpenClaw 安装完成").await;

            save_install_state(&path, &version, "managed_native", true, node_version.as_deref());
            finish_success(&store, None).await;
            return;
        }
    }

    append_log(&store, "[stderr] 安装后验证失败").await;
    set_hint(&store, "安装完成但验证失败").await;
    finish_failed(&store).await;
}

/// Save install state to ~/.warwolf/openclaw-install-state.json
fn save_install_state(
    binary_path: &str,
    version: &str,
    install_mode: &str,
    managed: bool,
    node_version: Option<&str>,
) {
    if let Some(home) = dirs::home_dir() {
        let state_dir = home.join(".warwolf");
        let _ = std::fs::create_dir_all(&state_dir);
        let state_file = state_dir.join("openclaw-install-state.json");

        let state = serde_json::json!({
            "schema_version": 1,
            "binary_path": binary_path,
            "version": version,
            "install_mode": install_mode,
            "managed_by_warwolf": managed,
            "node_version": node_version,
            "installed_at": chrono_now(),
        });

        let _ = std::fs::write(state_file, serde_json::to_string_pretty(&state).unwrap_or_default());
    }
}

fn chrono_now() -> String {
    // Simple ISO-ish timestamp without chrono dependency
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", secs)
}

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

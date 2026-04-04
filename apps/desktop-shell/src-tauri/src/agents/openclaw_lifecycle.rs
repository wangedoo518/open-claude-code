//! OpenClaw lifecycle management — stop, uninstall.

use crate::agents::openclaw_cli;
use crate::pipeline::AgentPipelineStatus;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

const UNINSTALL_RUN_KEY: &str = "openclaw:uninstall";

/// Stop the OpenClaw gateway service.
pub fn stop_service() -> Result<(), String> {
    #[cfg(unix)]
    {
        // Use pkill -9 (SIGKILL) matching cherry-studio's approach
        let output = std::process::Command::new("pkill")
            .args(["-9", "openclaw"])
            .output()
            .map_err(|e| format!("Failed to run pkill: {}", e))?;

        if output.status.success() || output.status.code() == Some(1) {
            // Exit code 1 = no processes matched, which is fine
            Ok(())
        } else {
            Err(format!(
                "pkill failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    #[cfg(windows)]
    {
        let output = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "openclaw.exe"])
            .output()
            .map_err(|e| format!("Failed to run taskkill: {}", e))?;

        // taskkill returns non-zero if process not found, which is OK
        let _ = output;
        Ok(())
    }
}

/// Run the uninstall flow as an async task.
pub async fn run_uninstall_flow(store: Arc<RwLock<HashMap<String, AgentPipelineStatus>>>) {
    // Mark as running
    {
        let mut map = store.write().await;
        map.insert(
            UNINSTALL_RUN_KEY.to_string(),
            AgentPipelineStatus::new_running("openclaw", "uninstall"),
        );
    }

    append_log(&store, "开始卸载 OpenClaw...").await;

    // Step 1: Stop service first
    append_log(&store, "停止 OpenClaw 服务...").await;
    set_hint(&store, "正在停止服务...").await;
    if let Err(e) = stop_service() {
        append_log(&store, &format!("[stderr] 停止���务失败: {}", e)).await;
        // Continue anyway — service may not be running
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Step 2: Check install mode
    let install_state = read_install_state();
    let managed = install_state
        .as_ref()
        .and_then(|s| s.get("managed_by_warwolf"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if managed {
        // Managed install: run npm uninstall -g openclaw
        append_log(&store, "运行: npm uninstall -g openclaw").await;
        set_hint(&store, "正在通过 npm 卸载...").await;

        match std::process::Command::new("npm")
            .args(["uninstall", "-g", "openclaw"])
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
                    append_log(&store, "[stderr] npm uninstall 返回非零状态码").await;
                }
            }
            Err(e) => {
                append_log(&store, &format!("[stderr] 执行 npm 失败: {}", e)).await;
            }
        }
    } else {
        append_log(&store, "保留系统已有 OpenClaw 安装（非 Warwolf 托管）").await;
    }

    // Step 3: Remove install state
    if let Some(home) = dirs::home_dir() {
        let state_file = home.join(".warwolf").join("openclaw-install-state.json");
        if state_file.exists() {
            let _ = std::fs::remove_file(&state_file);
            append_log(&store, "已删除安装状态文件").await;
        }
    }

    // Step 4: Verify
    append_log(&store, "验证卸载...").await;
    if managed {
        if openclaw_cli::find_openclaw_binary().is_none() {
            append_log(&store, "✓ OpenClaw 已完全卸载").await;
        } else {
            append_log(&store, "⚠ OpenClaw 二进制文件仍存在").await;
        }
    }

    set_hint(&store, "OpenClaw 卸载完成").await;
    finish_success(&store).await;
}

fn read_install_state() -> Option<serde_json::Value> {
    let home = dirs::home_dir()?;
    let state_file = home.join(".warwolf").join("openclaw-install-state.json");
    let content = std::fs::read_to_string(state_file).ok()?;
    serde_json::from_str(&content).ok()
}

async fn append_log(store: &Arc<RwLock<HashMap<String, AgentPipelineStatus>>>, line: &str) {
    let mut map = store.write().await;
    if let Some(status) = map.get_mut(UNINSTALL_RUN_KEY) {
        status.logs.push(line.to_string());
        status.updated_at_epoch = epoch_secs();
    }
}

async fn set_hint(store: &Arc<RwLock<HashMap<String, AgentPipelineStatus>>>, hint: &str) {
    let mut map = store.write().await;
    if let Some(status) = map.get_mut(UNINSTALL_RUN_KEY) {
        status.hint = Some(hint.to_string());
        status.updated_at_epoch = epoch_secs();
    }
}

async fn finish_success(store: &Arc<RwLock<HashMap<String, AgentPipelineStatus>>>) {
    let mut map = store.write().await;
    if let Some(status) = map.get_mut(UNINSTALL_RUN_KEY) {
        status.running = false;
        status.finished = true;
        status.success = true;
        status.updated_at_epoch = epoch_secs();
    }
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

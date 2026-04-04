//! Pipeline status store — in-memory state for agent install/start/uninstall pipelines.
//!
//! Each pipeline run is identified by a `run_key` (e.g. "openclaw:install").
//! The store is a `RwLock<HashMap>` managed by Tauri.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPipelineStatus {
    pub run_key: String,
    pub agent_id: String,
    pub action: String,
    pub running: bool,
    pub finished: bool,
    pub success: bool,
    pub logs: Vec<String>,
    pub dashboard_url: Option<String>,
    pub hint: Option<String>,
    pub updated_at_epoch: u64,
}

impl AgentPipelineStatus {
    pub fn new_pending(agent_id: &str, action: &str) -> Self {
        Self {
            run_key: format!("{}:{}", agent_id, action),
            agent_id: agent_id.to_string(),
            action: action.to_string(),
            running: false,
            finished: false,
            success: false,
            logs: Vec::new(),
            dashboard_url: None,
            hint: None,
            updated_at_epoch: epoch_secs(),
        }
    }

    pub fn new_running(agent_id: &str, action: &str) -> Self {
        Self {
            run_key: format!("{}:{}", agent_id, action),
            agent_id: agent_id.to_string(),
            action: action.to_string(),
            running: true,
            finished: false,
            success: false,
            logs: Vec::new(),
            dashboard_url: None,
            hint: None,
            updated_at_epoch: epoch_secs(),
        }
    }
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Default)]
pub struct PipelineStore {
    statuses: Arc<RwLock<HashMap<String, AgentPipelineStatus>>>,
}

impl PipelineStore {
    pub fn new() -> Self {
        Self {
            statuses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, run_key: &str) -> Option<AgentPipelineStatus> {
        self.statuses.read().await.get(run_key).cloned()
    }

    pub async fn set(&self, status: AgentPipelineStatus) {
        self.statuses
            .write()
            .await
            .insert(status.run_key.clone(), status);
    }

    pub async fn append_log(&self, run_key: &str, line: String) {
        let mut map = self.statuses.write().await;
        if let Some(status) = map.get_mut(run_key) {
            status.logs.push(line);
            status.updated_at_epoch = epoch_secs();
        }
    }

    pub async fn set_hint(&self, run_key: &str, hint: String) {
        let mut map = self.statuses.write().await;
        if let Some(status) = map.get_mut(run_key) {
            status.hint = Some(hint);
            status.updated_at_epoch = epoch_secs();
        }
    }

    pub async fn finish(&self, run_key: &str, success: bool, dashboard_url: Option<String>) {
        let mut map = self.statuses.write().await;
        if let Some(status) = map.get_mut(run_key) {
            status.running = false;
            status.finished = true;
            status.success = success;
            status.dashboard_url = dashboard_url;
            status.updated_at_epoch = epoch_secs();
        }
    }

    /// Get a cloned Arc handle for spawned tasks.
    pub fn arc_handle(&self) -> Arc<RwLock<HashMap<String, AgentPipelineStatus>>> {
        self.statuses.clone()
    }
}

//! Pipeline state machine types for the one-scan kefu setup flow.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelinePhase {
    CfRegister,
    WorkerDeploy,
    WecomAuth,
    CallbackConfig,
    KefuCreate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Pending,
    Running,
    WaitingScan,
    Skipped,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseState {
    pub phase: PipelinePhase,
    pub status: PhaseStatus,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineState {
    pub phases: Vec<PhaseState>,
    #[serde(default)]
    pub logs: Vec<String>,
    #[serde(default)]
    pub current_phase: Option<PipelinePhase>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub contact_url: Option<String>,
    #[serde(default)]
    pub qr_data: Option<String>,
}

impl PipelineState {
    pub fn new() -> Self {
        Self {
            phases: vec![
                PhaseState {
                    phase: PipelinePhase::CfRegister,
                    status: PhaseStatus::Pending,
                    message: None,
                    error: None,
                },
                PhaseState {
                    phase: PipelinePhase::WorkerDeploy,
                    status: PhaseStatus::Pending,
                    message: None,
                    error: None,
                },
                PhaseState {
                    phase: PipelinePhase::WecomAuth,
                    status: PhaseStatus::Pending,
                    message: None,
                    error: None,
                },
                PhaseState {
                    phase: PipelinePhase::CallbackConfig,
                    status: PhaseStatus::Pending,
                    message: None,
                    error: None,
                },
                PhaseState {
                    phase: PipelinePhase::KefuCreate,
                    status: PhaseStatus::Pending,
                    message: None,
                    error: None,
                },
            ],
            logs: Vec::new(),
            current_phase: None,
            started_at: None,
            finished_at: None,
            contact_url: None,
            qr_data: None,
        }
    }

    pub fn update_phase(&mut self, phase: PipelinePhase, status: PhaseStatus, msg: Option<String>) {
        if let Some(p) = self.phases.iter_mut().find(|p| p.phase == phase) {
            p.status = status;
            if let Some(m) = msg {
                p.message = Some(m);
            }
            if status == PhaseStatus::Failed {
                p.error = p.message.clone();
            }
        }
        self.current_phase = Some(phase);
    }

    pub fn mark_failed(&mut self, phase: PipelinePhase, error: String) {
        if let Some(p) = self.phases.iter_mut().find(|p| p.phase == phase) {
            p.status = PhaseStatus::Failed;
            p.error = Some(error);
        }
        self.current_phase = Some(phase);
    }

    pub fn is_active(&self) -> bool {
        self.finished_at.is_none()
            && self
                .phases
                .iter()
                .any(|p| matches!(p.status, PhaseStatus::Running | PhaseStatus::WaitingScan))
    }
}

/// Cloudflare credentials collected during Phase 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfCredentials {
    pub email: String,
    pub password: String,
    pub api_token: String,
}

/// Result from Phase 2 (Worker deployment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub worker_url: String,
    pub callback_url: String,
    pub ws_url: String,
    pub auth_token: String,
    pub callback_token: String,
    pub encoding_aes_key: String,
}

/// Credentials from Phase 3 (WeCom authorization).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomCredentials {
    pub corpid: String,
    pub secret: String,
}

/// Final pipeline result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub open_kfid: String,
    pub contact_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("prerequisite: {0}")]
    Prerequisite(String),
    #[error("email: {0}")]
    Email(String),
    #[error("opencli: {0}")]
    OpenCli(String),
    #[error("deploy: {0}")]
    Deploy(String),
    #[error("wecom auth: {0}")]
    WecomAuth(String),
    #[error("callback config: {0}")]
    CallbackConfig(String),
    #[error("kefu api: {0}")]
    KefuApi(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("skipped")]
    Skipped,
    #[error("cancelled")]
    Cancelled,
}

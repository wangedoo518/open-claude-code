//! Official WeChat Customer Service API integration (Channel B).
//!
//! One-scan pipeline: CF register → Worker deploy → WeCom auth →
//! callback config → kefu account creation. All automated via OpenCLI.

pub mod account;
pub mod callback;
pub mod client;
pub mod deployer;
pub mod desktop_handler;
pub mod email_client;
pub mod monitor;
pub mod pipeline;
pub mod pipeline_types;
pub mod relay_client;
pub mod types;

pub use account::{load_config, save_config};
pub use callback::{CallbackEvent, KefuCallback};
pub use client::KefuClient;
pub use pipeline_types::{PipelineError, PipelinePhase, PipelineResult, PipelineState};
pub use types::{KefuConfig, KefuConfigSummary, KefuStatus};

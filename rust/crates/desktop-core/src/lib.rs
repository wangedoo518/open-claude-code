use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use api::{
    detect_provider_kind, max_tokens_for_model, read_base_url as read_claw_base_url,
    read_xai_base_url, resolve_model_alias, resolve_startup_auth_source, AnthropicClient,
    AuthSource, ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest,
    MessageResponse, OpenAiCompatClient, OpenAiCompatConfig, OutputContentBlock, ProviderClient,
    ProviderKind, StreamEvent as ApiStreamEvent, ToolChoice, ToolResultContentBlock,
};
use plugins::{PluginManager, PluginManagerConfig};
use runtime::{
    credentials_path, load_system_prompt, ApiClient as RuntimeApiClient, ApiRequest,
    AssistantEvent, ConfigLoader, ConfigSource, ContentBlock, ConversationMessage,
    ConversationRuntime, ManagedMcpTool, McpServerConfig, McpServerManager, MessageRole,
    PermissionMode, PermissionPolicy, ResolvedPermissionMode, RuntimeConfig, RuntimeError,
    RuntimeFeatureConfig, Session as RuntimeSession, SessionCompaction as RuntimeSessionCompaction,
    SessionFork as RuntimeSessionFork, TokenUsage, ToolError, ToolExecutor as RuntimeToolExecutor,
};
use serde::{Deserialize, Serialize};
use time::{
    macros::format_description, Duration as TimeDuration, OffsetDateTime, PrimitiveDateTime,
    UtcOffset,
};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tools::GlobalToolRegistry;

pub mod agentic_loop;
// A1 — Ask context engine. Owns `ContextMode`, `ContextBasis`,
// history trimming, and boundary-marker / enrich-prefix rendering.
// Centralises the per-turn context packaging that used to be implicit
// in `build_api_request` + `maybe_enrich_url`. See the module docstring
// for the mode semantics the frontend switches between.
pub mod ask_context;
pub mod attachments;
mod codex_auth;
// Optional private-cloud account pool. OSS builds keep this feature
// off by default and use the generic provider registry path instead.
#[cfg(feature = "private-cloud")]
pub mod codex_broker;
mod managed_auth;
mod oauth_runtime;
pub mod protocol_codegen;
// S0.4 cut day note: providers_config was removed but restored for
// generic compatible-gateway support. Users configure Anthropic-
// compatible and OpenAI-compatible providers via the settings UI.
pub mod providers_config;
// A5.1 — OpenAI-compat ChatCompletions streaming path. Used when
// providers.json active = openai_compat (Moonshot/Kimi/DeepSeek/Qwen/…)
// so the Ask UI sees real `TextDelta` events instead of a single
// "整段落下" Message event at the end of the turn.
pub mod openai_compat_streaming;
pub mod openai_tool_schema;
pub mod secure_storage;
pub mod system_prompt;
pub mod tool_execution;
// M2 runtime prerequisite classifier. Turns raw stderr / IngestError
// strings into a `MissingPrerequisite` enum with Chinese user-facing
// hints, used by `url_ingest` to structure `PrerequisiteMissing` and
// by the desktop-server env-check endpoints to stay consistent.
pub mod prerequisites;
// M2 URL ingest orchestrator. Centralises the previously-duplicated
// "fetch URL → validate → write raw → queue inbox" pipeline that used
// to live in three copies inside `maybe_enrich_url` and clones in
// `wechat_fetch_handler` + `wechat_ilink::ingest_wechat_text_to_wiki`.
// See `url_ingest/mod.rs` for the entry point + `IngestOutcome` enum.
pub mod url_ingest;
pub mod wechat_ilink;
pub mod wechat_kefu;
// ClawWiki S4: adapter bridging codex_broker::CodexBroker to
// wiki_maintainer::BrokerSender. Implements the maintainer's trait
// for a wrapper struct (orphan rule forbids impl on foreign types)
// so desktop-server's wiki routes can pass the process-global broker
// straight into propose_for_raw_entry.
pub mod absorb_task;
pub mod skill_router;
pub mod wiki_maintainer_adapter;

pub use ask_context::binding::{SessionSourceBinding, SourceRef};
pub use ask_context::{ContextBasis, ContextMode};
pub use codex_auth::{
    DesktopCodexAuthOverview, DesktopCodexAuthSource, DesktopCodexInstallationRecord,
    DesktopCodexLoginSessionSnapshot, DesktopCodexLoginSessionStatus, DesktopCodexProfileSummary,
};
pub use managed_auth::{
    DesktopCodeToolLaunchProfile, DesktopManagedAuthAccount, DesktopManagedAuthAccountStatus,
    DesktopManagedAuthLoginSessionSnapshot, DesktopManagedAuthLoginSessionStatus,
    DesktopManagedAuthProvider, DesktopManagedAuthProviderKind, DesktopManagedAuthRuntimeBinding,
    DesktopManagedAuthRuntimeClient, DesktopManagedAuthSource,
};
pub use oauth_runtime::{DesktopCodexLiveProvider, DesktopCodexRuntimeState, DesktopProviderModel};

pub type SessionId = String;

const BROADCAST_CAPACITY: usize = 64;
const DEFAULT_PROJECT_NAME: &str = "Warwolf";
/// Fallback project path (compile-time constant, ONLY used when
/// `default_project_path()` can't resolve the CWD — should never
/// happen in practice).
const DEFAULT_PROJECT_PATH_FALLBACK: &str = ".";

/// Runtime-resolved default project path. Uses `OnceLock` so the
/// current working directory is captured ONCE at first access and
/// reused for the lifetime of the process. This works on both
/// Windows (`D:\Users\111\...`) and Mac (`/Users/xxx/...`).
///
/// The original hardcoded Mac path (`/Users/champion/...`) was a
/// dev artifact that caused "os error 3" on Windows.
static RESOLVED_DEFAULT_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn default_project_path() -> &'static str {
    RESOLVED_DEFAULT_PATH.get_or_init(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| DEFAULT_PROJECT_PATH_FALLBACK.to_string())
    })
}

// NOTE: The old `const DEFAULT_PROJECT_PATH` was removed because all
// callsites now use `default_project_path()` (runtime-resolved via
// OnceLock). If you need a compile-time fallback, use
// `DEFAULT_PROJECT_PATH_FALLBACK` directly.
const DEFAULT_MODEL_ID: &str = "claude-opus-4-6";
const DEFAULT_MODEL_LABEL: &str = "Opus 4.6";
const DEFAULT_ENVIRONMENT_LABEL: &str = "Local";
const DEFAULT_PERMISSION_MODE_LABEL: &str = "Danger full access";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTabKind {
    Home,
    Search,
    Scheduled,
    Dispatch,
    Customize,
    OpenClaw,
    Settings,
    CodeSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopTopTab {
    pub id: String,
    pub label: String,
    pub kind: DesktopTabKind,
    pub closable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopLaunchpadItem {
    pub id: String,
    pub label: String,
    pub description: String,
    pub accent: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSettingsGroup {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopBootstrap {
    pub product_name: String,
    pub code_label: String,
    pub top_tabs: Vec<DesktopTopTab>,
    pub launchpad_items: Vec<DesktopLaunchpadItem>,
    pub settings_groups: Vec<DesktopSettingsGroup>,
    #[serde(default)]
    pub private_cloud_enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSessionBucket {
    Today,
    Yesterday,
    Older,
}

impl DesktopSessionBucket {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::Older => "Older",
        }
    }

    #[must_use]
    fn order(self) -> usize {
        match self {
            Self::Today => 0,
            Self::Yesterday => 1,
            Self::Older => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSidebarAction {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub target_tab_id: String,
    pub kind: DesktopTabKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionSummary {
    pub id: SessionId,
    pub title: String,
    pub preview: String,
    pub bucket: DesktopSessionBucket,
    pub created_at: u64,
    pub updated_at: u64,
    pub project_name: String,
    pub project_path: String,
    pub environment_label: String,
    pub model_label: String,
    pub turn_state: DesktopTurnState,
    #[serde(default = "default_lifecycle_status")]
    pub lifecycle_status: DesktopLifecycleStatus,
    #[serde(default = "default_flagged")]
    pub flagged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionSection {
    pub id: String,
    pub label: String,
    pub sessions: Vec<DesktopSessionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopUpdateBanner {
    pub version: String,
    pub cta_label: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopAccountCard {
    pub name: String,
    pub plan_label: String,
    pub shortcut_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopComposerState {
    pub permission_mode_label: String,
    pub environment_label: String,
    pub model_label: String,
    pub send_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopWorkbench {
    pub primary_actions: Vec<DesktopSidebarAction>,
    pub secondary_actions: Vec<DesktopSidebarAction>,
    pub project_label: String,
    pub project_name: String,
    pub session_sections: Vec<DesktopSessionSection>,
    pub active_session_id: Option<SessionId>,
    pub update_banner: DesktopUpdateBanner,
    pub account: DesktopAccountCard,
    pub composer: DesktopComposerState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionDetail {
    pub id: SessionId,
    pub title: String,
    pub preview: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub project_name: String,
    pub project_path: String,
    pub environment_label: String,
    pub model_label: String,
    pub turn_state: DesktopTurnState,
    #[serde(default = "default_lifecycle_status")]
    pub lifecycle_status: DesktopLifecycleStatus,
    #[serde(default = "default_flagged")]
    pub flagged: bool,
    pub session: DesktopSessionData,
    /// M2 side-channel: carries the outcome of the URL-enrichment
    /// pipeline (`url_ingest::ingest_url`) for the message that
    /// triggered this detail. Only populated by `append_user_message`
    /// in the success/failure/prerequisite paths; every other code
    /// site that builds a detail (idle refresh, rename, fork, …)
    /// leaves this `None`. Frontend consumers treat absence as
    /// "no URL was processed".
    ///
    /// Kept optional with `serde(default)` so older snapshots
    /// persisted to disk still deserialize cleanly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrich_status: Option<EnrichStatus>,
    /// A1 side-channel: carries how the backend packaged the user's
    /// most recent turn for the LLM (mode, history-kept count,
    /// boundary-marker-injected flag). Only populated by
    /// `append_user_message` on the turn that triggered it; every
    /// other detail builder leaves it `None`. Frontend uses it to
    /// render the `ContextBasisLabel` chip explaining which mode
    /// actually ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_basis: Option<ContextBasis>,
    /// A2: session-level pinned source. Echoed from
    /// `SessionMetadata.source_binding` on every `record.detail()`
    /// call — including list/get/SSE-snapshot paths — so the UI can
    /// render the binding chip without subscribing to extra events.
    /// `None` when no source is currently bound to the session.
    /// Legacy JSON without this field decodes to `None` via
    /// `#[serde(default)]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_binding: Option<crate::ask_context::binding::SessionSourceBinding>,
}

/// Side-channel status reported by `DesktopState::append_user_message`
/// when the user's message contained a URL. Surfaced to the frontend
/// via the returned `DesktopSessionDetail` so the UI can flag failed /
/// rejected enrichments without having to poll another endpoint.
///
/// See `crates/desktop-core/src/url_ingest/mod.rs` for the underlying
/// outcome classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EnrichStatus {
    /// URL fetched + written to raw. Frontend can show a "saved to
    /// Raw Library" confirmation if desired.
    Success { title: String, raw_id: u32 },
    /// M3: canonical URL matched an existing raw entry; we skipped
    /// the fetch and reused the prior landing. `reason` carries the
    /// dedupe decision tag (pending / approved / rejected / silent)
    /// so the UI can render an inline "已在素材库 #NNNNN" chip rather
    /// than a full "saved" banner.
    Reused {
        title: String,
        raw_id: u32,
        reason: String,
    },
    /// URL fetched but the body failed quality validation (anti-bot
    /// page, empty, image-only). Nothing persisted.
    RejectedQuality { reason: String },
    /// Network / adapter failure. Nothing persisted.
    FetchFailed { reason: String },
    /// Host-level dependency missing (e.g. Playwright not installed).
    /// The `hint` is safe to display verbatim as an install CTA.
    PrerequisiteMissing { dep: String, hint: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl From<MessageRole> for DesktopMessageRole {
    fn from(value: MessageRole) -> Self {
        match value {
            MessageRole::System => Self::System,
            MessageRole::User => Self::User,
            MessageRole::Assistant => Self::Assistant,
            MessageRole::Tool => Self::Tool,
        }
    }
}

impl From<DesktopMessageRole> for MessageRole {
    fn from(value: DesktopMessageRole) -> Self {
        match value {
            DesktopMessageRole::System => Self::System,
            DesktopMessageRole::User => Self::User,
            DesktopMessageRole::Assistant => Self::Assistant,
            DesktopMessageRole::Tool => Self::Tool,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        tool_use_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    },
}

impl From<&ContentBlock> for DesktopContentBlock {
    fn from(value: &ContentBlock) -> Self {
        match value {
            ContentBlock::Text { text } => Self::Text { text: text.clone() },
            ContentBlock::ToolUse { id, name, input } => Self::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => Self::ToolResult {
                tool_use_id: tool_use_id.clone(),
                tool_name: tool_name.clone(),
                output: output.clone(),
                is_error: *is_error,
            },
        }
    }
}

impl From<DesktopContentBlock> for ContentBlock {
    fn from(value: DesktopContentBlock) -> Self {
        match value {
            DesktopContentBlock::Text { text } => Self::Text { text },
            DesktopContentBlock::ToolUse { id, name, input } => Self::ToolUse { id, name, input },
            DesktopContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => Self::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopTokenUsageData {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

impl From<TokenUsage> for DesktopTokenUsageData {
    fn from(value: TokenUsage) -> Self {
        Self {
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            cache_creation_input_tokens: value.cache_creation_input_tokens,
            cache_read_input_tokens: value.cache_read_input_tokens,
        }
    }
}

impl From<DesktopTokenUsageData> for TokenUsage {
    fn from(value: DesktopTokenUsageData) -> Self {
        Self {
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            cache_creation_input_tokens: value.cache_creation_input_tokens,
            cache_read_input_tokens: value.cache_read_input_tokens,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopConversationMessage {
    pub role: DesktopMessageRole,
    pub blocks: Vec<DesktopContentBlock>,
    pub usage: Option<DesktopTokenUsageData>,
}

impl From<&ConversationMessage> for DesktopConversationMessage {
    fn from(value: &ConversationMessage) -> Self {
        Self {
            role: value.role.into(),
            blocks: value.blocks.iter().map(DesktopContentBlock::from).collect(),
            usage: value.usage.map(DesktopTokenUsageData::from),
        }
    }
}

impl From<DesktopConversationMessage> for ConversationMessage {
    fn from(value: DesktopConversationMessage) -> Self {
        Self {
            role: value.role.into(),
            blocks: value.blocks.into_iter().map(ContentBlock::from).collect(),
            usage: value.usage.map(TokenUsage::from),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionCompactionData {
    pub count: u32,
    pub removed_message_count: usize,
    pub summary: String,
}

impl From<&RuntimeSessionCompaction> for DesktopSessionCompactionData {
    fn from(value: &RuntimeSessionCompaction) -> Self {
        Self {
            count: value.count,
            removed_message_count: value.removed_message_count,
            summary: value.summary.clone(),
        }
    }
}

impl From<DesktopSessionCompactionData> for RuntimeSessionCompaction {
    fn from(value: DesktopSessionCompactionData) -> Self {
        Self {
            count: value.count,
            removed_message_count: value.removed_message_count,
            summary: value.summary,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionForkData {
    pub parent_session_id: String,
    pub branch_name: Option<String>,
}

impl From<&RuntimeSessionFork> for DesktopSessionForkData {
    fn from(value: &RuntimeSessionFork) -> Self {
        Self {
            parent_session_id: value.parent_session_id.clone(),
            branch_name: value.branch_name.clone(),
        }
    }
}

impl From<DesktopSessionForkData> for RuntimeSessionFork {
    fn from(value: DesktopSessionForkData) -> Self {
        Self {
            parent_session_id: value.parent_session_id,
            branch_name: value.branch_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionData {
    #[serde(default = "default_session_version")]
    pub version: u32,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default)]
    pub messages: Vec<DesktopConversationMessage>,
    #[serde(default)]
    pub compaction: Option<DesktopSessionCompactionData>,
    #[serde(default)]
    pub fork: Option<DesktopSessionForkData>,
}

impl From<&RuntimeSession> for DesktopSessionData {
    fn from(value: &RuntimeSession) -> Self {
        Self {
            version: value.version,
            session_id: value.session_id.clone(),
            created_at_ms: value.created_at_ms,
            updated_at_ms: value.updated_at_ms,
            messages: value
                .messages
                .iter()
                .map(DesktopConversationMessage::from)
                .collect(),
            compaction: value
                .compaction
                .as_ref()
                .map(DesktopSessionCompactionData::from),
            fork: value.fork.as_ref().map(DesktopSessionForkData::from),
        }
    }
}

fn default_session_version() -> u32 {
    1
}

impl DesktopSessionData {
    fn into_runtime_session_with_metadata(self, metadata: &SessionMetadata) -> RuntimeSession {
        let mut session = RuntimeSession::from(self);
        if session.session_id.is_empty() {
            session.session_id = metadata.id.clone();
        }
        if session.created_at_ms == 0 {
            session.created_at_ms = metadata.created_at;
        }
        if session.updated_at_ms == 0 {
            session.updated_at_ms = metadata.updated_at;
        }
        session
    }
}

impl From<DesktopSessionData> for RuntimeSession {
    fn from(value: DesktopSessionData) -> Self {
        let mut session = RuntimeSession::new();
        session.version = value.version;
        session.session_id = value.session_id;
        session.created_at_ms = value.created_at_ms;
        session.updated_at_ms = value.updated_at_ms;
        session.messages = value
            .messages
            .into_iter()
            .map(ConversationMessage::from)
            .collect();
        session.compaction = value.compaction.map(RuntimeSessionCompaction::from);
        session.fork = value.fork.map(RuntimeSessionFork::from);
        session
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTurnState {
    Idle,
    Running,
}

fn default_turn_state() -> DesktopTurnState {
    DesktopTurnState::Idle
}

/// Session lifecycle / workflow status, independent of `turn_state`.
///
/// `turn_state` is "is the LLM processing this session right now?"
/// `lifecycle_status` is "what's my relationship to this session?"
///
/// Inspired by craft-agents-oss's inbox model (Todo → InProgress →
/// NeedsReview → Done). Lets users manage multiple concurrent agent
/// tasks without forgetting which ones need attention.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopLifecycleStatus {
    /// Not yet started working on it.
    Todo,
    /// Currently working (either LLM running or user actively reading).
    InProgress,
    /// Agent finished; user should review the output.
    NeedsReview,
    /// User confirmed the work is complete.
    Done,
    /// Archived out of the main inbox view.
    Archived,
}

fn default_lifecycle_status() -> DesktopLifecycleStatus {
    DesktopLifecycleStatus::InProgress
}

fn default_flagged() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSearchHit {
    pub session_id: SessionId,
    pub title: String,
    pub project_name: String,
    pub project_path: String,
    pub bucket: DesktopSessionBucket,
    pub preview: String,
    pub snippet: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCustomizeSummary {
    pub loaded_config_count: usize,
    pub mcp_server_count: usize,
    pub plugin_count: usize,
    pub enabled_plugin_count: usize,
    pub plugin_tool_count: usize,
    pub pre_tool_hook_count: usize,
    pub post_tool_hook_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCustomizeState {
    pub project_path: String,
    pub model_id: String,
    pub model_label: String,
    pub permission_mode: String,
    pub summary: DesktopCustomizeSummary,
    pub loaded_configs: Vec<DesktopConfigFile>,
    pub hooks: DesktopHookConfigView,
    pub mcp_servers: Vec<DesktopMcpServer>,
    pub plugins: Vec<DesktopPluginView>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSettingsState {
    pub project_path: String,
    pub config_home: String,
    pub desktop_session_store_path: String,
    pub oauth_credentials_path: Option<String>,
    pub providers: Vec<DesktopProviderSetting>,
    pub storage_locations: Vec<DesktopStorageLocation>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderSetting {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub auth_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopStorageLocation {
    pub label: String,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopScheduledState {
    pub project_path: String,
    pub summary: DesktopScheduledSummary,
    pub tasks: Vec<DesktopScheduledTask>,
    pub trusted_project_paths: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopScheduledSummary {
    pub total_task_count: usize,
    pub enabled_task_count: usize,
    pub running_task_count: usize,
    pub blocked_task_count: usize,
    pub due_task_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopScheduledTask {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub project_name: String,
    pub project_path: String,
    pub schedule: DesktopScheduledSchedule,
    pub schedule_label: String,
    pub target: DesktopScheduledTaskTarget,
    pub enabled: bool,
    pub blocked_reason: Option<String>,
    pub status: DesktopScheduledTaskStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_run_at: Option<u64>,
    pub next_run_at: Option<u64>,
    pub last_run_status: Option<DesktopScheduledRunStatus>,
    pub last_outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDispatchState {
    pub project_path: String,
    pub summary: DesktopDispatchSummary,
    pub items: Vec<DesktopDispatchItem>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDispatchSummary {
    pub total_item_count: usize,
    pub unread_item_count: usize,
    pub pending_item_count: usize,
    pub delivered_item_count: usize,
    pub archived_item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDispatchItem {
    pub id: String,
    pub title: String,
    pub body: String,
    pub project_name: String,
    pub project_path: String,
    pub source: DesktopDispatchSource,
    pub priority: DesktopDispatchPriority,
    pub target: DesktopDispatchTarget,
    pub status: DesktopDispatchStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub delivered_at: Option<u64>,
    pub last_outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDispatchSource {
    pub kind: DesktopDispatchSourceKind,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDispatchTarget {
    pub kind: DesktopDispatchTargetKind,
    pub session_id: Option<SessionId>,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopDispatchSourceKind {
    LocalInbox,
    RemoteBridge,
    Scheduled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopDispatchTargetKind {
    NewSession,
    ExistingSession,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopDispatchPriority {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopDispatchStatus {
    Unread,
    Read,
    Delivering,
    Delivered,
    Archived,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopScheduledTaskTarget {
    pub kind: DesktopScheduledTaskTargetKind,
    pub session_id: Option<SessionId>,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopScheduledTaskTargetKind {
    NewSession,
    ExistingSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DesktopScheduledSchedule {
    Hourly {
        interval_hours: u16,
    },
    Weekly {
        days: Vec<DesktopWeekday>,
        hour: u8,
        minute: u8,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DesktopWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl DesktopWeekday {
    #[must_use]
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Monday => "Mon",
            Self::Tuesday => "Tue",
            Self::Wednesday => "Wed",
            Self::Thursday => "Thu",
            Self::Friday => "Fri",
            Self::Saturday => "Sat",
            Self::Sunday => "Sun",
        }
    }
}

impl From<time::Weekday> for DesktopWeekday {
    fn from(value: time::Weekday) -> Self {
        match value {
            time::Weekday::Monday => Self::Monday,
            time::Weekday::Tuesday => Self::Tuesday,
            time::Weekday::Wednesday => Self::Wednesday,
            time::Weekday::Thursday => Self::Thursday,
            time::Weekday::Friday => Self::Friday,
            time::Weekday::Saturday => Self::Saturday,
            time::Weekday::Sunday => Self::Sunday,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopScheduledTaskStatus {
    Idle,
    Running,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopScheduledRunStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopConfigFile {
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopHookConfigView {
    pub pre_tool_use: Vec<String>,
    pub post_tool_use: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopMcpServer {
    pub name: String,
    pub scope: String,
    pub transport: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopPluginView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: String,
    pub source: String,
    pub root_path: Option<String>,
    pub enabled: bool,
    pub default_enabled: bool,
    pub tool_count: usize,
    pub pre_tool_hook_count: usize,
    pub post_tool_hook_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateDesktopSessionRequest {
    pub title: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppendDesktopMessageRequest {
    pub message: String,
    /// A1: opt-in context mode for this turn. `None` (or an absent
    /// field in the HTTP body) falls back to `ContextMode::FollowUp`,
    /// preserving the pre-A1 behaviour. Frontend sends one of
    /// `"follow_up" | "source_first" | "combine"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ContextMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateDesktopScheduledTaskRequest {
    pub title: String,
    pub prompt: String,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub target_session_id: Option<SessionId>,
    pub schedule: DesktopScheduledSchedule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateDesktopScheduledTaskRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateDesktopDispatchItemRequest {
    pub title: String,
    pub body: String,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub target_session_id: Option<SessionId>,
    pub priority: DesktopDispatchPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateDesktopDispatchItemStatusRequest {
    pub status: DesktopDispatchStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopSessionEvent {
    Snapshot {
        session: DesktopSessionDetail,
    },
    Message {
        session_id: SessionId,
        message: DesktopConversationMessage,
    },
    PermissionRequest {
        session_id: SessionId,
        request_id: String,
        tool_name: String,
        tool_input: String,
    },
    TextDelta {
        session_id: SessionId,
        content: String,
    },
    /// Per-entry progress event emitted during a running `absorb_batch`
    /// task. Canonical SSE shape: `technical-design.md §2.1` Progress
    /// Event table. Newtype variant with an internally-tagged struct —
    /// serde flattens the inner `AbsorbProgressEvent` fields next to the
    /// `type` tag so the wire JSON matches the spec example exactly:
    /// `{"type":"absorb_progress","processed":2,"total":5,...}`.
    ///
    /// The `AbsorbProgressEvent` struct is authored in `wiki_maintainer`
    /// (§4.2.2) and shared via re-export so the maintainer's
    /// `progress_tx.send(...)` argument is wire-compatible with the SSE
    /// event body. Step 4 of this sprint adds `task_id` to the struct
    /// so multi-task SSE streams can disambiguate.
    AbsorbProgress(wiki_maintainer::AbsorbProgressEvent),
    /// Terminal event emitted once an `absorb_batch` task finishes
    /// (success, partial, or cancelled). Canonical SSE shape:
    /// `technical-design.md §2.1` Complete Event. Struct variant so
    /// `task_id` + aggregate counters are first-class siblings of
    /// `"type"` on the wire — `{"type":"absorb_complete","task_id":
    /// "absorb-...","created":3,"updated":1,"skipped":1,"failed":0,
    /// "duration_ms":12500}`.
    AbsorbComplete {
        task_id: String,
        created: usize,
        updated: usize,
        skipped: usize,
        failed: usize,
        duration_ms: u64,
    },
}

impl DesktopSessionEvent {
    #[must_use]
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::Snapshot { .. } => "snapshot",
            Self::Message { .. } => "message",
            Self::PermissionRequest { .. } => "permission_request",
            Self::TextDelta { .. } => "text_delta",
            Self::AbsorbProgress(_) => "absorb_progress",
            Self::AbsorbComplete { .. } => "absorb_complete",
        }
    }
}

/// Plan for a deferred background URL ingest.
///
/// `maybe_enrich_url` returns this when a non-WeChat URL's simple HTTP
/// fetch did not produce a raw entry and the Playwright adapter should
/// run asynchronously. `append_user_message` owns the actual spawn so
/// it can clone the session event sender and broadcast a late-arriving
/// `Snapshot` with the `enrich_status` once the bg ingest finishes —
/// letting `useAskSSE` reconcile to the new status without an extra
/// poll.
#[derive(Debug, Clone)]
struct BgIngestPlan {
    url: String,
    origin_tag: String,
}

#[derive(Debug, Clone)]
struct DesktopSessionRecord {
    metadata: SessionMetadata,
    session: RuntimeSession,
    events: broadcast::Sender<DesktopSessionEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SessionMetadata {
    id: SessionId,
    title: String,
    preview: String,
    bucket: DesktopSessionBucket,
    created_at: u64,
    updated_at: u64,
    project_name: String,
    project_path: String,
    environment_label: String,
    model_label: String,
    #[serde(default = "default_turn_state")]
    turn_state: DesktopTurnState,
    #[serde(default = "default_lifecycle_status")]
    lifecycle_status: DesktopLifecycleStatus,
    #[serde(default = "default_flagged")]
    flagged: bool,
    /// A1.1: persistent record of the most-recent turn's ContextBasis,
    /// stamped by `append_user_message` when a user turn is pushed.
    /// Replaces the pre-A1.1 "transient snapshot side-channel" design
    /// so the ContextBasisLabel can render reliably on short turns,
    /// after reload, after SSE reconnect, etc.
    ///
    /// Per-session, not per-message — we keep only the last turn's
    /// basis so scrollback shows the latest decision. The `ContextBasis`
    /// type already has `#[derive(Serialize, Deserialize)]`; the outer
    /// `#[serde(default, skip_serializing_if)]` guards legacy session
    /// JSON (pre-A1.1) which doesn't have this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_context_basis: Option<crate::ask_context::ContextBasis>,
    /// A2: session-level pinned source. Persists across turns until
    /// explicitly cleared or overwritten by another `bind_source`
    /// call. When present it overrides the A1 mode classifier — the
    /// bound source becomes the highest-priority context slice in
    /// the system prompt.
    ///
    /// Cleared by:
    ///   * `DesktopState::clear_source_binding(session_id)` — user-
    ///     triggered unbind.
    ///   * `DesktopState::bind_source(session_id, new_source)` —
    ///     implicit overwrite (binding a new ref replaces the prior).
    ///
    /// `#[serde(default, skip_serializing_if = "Option::is_none")]`
    /// guards legacy session JSON (pre-A2) which doesn't have this
    /// field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_binding: Option<crate::ask_context::binding::SessionSourceBinding>,
}

#[derive(Debug, Default)]
struct DesktopStore {
    sessions: HashMap<SessionId, DesktopSessionRecord>,
}

#[derive(Debug, Default)]
struct DesktopScheduledStore {
    tasks: HashMap<String, ScheduledTaskMetadata>,
}

#[derive(Debug, Default)]
struct DesktopDispatchStore {
    items: HashMap<String, DispatchItemMetadata>,
}

/// Best-effort per-spawn cleanup guard for turn-executing tokio tasks.
///
/// Each of the three spawn paths that drives a turn (agentic loop at
/// `lib.rs` ≈L4319, OpenAI-compat streaming at ≈L4042, vendored
/// synchronous fallback at ≈L4442) constructs one of these at the top
/// of its `tokio::spawn` closure and flips `fired = true` only after
/// the normal finalize path has succeeded. If the spawned future
/// panics, is cancelled, or otherwise bypasses the finalize block,
/// `drop` runs and performs the minimum cleanup needed to keep the
/// session from getting stuck in `Running`:
///
///   1. `try_write` on `permission_gates` / `cancel_tokens` /
///      `store.sessions` to drop stale per-turn state and reset
///      `turn_state` to `Idle`. `try_write` (not `write().await`) is
///      deliberate — `Drop` is sync, and if a writer already holds
///      the lock (e.g. a concurrent shutdown) we don't want to panic
///      on blocking; the startup reconciliation pass will recover.
///   2. A best-effort `DesktopSessionEvent::Snapshot` broadcast so
///      SSE subscribers (the frontend) see the `Idle` state
///      immediately instead of waiting on the 30-second `isStale`
///      bandaid. If the `try_read` or the send fails we silently
///      drop the broadcast — the frontend's existing staleness
///      check is still a correctness safety net.
///
/// Broadcasting a Snapshot on drop is cheap (tokio `broadcast::send`
/// is non-blocking and returns `Err` when there are no receivers),
/// and the Snapshot variant already encodes the full `DesktopSessionDetail`
/// so subscribers reconcile fully without needing a follow-up GET.
struct SessionCleanupGuard {
    state: DesktopState,
    session_id: SessionId,
    sender: broadcast::Sender<DesktopSessionEvent>,
    /// Set to `true` by the spawn closure after the happy-path finalize
    /// has completed. When `true`, `drop` is a no-op. When `false` (the
    /// closure unwound early), `drop` performs the recovery cleanup +
    /// terminal Snapshot broadcast described above.
    fired: bool,
}

impl SessionCleanupGuard {
    fn new(
        state: DesktopState,
        session_id: SessionId,
        sender: broadcast::Sender<DesktopSessionEvent>,
    ) -> Self {
        Self {
            state,
            session_id,
            sender,
            fired: false,
        }
    }
}

impl Drop for SessionCleanupGuard {
    fn drop(&mut self) {
        if self.fired {
            return;
        }
        // Sync try_write — will not block if a writer is already holding
        // the lock (which can happen on shutdown). If any try_write
        // fails, the session may be left in Running state but will be
        // reconciled at next startup (see `with_executor`).
        if let Ok(mut gates) = self.state.permission_gates.try_write() {
            gates.remove(&self.session_id);
        }
        if let Ok(mut tokens) = self.state.cancel_tokens.try_write() {
            tokens.remove(&self.session_id);
        }
        if let Ok(mut store) = self.state.store.try_write() {
            if let Some(record) = store.sessions.get_mut(&self.session_id) {
                record.metadata.turn_state = DesktopTurnState::Idle;
            }
        }
        // P0-3: best-effort terminal Snapshot so SSE subscribers can
        // recover without waiting on the 30-second `isStale` bandaid.
        // We use `try_read` (non-blocking) and ignore all failures —
        // every error path here has a fallback (startup reconcile +
        // client-side staleness check), so a missed broadcast only
        // delays recovery, never corrupts state.
        if let Ok(store) = self.state.store.try_read() {
            if let Some(record) = store.sessions.get(&self.session_id) {
                let _ = self.sender.send(DesktopSessionEvent::Snapshot {
                    session: record.detail(),
                });
            }
        }
    }
}

#[derive(Debug, Clone)]
struct DesktopTurnRequest {
    message: String,
    project_path: String,
    /// Enriched URL content (fetched via wiki_ingest) to inject into
    /// the system prompt for this turn only. Replaces the old global
    /// `CLAWWIKI_URL_CONTEXT` env var that raced across concurrent
    /// requests.
    url_context: Option<String>,
}

#[derive(Debug, Clone)]
struct DesktopTurnResult {
    session: RuntimeSession,
    model_label: String,
}

trait DesktopTurnExecutor {
    fn execute_turn(
        &self,
        session: RuntimeSession,
        request: DesktopTurnRequest,
    ) -> DesktopTurnResult;
}

#[derive(Debug, Clone)]
struct DesktopPersistence {
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct DesktopScheduledPersistence {
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct DesktopDispatchPersistence {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedDesktopState {
    next_session_id: u64,
    sessions: Vec<PersistedDesktopSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedDesktopScheduledState {
    next_task_id: u64,
    tasks: Vec<ScheduledTaskMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedDesktopDispatchState {
    next_item_id: u64,
    items: Vec<DispatchItemMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedDesktopSession {
    metadata: SessionMetadata,
    session: DesktopSessionData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ScheduledTaskMetadata {
    id: String,
    title: String,
    prompt: String,
    project_name: String,
    project_path: String,
    schedule: DesktopScheduledSchedule,
    target_session_id: Option<SessionId>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    running: bool,
    created_at: u64,
    updated_at: u64,
    last_run_at: Option<u64>,
    last_run_status: Option<DesktopScheduledRunStatus>,
    last_outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DispatchItemMetadata {
    id: String,
    title: String,
    body: String,
    project_name: String,
    project_path: String,
    source_kind: DesktopDispatchSourceKind,
    source_label: String,
    priority: DesktopDispatchPriority,
    target_session_id: Option<SessionId>,
    #[serde(default)]
    prefer_new_session: bool,
    #[serde(default = "default_dispatch_status")]
    status: DesktopDispatchStatus,
    created_at: u64,
    updated_at: u64,
    delivered_at: Option<u64>,
    last_outcome: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct DesktopSessionContext {
    trusted_project_paths: BTreeSet<String>,
    session_titles: HashMap<SessionId, String>,
}

#[derive(Debug, Clone)]
struct ScheduledTaskRunOutcome {
    is_error: bool,
    message: String,
    _session_id: Option<SessionId>,
}

impl ScheduledTaskRunOutcome {
    fn success(session_id: Option<SessionId>, message: String) -> Self {
        Self {
            is_error: false,
            message,
            _session_id: session_id,
        }
    }

    fn error(message: String) -> Self {
        Self {
            is_error: true,
            message,
            _session_id: None,
        }
    }
}

#[derive(Debug, Default)]
struct MockTurnExecutor;

impl DesktopTurnExecutor for MockTurnExecutor {
    fn execute_turn(
        &self,
        mut session: RuntimeSession,
        request: DesktopTurnRequest,
    ) -> DesktopTurnResult {
        session
            .messages
            .push(ConversationMessage::user_text(request.message));
        session.messages.push(assistant_text(
            "Desktop shell scaffold is connected to the Rust session store. Runtime turns, permissions, and tool streaming are the next integration layer.",
        ));

        DesktopTurnResult {
            session,
            model_label: DEFAULT_MODEL_LABEL.to_string(),
        }
    }
}

#[derive(Debug, Default)]
struct LiveTurnExecutor;

impl DesktopTurnExecutor for LiveTurnExecutor {
    fn execute_turn(
        &self,
        session: RuntimeSession,
        request: DesktopTurnRequest,
    ) -> DesktopTurnResult {
        execute_live_turn(session, request)
    }
}

#[derive(Clone)]
pub struct DesktopState {
    store: Arc<RwLock<DesktopStore>>,
    scheduled_store: Arc<RwLock<DesktopScheduledStore>>,
    dispatch_store: Arc<RwLock<DesktopDispatchStore>>,
    next_session_id: Arc<AtomicU64>,
    next_task_id: Arc<AtomicU64>,
    next_dispatch_item_id: Arc<AtomicU64>,
    turn_executor: Arc<dyn DesktopTurnExecutor + Send + Sync>,
    turn_lock: Arc<Mutex<()>>,
    persistence: Option<Arc<DesktopPersistence>>,
    scheduled_persistence: Option<Arc<DesktopScheduledPersistence>>,
    dispatch_persistence: Option<Arc<DesktopDispatchPersistence>>,
    scheduler_started: Arc<AtomicBool>,
    /// Per-session permission gates for the async agentic loop.
    permission_gates: Arc<RwLock<HashMap<SessionId, Arc<agentic_loop::PermissionGate>>>>,
    /// Per-session cancellation tokens for the async agentic loop.
    cancel_tokens: Arc<RwLock<HashMap<SessionId, CancellationToken>>>,
    /// Shared HTTP client reused across all agentic loop turns.
    /// Constructing a new client per turn costs DNS + TCP + TLS handshake.
    /// This single client maintains a connection pool for keep-alive.
    http_client: reqwest::Client,
    /// Persistent MCP server manager. Kept alive for the lifetime of the
    /// process so subprocess connections stay warm between tool calls.
    /// Initialized lazily on first use. Bypasses the vendored crate's
    /// crate-private global registry (see docs/audit-lessons.md L-09).
    mcp_manager: Arc<Mutex<Option<McpServerManager>>>,
    /// Discovered MCP tools, indexed by qualified_name (`mcp__server__tool`).
    /// Populated by `ensure_mcp_initialized`, consumed by the agentic loop
    /// when building the system prompt and validating tool_use dispatch.
    mcp_tools: Arc<RwLock<Vec<ManagedMcpTool>>>,
    /// In-flight WeChat QR login slots keyed by the opaque `handle`
    /// returned to the frontend. Phase 6C: lets the user add a new
    /// WeChat account entirely from the UI without running
    /// `desktop-server wechat-login` on the CLI.
    pending_wechat_logins: Arc<RwLock<HashMap<String, Arc<Mutex<wechat_ilink::PendingLoginSlot>>>>>,
    /// Running WeChat iLink long-poll monitors keyed by account_id.
    /// Each value holds the `CancellationToken` so we can stop a
    /// monitor when the user deletes that account via the HTTP API.
    /// Populated on startup by `spawn_wechat_monitors_for_all_accounts`
    /// and mutated dynamically by the add/delete account routes.
    wechat_monitors: Arc<RwLock<HashMap<String, WeChatMonitorHandle>>>,
    /// Default project path passed to newly-spawned WeChat monitors.
    /// Resolved at `DesktopState::live()` from
    /// `WECHAT_DEFAULT_PROJECT_PATH` → current dir → ".".
    wechat_default_project_path: Arc<RwLock<String>>,

    // --- Channel B: Official WeChat Customer Service (kefu) ---
    kefu_monitor: Arc<RwLock<Option<WeChatMonitorHandle>>>,
    kefu_callback_tx: Arc<RwLock<Option<tokio::sync::mpsc::Sender<wechat_kefu::CallbackEvent>>>>,
    kefu_pipeline_state:
        Arc<RwLock<Option<tokio::sync::watch::Receiver<wechat_kefu::PipelineState>>>>,
    kefu_pipeline_cancel: Arc<RwLock<Option<CancellationToken>>>,

    /// Server-level shutdown cancellation token. Injected by
    /// `desktop-server::main` at startup via `set_shutdown_cancel`
    /// **before** any monitor spawn, so tokens derived by
    /// `monitor_cancel_token()` are child tokens of this one. When the
    /// server starts a graceful shutdown (Ctrl-C / SIGTERM / Tauri
    /// window-close), cancelling this token cascades to every WeChat
    /// monitor (iLink + kefu) and the kefu pipeline — the monitors
    /// already `tokio::select!` on their `cancel`, so graceful stop is
    /// automatic. Stays `None` in unit tests / `DesktopState::new()`.
    shutdown_cancel: Arc<RwLock<Option<CancellationToken>>>,

    /// SKILL task registry. One-per-kind concurrency gate for the
    /// async `/absorb`, `/cleanup`, `/patrol` paths. Populated lazily
    /// — the HTTP handlers call `task_manager.register(kind)` which
    /// returns 409 if another task of the same kind is already
    /// running. See `absorb_task::TaskManager` (§4.5.2) for the full
    /// semantics. Initialised to an empty registry by
    /// `with_executor` so every `DesktopState` — Mock, Live, test —
    /// carries one.
    task_manager: Arc<absorb_task::TaskManager>,
    /// Session-agnostic SKILL event stream. Unlike per-session SSE,
    /// this channel works even when the user has not created/opened an
    /// Ask session yet, which is the common `/wiki` absorb-trigger path.
    skill_events: broadcast::Sender<DesktopSessionEvent>,
}

/// Handle to a running WeChat iLink monitor. Held inside
/// [`DesktopState::wechat_monitors`] so the HTTP delete-account route
/// can cancel the task cleanly.
#[derive(Clone)]
pub struct WeChatMonitorHandle {
    /// Cancellation token — calling `.cancel()` stops the monitor
    /// on its next iteration boundary.
    pub cancel: tokio_util::sync::CancellationToken,
    /// Last-known status (may be stale by the time a handler reads
    /// it — treat as informational only).
    pub status_rx: tokio::sync::watch::Receiver<wechat_ilink::MonitorStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopStateError {
    SessionNotFound(SessionId),
    SessionBusy(SessionId),
    ScheduledTaskNotFound(String),
    ScheduledTaskBusy(String),
    InvalidScheduledTask(String),
    DispatchItemNotFound(String),
    InvalidDispatchItem(String),
    ProviderNotFound(String),
    InvalidProvider(String),
}

impl Display for DesktopStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound(session_id) => {
                write!(f, "desktop session `{session_id}` not found")
            }
            Self::SessionBusy(session_id) => {
                write!(f, "desktop session `{session_id}` is already running")
            }
            Self::ScheduledTaskNotFound(task_id) => {
                write!(f, "scheduled task `{task_id}` not found")
            }
            Self::ScheduledTaskBusy(task_id) => {
                write!(f, "scheduled task `{task_id}` is already running")
            }
            Self::InvalidScheduledTask(message) => f.write_str(message),
            Self::DispatchItemNotFound(item_id) => {
                write!(f, "dispatch item `{item_id}` not found")
            }
            Self::InvalidDispatchItem(message) => f.write_str(message),
            Self::ProviderNotFound(provider_id) => {
                write!(f, "provider `{provider_id}` not found")
            }
            Self::InvalidProvider(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DesktopStateError {}

/// R1.1 reliability gate · output of
/// [`DesktopState::resolve_bound_source_body`].
///
/// Carries the body that the prompt assembler would inject as the
/// bound source body, plus a verdict on whether that body is safe
/// to summarize / ground an LLM on. Callers MUST consult
/// `is_article` before pushing the body into the system prompt:
/// when `false`, push
/// [`crate::ask_context::binding::format_archived_link_sentinel`]
/// instead, otherwise the LLM hallucinates an "article summary"
/// of an empty / one-liner body.
///
/// `kind_label` is the stable identifier that
/// [`crate::ask_context::binding::format_archived_link_sentinel`]
/// uses to pick a tailored Chinese phrasing for the user-facing
/// message ("微信纯文本消息" vs "客服对话文本片段" etc).
#[derive(Debug, Clone)]
struct ResolvedBoundSource {
    body: String,
    is_article: bool,
    kind_label: &'static str,
}

impl DesktopState {
    #[must_use]
    pub fn new() -> Self {
        Self::with_executor(Arc::new(MockTurnExecutor), None, None, None)
    }

    #[must_use]
    pub fn live() -> Self {
        Self::with_executor(
            Arc::new(LiveTurnExecutor),
            Some(Arc::new(DesktopPersistence::default())),
            Some(Arc::new(DesktopScheduledPersistence::default())),
            Some(Arc::new(DesktopDispatchPersistence::default())),
        )
    }

    fn with_executor(
        turn_executor: Arc<dyn DesktopTurnExecutor + Send + Sync>,
        persistence: Option<Arc<DesktopPersistence>>,
        scheduled_persistence: Option<Arc<DesktopScheduledPersistence>>,
        dispatch_persistence: Option<Arc<DesktopDispatchPersistence>>,
    ) -> Self {
        let (next_session_id, seeded) = persistence.as_ref().map_or_else(
            || {
                let seeded = seeded_sessions();
                (seeded.len() as u64 + 1, seeded)
            },
            |persistence| match persistence.load() {
                Ok(Some(saved)) => (saved.next_session_id, saved.into_records()),
                Ok(None) => {
                    let seeded = seeded_sessions();
                    (seeded.len() as u64 + 1, seeded)
                }
                Err(error) => {
                    eprintln!("desktop persistence load failed: {error}");
                    let seeded = seeded_sessions();
                    (seeded.len() as u64 + 1, seeded)
                }
            },
        );
        // Reconcile sessions stuck in Running state from a previous crash
        // or shutdown. Without this, sessions can stay "Running" forever
        // after a kill -9 or panic, and the user cannot send new messages
        // (they hit SessionBusy).
        let sessions: HashMap<_, _> = seeded
            .into_iter()
            .map(|mut record| {
                if record.metadata.turn_state == DesktopTurnState::Running {
                    eprintln!(
                        "[startup reconcile] session {} was Running at load — resetting to Idle (crash recovery)",
                        record.metadata.id
                    );
                    record.metadata.turn_state = DesktopTurnState::Idle;
                }
                (record.metadata.id.clone(), record)
            })
            .collect();
        let (next_task_id, scheduled_tasks) = scheduled_persistence.as_ref().map_or_else(
            || (1_u64, Vec::new()),
            |persistence| match persistence.load() {
                Ok(Some(saved)) => (saved.next_task_id, saved.tasks),
                Ok(None) => (1_u64, Vec::new()),
                Err(error) => {
                    eprintln!("desktop scheduled persistence load failed: {error}");
                    (1_u64, Vec::new())
                }
            },
        );
        let tasks = scheduled_tasks
            .into_iter()
            .map(|mut task| {
                task.running = false;
                (task.id.clone(), task)
            })
            .collect();
        let (next_dispatch_item_id, dispatch_items) = dispatch_persistence.as_ref().map_or_else(
            || (1_u64, Vec::new()),
            |persistence| match persistence.load() {
                Ok(Some(saved)) => (saved.next_item_id, saved.items),
                Ok(None) => (1_u64, Vec::new()),
                Err(error) => {
                    eprintln!("desktop dispatch persistence load failed: {error}");
                    (1_u64, Vec::new())
                }
            },
        );
        let items = dispatch_items
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect();

        Self {
            store: Arc::new(RwLock::new(DesktopStore { sessions })),
            scheduled_store: Arc::new(RwLock::new(DesktopScheduledStore { tasks })),
            dispatch_store: Arc::new(RwLock::new(DesktopDispatchStore { items })),
            next_session_id: Arc::new(AtomicU64::new(next_session_id)),
            next_task_id: Arc::new(AtomicU64::new(next_task_id)),
            next_dispatch_item_id: Arc::new(AtomicU64::new(next_dispatch_item_id)),
            turn_executor,
            turn_lock: Arc::new(Mutex::new(())),
            persistence,
            scheduled_persistence,
            dispatch_persistence,
            scheduler_started: Arc::new(AtomicBool::new(false)),
            permission_gates: Arc::new(RwLock::new(HashMap::new())),
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .pool_idle_timeout(Duration::from_secs(90))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            mcp_manager: Arc::new(Mutex::new(None)),
            mcp_tools: Arc::new(RwLock::new(Vec::new())),
            pending_wechat_logins: Arc::new(RwLock::new(HashMap::new())),
            wechat_monitors: Arc::new(RwLock::new(HashMap::new())),
            wechat_default_project_path: Arc::new(RwLock::new(
                resolve_wechat_default_project_path(),
            )),
            kefu_monitor: Arc::new(RwLock::new(None)),
            kefu_callback_tx: Arc::new(RwLock::new(None)),
            kefu_pipeline_state: Arc::new(RwLock::new(None)),
            kefu_pipeline_cancel: Arc::new(RwLock::new(None)),
            shutdown_cancel: Arc::new(RwLock::new(None)),
            task_manager: Arc::new(absorb_task::TaskManager::new()),
            skill_events: {
                let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
                tx
            },
        }
    }

    /// Access the SKILL task registry (§4.5.2). HTTP handlers call
    /// `state.task_manager().register("absorb")` to acquire a task id
    /// + cancel token under the one-per-kind concurrency rule.
    #[must_use]
    pub fn task_manager(&self) -> Arc<absorb_task::TaskManager> {
        self.task_manager.clone()
    }

    /// Subscribe to session-agnostic SKILL events such as absorb
    /// progress/completion. This is intentionally separate from
    /// `subscribe(session_id)` so wiki surfaces can show task progress
    /// without creating an empty Ask session just to hold an SSE stream.
    pub fn subscribe_skill_events(&self) -> broadcast::Receiver<DesktopSessionEvent> {
        self.skill_events.subscribe()
    }

    /// Inject the server-level shutdown cancellation token. Call this
    /// once, **before** `spawn_wechat_monitors_for_all_accounts` /
    /// `auto_start_kefu_monitor` / `start_kefu_pipeline`, so the
    /// monitor-scoped tokens are children of it and cascade on shutdown.
    /// Safe to call on a test-only `DesktopState::new()` but it won't
    /// do anything useful unless monitors are also spawned.
    pub async fn set_shutdown_cancel(&self, cancel: CancellationToken) {
        *self.shutdown_cancel.write().await = Some(cancel);
    }

    /// Derive a new `CancellationToken` for a monitor-scoped task.
    ///
    /// If `set_shutdown_cancel` was called at startup, returns a
    /// `child_token()` so a server-shutdown cancel cascades. Otherwise
    /// returns an isolated root token (tests, or standalone uses that
    /// never wire shutdown) — callers' existing cancel() semantics are
    /// preserved either way.
    async fn monitor_cancel_token(&self) -> CancellationToken {
        match self.shutdown_cancel.read().await.as_ref() {
            Some(parent) => parent.child_token(),
            None => CancellationToken::new(),
        }
    }

    #[must_use]
    pub fn bootstrap(&self) -> DesktopBootstrap {
        DesktopBootstrap {
            product_name: "OpenClaudeCode".to_string(),
            code_label: "Code".to_string(),
            top_tabs: vec![
                top_tab("home", "Home", DesktopTabKind::Home, false),
                top_tab("search", "Search", DesktopTabKind::Search, false),
                top_tab("scheduled", "Scheduled", DesktopTabKind::Scheduled, false),
                top_tab("dispatch", "Dispatch", DesktopTabKind::Dispatch, false),
                top_tab("customize", "Customize", DesktopTabKind::Customize, false),
                top_tab("openclaw", "OpenClaw", DesktopTabKind::OpenClaw, false),
                top_tab("settings", "Settings", DesktopTabKind::Settings, false),
            ],
            launchpad_items: vec![
                DesktopLaunchpadItem {
                    id: "code".to_string(),
                    label: "Code".to_string(),
                    description: "Claude Code workbench, sessions, search, and permissions."
                        .to_string(),
                    accent: "graphite".to_string(),
                    tab_id: "code".to_string(),
                },
                DesktopLaunchpadItem {
                    id: "openclaw".to_string(),
                    label: "OpenClaw".to_string(),
                    description: "Provider hub, runtime status, and agent integrations."
                        .to_string(),
                    accent: "ember".to_string(),
                    tab_id: "openclaw".to_string(),
                },
                DesktopLaunchpadItem {
                    id: "settings".to_string(),
                    label: "Settings".to_string(),
                    description: "Model providers, MCP, local environments, and defaults."
                        .to_string(),
                    accent: "cobalt".to_string(),
                    tab_id: "settings".to_string(),
                },
            ],
            settings_groups: vec![
                settings_group(
                    "providers",
                    "Model Providers",
                    "Configure upstream model services and profiles.",
                ),
                settings_group(
                    "display",
                    "Display",
                    "Window behavior, compactness, and visual defaults.",
                ),
                settings_group(
                    "data",
                    "Data & Search",
                    "Session indexing, local history, and search controls.",
                ),
                settings_group(
                    "mcp",
                    "MCP & Plugins",
                    "Connectors, plugin marketplaces, and local MCP servers.",
                ),
                settings_group(
                    "openclaw",
                    "OpenClaw",
                    "Managed provider routing and OpenClaw integration.",
                ),
            ],
            private_cloud_enabled: cfg!(feature = "private-cloud"),
        }
    }

    pub async fn workbench(&self) -> DesktopWorkbench {
        let store = self.store.read().await;
        let mut sessions = store
            .sessions
            .values()
            .map(DesktopSessionRecord::summary)
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

        let active_session_id = sessions.first().map(|session| session.id.clone());
        let mut grouped = HashMap::<DesktopSessionBucket, Vec<DesktopSessionSummary>>::new();
        for session in sessions {
            grouped.entry(session.bucket).or_default().push(session);
        }

        let mut sections = grouped
            .into_iter()
            .map(|(bucket, mut sessions)| {
                sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                DesktopSessionSection {
                    id: bucket.label().to_ascii_lowercase(),
                    label: bucket.label().to_string(),
                    sessions,
                }
            })
            .collect::<Vec<_>>();
        sections.sort_by(|left, right| {
            let left_bucket = bucket_from_label(&left.label);
            let right_bucket = bucket_from_label(&right.label);
            left_bucket.order().cmp(&right_bucket.order())
        });

        DesktopWorkbench {
            primary_actions: vec![
                nav_action(
                    "new-session",
                    "New session",
                    "plus",
                    "code",
                    DesktopTabKind::CodeSession,
                ),
                nav_action(
                    "search",
                    "Search",
                    "search",
                    "search",
                    DesktopTabKind::Search,
                ),
                nav_action(
                    "scheduled",
                    "Scheduled",
                    "clock",
                    "scheduled",
                    DesktopTabKind::Scheduled,
                ),
                nav_action(
                    "dispatch",
                    "Dispatch",
                    "dispatch",
                    "dispatch",
                    DesktopTabKind::Dispatch,
                ),
            ],
            secondary_actions: vec![nav_action(
                "customize",
                "Customize",
                "sliders",
                "customize",
                DesktopTabKind::Customize,
            )],
            project_label: "All projects".to_string(),
            project_name: DEFAULT_PROJECT_NAME.to_string(),
            session_sections: sections,
            active_session_id,
            update_banner: DesktopUpdateBanner {
                version: "1.569.0".to_string(),
                cta_label: "Relaunch".to_string(),
                body: "Updated to 1.569.0".to_string(),
            },
            account: DesktopAccountCard {
                name: "pumbaa".to_string(),
                plan_label: "Max plan".to_string(),
                shortcut_label: DEFAULT_ENVIRONMENT_LABEL.to_string(),
            },
            composer: DesktopComposerState {
                permission_mode_label: DEFAULT_PERMISSION_MODE_LABEL.to_string(),
                environment_label: DEFAULT_ENVIRONMENT_LABEL.to_string(),
                model_label: DEFAULT_MODEL_LABEL.to_string(),
                send_label: "Send".to_string(),
            },
        }
    }

    pub async fn list_sessions(&self) -> Vec<DesktopSessionSummary> {
        let store = self.store.read().await;
        let mut sessions = store
            .sessions
            .values()
            .map(DesktopSessionRecord::summary)
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        sessions
    }

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let store = self.store.read().await;
        let record = store
            .sessions
            .get(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        Ok(record.detail())
    }

    pub async fn search_sessions(&self, query: &str) -> Vec<DesktopSearchHit> {
        let normalized_query = query.trim().to_ascii_lowercase();
        if normalized_query.is_empty() {
            return Vec::new();
        }

        let store = self.store.read().await;
        let mut hits = store
            .sessions
            .values()
            .filter_map(|record| record.search_hit(&normalized_query))
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        hits
    }

    pub async fn customize(&self) -> DesktopCustomizeState {
        let project_path = {
            let store = self.store.read().await;
            store
                .sessions
                .values()
                .max_by_key(|record| record.metadata.updated_at)
                .map(|record| record.metadata.project_path.clone())
                .unwrap_or_else(|| default_project_path().to_string())
        };

        tokio::task::spawn_blocking(move || build_customize_state(project_path))
            .await
            .unwrap_or_else(|error| {
                DesktopCustomizeState::empty_with_warning(
                    default_project_path().to_string(),
                    format!("desktop customize worker crashed: {error}"),
                )
            })
    }

    pub async fn settings(&self) -> DesktopSettingsState {
        let project_path = {
            let store = self.store.read().await;
            store
                .sessions
                .values()
                .max_by_key(|record| record.metadata.updated_at)
                .map(|record| record.metadata.project_path.clone())
                .unwrap_or_else(|| default_project_path().to_string())
        };

        tokio::task::spawn_blocking(move || build_settings_state(project_path))
            .await
            .unwrap_or_else(|error| DesktopSettingsState {
                project_path: default_project_path().to_string(),
                config_home: ConfigLoader::default_for(default_project_path())
                    .config_home()
                    .display()
                    .to_string(),
                desktop_session_store_path: DesktopPersistence::default_path()
                    .display()
                    .to_string(),
                oauth_credentials_path: None,
                providers: Vec::new(),
                storage_locations: Vec::new(),
                warnings: vec![format!("desktop settings worker crashed: {error}")],
            })
    }

    pub async fn codex_runtime_state(&self) -> Result<DesktopCodexRuntimeState, DesktopStateError> {
        tokio::task::spawn_blocking(oauth_runtime::codex_runtime_state)
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("provider worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn codex_auth_overview(&self) -> Result<DesktopCodexAuthOverview, DesktopStateError> {
        tokio::task::spawn_blocking(|| codex_auth::overview_get(None))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("codex auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn import_codex_auth_profile(
        &self,
    ) -> Result<DesktopCodexAuthOverview, DesktopStateError> {
        tokio::task::spawn_blocking(|| codex_auth::profile_import(None))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("codex auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn activate_codex_auth_profile(
        &self,
        profile_id: &str,
    ) -> Result<DesktopCodexAuthOverview, DesktopStateError> {
        let profile_id = profile_id.to_string();
        tokio::task::spawn_blocking(move || codex_auth::profile_set_active(profile_id, None))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("codex auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn remove_codex_auth_profile(
        &self,
        profile_id: &str,
    ) -> Result<DesktopCodexAuthOverview, DesktopStateError> {
        let profile_id = profile_id.to_string();
        tokio::task::spawn_blocking(move || codex_auth::profile_remove(profile_id, None))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("codex auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn refresh_codex_auth_profile(
        &self,
        profile_id: &str,
    ) -> Result<DesktopCodexAuthOverview, DesktopStateError> {
        codex_auth::profile_refresh(profile_id.to_string(), None)
            .await
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn begin_codex_login(
        &self,
    ) -> Result<DesktopCodexLoginSessionSnapshot, DesktopStateError> {
        codex_auth::login_begin(None)
            .await
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn poll_codex_login(
        &self,
        session_id: &str,
    ) -> Result<DesktopCodexLoginSessionSnapshot, DesktopStateError> {
        codex_auth::login_poll(session_id.to_string())
            .await
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn managed_auth_providers(
        &self,
    ) -> Result<Vec<DesktopManagedAuthProvider>, DesktopStateError> {
        tokio::task::spawn_blocking(managed_auth::list_providers)
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn managed_auth_provider(
        &self,
        provider_id: &str,
    ) -> Result<DesktopManagedAuthProvider, DesktopStateError> {
        let provider_id = provider_id.to_string();
        tokio::task::spawn_blocking(move || managed_auth::provider_state(&provider_id))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn managed_auth_accounts(
        &self,
        provider_id: &str,
    ) -> Result<Vec<DesktopManagedAuthAccount>, DesktopStateError> {
        let provider_id = provider_id.to_string();
        tokio::task::spawn_blocking(move || managed_auth::list_accounts(&provider_id))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn import_managed_auth_accounts(
        &self,
        provider_id: &str,
    ) -> Result<Vec<DesktopManagedAuthAccount>, DesktopStateError> {
        let provider_id = provider_id.to_string();
        tokio::task::spawn_blocking(move || managed_auth::import_accounts(&provider_id))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn begin_managed_auth_login(
        &self,
        provider_id: &str,
    ) -> Result<DesktopManagedAuthLoginSessionSnapshot, DesktopStateError> {
        managed_auth::begin_login(provider_id)
            .await
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn poll_managed_auth_login(
        &self,
        provider_id: &str,
        session_id: &str,
    ) -> Result<DesktopManagedAuthLoginSessionSnapshot, DesktopStateError> {
        managed_auth::poll_login(provider_id, session_id)
            .await
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn set_managed_auth_default_account(
        &self,
        provider_id: &str,
        account_id: &str,
    ) -> Result<Vec<DesktopManagedAuthAccount>, DesktopStateError> {
        let provider_id = provider_id.to_string();
        let account_id = account_id.to_string();
        tokio::task::spawn_blocking(move || {
            managed_auth::set_default_account(&provider_id, &account_id)
        })
        .await
        .map_err(|error| {
            DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
        })?
        .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn refresh_managed_auth_account(
        &self,
        provider_id: &str,
        account_id: &str,
    ) -> Result<Vec<DesktopManagedAuthAccount>, DesktopStateError> {
        managed_auth::refresh_account(provider_id, account_id)
            .await
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn remove_managed_auth_account(
        &self,
        provider_id: &str,
        account_id: &str,
    ) -> Result<Vec<DesktopManagedAuthAccount>, DesktopStateError> {
        let provider_id = provider_id.to_string();
        let account_id = account_id.to_string();
        tokio::task::spawn_blocking(move || managed_auth::remove_account(&provider_id, &account_id))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn code_tool_launch_profile(
        &self,
        cli_tool: &str,
        provider_id: &str,
        model_id: &str,
        desktop_api_base: &str,
    ) -> Result<DesktopCodeToolLaunchProfile, DesktopStateError> {
        let cli_tool = cli_tool.to_string();
        let provider_id = provider_id.to_string();
        let model_id = model_id.to_string();
        let desktop_api_base = desktop_api_base.to_string();
        tokio::task::spawn_blocking(move || {
            managed_auth::build_code_tool_launch_profile(
                &cli_tool,
                &provider_id,
                &model_id,
                &desktop_api_base,
            )
        })
        .await
        .map_err(|error| {
            DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
        })?
        .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn managed_auth_runtime_client(
        &self,
        provider_id: &str,
    ) -> Result<DesktopManagedAuthRuntimeClient, DesktopStateError> {
        let provider_id = provider_id.to_string();
        tokio::task::spawn_blocking(move || managed_auth::runtime_client(&provider_id))
            .await
            .map_err(|error| {
                DesktopStateError::InvalidProvider(format!("managed auth worker crashed: {error}"))
            })?
            .map_err(DesktopStateError::InvalidProvider)
    }

    pub async fn scheduled(&self) -> DesktopScheduledState {
        self.ensure_scheduler();

        let project_path = self.current_project_path().await;
        let session_context = self.session_context().await;
        let now = unix_timestamp_millis();
        let tasks = {
            let store = self.scheduled_store.read().await;
            let mut tasks = store
                .tasks
                .values()
                .map(|task| build_scheduled_task(task, &session_context, now))
                .collect::<Vec<_>>();
            tasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            tasks
        };

        let summary = DesktopScheduledSummary {
            total_task_count: tasks.len(),
            enabled_task_count: tasks.iter().filter(|task| task.enabled).count(),
            running_task_count: tasks
                .iter()
                .filter(|task| task.status == DesktopScheduledTaskStatus::Running)
                .count(),
            blocked_task_count: tasks
                .iter()
                .filter(|task| task.blocked_reason.is_some())
                .count(),
            due_task_count: tasks
                .iter()
                .filter(|task| {
                    task.enabled
                        && task.blocked_reason.is_none()
                        && task.status == DesktopScheduledTaskStatus::Idle
                        && task
                            .next_run_at
                            .is_some_and(|next_run_at| next_run_at <= now)
                })
                .count(),
        };

        DesktopScheduledState {
            project_path,
            summary,
            tasks,
            trusted_project_paths: session_context.trusted_project_paths.into_iter().collect(),
            warnings: Vec::new(),
        }
    }

    pub async fn create_scheduled_task(
        &self,
        request: CreateDesktopScheduledTaskRequest,
    ) -> Result<DesktopScheduledTask, DesktopStateError> {
        self.ensure_scheduler();
        validate_scheduled_schedule(&request.schedule)?;

        let now = unix_timestamp_millis();
        let task_id = format!(
            "scheduled-task-{}",
            self.next_task_id.fetch_add(1, Ordering::Relaxed)
        );
        let title = normalize_scheduled_title(&request.title)?;
        let prompt = normalize_scheduled_prompt(&request.prompt)?;

        let (derived_project_name, derived_project_path) = {
            let store = self.store.read().await;
            let target_record = request
                .target_session_id
                .as_ref()
                .map(|session_id| {
                    store
                        .sessions
                        .get(session_id)
                        .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))
                })
                .transpose()?;

            (
                request
                    .project_name
                    .clone()
                    .or_else(|| target_record.map(|record| record.metadata.project_name.clone()))
                    .unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string()),
                request
                    .project_path
                    .clone()
                    .or_else(|| target_record.map(|record| record.metadata.project_path.clone()))
                    .unwrap_or_else(|| default_project_path().to_string()),
            )
        };

        let metadata = ScheduledTaskMetadata {
            id: task_id.clone(),
            title,
            prompt,
            project_name: derived_project_name,
            project_path: derived_project_path,
            schedule: request.schedule,
            target_session_id: request.target_session_id,
            enabled: true,
            running: false,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_run_status: None,
            last_outcome: None,
        };

        {
            let mut store = self.scheduled_store.write().await;
            store.tasks.insert(task_id.clone(), metadata);
        }

        self.persist_scheduled().await;
        self.get_scheduled_task(&task_id).await
    }

    pub async fn update_scheduled_task_enabled(
        &self,
        task_id: &str,
        enabled: bool,
    ) -> Result<DesktopScheduledTask, DesktopStateError> {
        self.ensure_scheduler();

        {
            let mut store = self.scheduled_store.write().await;
            let task = store
                .tasks
                .get_mut(task_id)
                .ok_or_else(|| DesktopStateError::ScheduledTaskNotFound(task_id.to_string()))?;
            task.enabled = enabled;
            task.updated_at = unix_timestamp_millis();
        }

        self.persist_scheduled().await;
        self.get_scheduled_task(task_id).await
    }

    pub async fn run_scheduled_task_now(
        &self,
        task_id: &str,
    ) -> Result<DesktopScheduledTask, DesktopStateError> {
        self.ensure_scheduler();
        let task = self.start_scheduled_task_run(task_id, true).await?;
        Ok(task)
    }

    pub async fn dispatch(&self) -> DesktopDispatchState {
        let project_path = self.current_project_path().await;
        let session_context = self.session_context().await;
        let items = {
            let store = self.dispatch_store.read().await;
            let mut items = store
                .items
                .values()
                .map(|item| build_dispatch_item(item, &session_context))
                .collect::<Vec<_>>();
            items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            items
        };

        let summary = DesktopDispatchSummary {
            total_item_count: items.len(),
            unread_item_count: items
                .iter()
                .filter(|item| item.status == DesktopDispatchStatus::Unread)
                .count(),
            pending_item_count: items
                .iter()
                .filter(|item| {
                    matches!(
                        item.status,
                        DesktopDispatchStatus::Unread
                            | DesktopDispatchStatus::Read
                            | DesktopDispatchStatus::Delivering
                            | DesktopDispatchStatus::Error
                    )
                })
                .count(),
            delivered_item_count: items
                .iter()
                .filter(|item| item.status == DesktopDispatchStatus::Delivered)
                .count(),
            archived_item_count: items
                .iter()
                .filter(|item| item.status == DesktopDispatchStatus::Archived)
                .count(),
        };

        DesktopDispatchState {
            project_path,
            summary,
            items,
            warnings: Vec::new(),
        }
    }

    pub async fn create_dispatch_item(
        &self,
        request: CreateDesktopDispatchItemRequest,
    ) -> Result<DesktopDispatchItem, DesktopStateError> {
        let title = normalize_dispatch_title(&request.title)?;
        let body = normalize_dispatch_body(&request.body)?;
        let item_id = format!(
            "dispatch-item-{}",
            self.next_dispatch_item_id.fetch_add(1, Ordering::Relaxed)
        );
        let now = unix_timestamp_millis();
        let (project_name, project_path) = {
            let store = self.store.read().await;
            let target_record = request
                .target_session_id
                .as_ref()
                .map(|session_id| {
                    store
                        .sessions
                        .get(session_id)
                        .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))
                })
                .transpose()?;

            (
                request
                    .project_name
                    .clone()
                    .or_else(|| target_record.map(|record| record.metadata.project_name.clone()))
                    .unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string()),
                request
                    .project_path
                    .clone()
                    .or_else(|| target_record.map(|record| record.metadata.project_path.clone()))
                    .unwrap_or_else(|| default_project_path().to_string()),
            )
        };

        let metadata = DispatchItemMetadata {
            id: item_id.clone(),
            title,
            body,
            project_name,
            project_path,
            source_kind: DesktopDispatchSourceKind::LocalInbox,
            source_label: "Local inbox".to_string(),
            priority: request.priority,
            target_session_id: request.target_session_id.clone(),
            prefer_new_session: request.target_session_id.is_none(),
            status: DesktopDispatchStatus::Unread,
            created_at: now,
            updated_at: now,
            delivered_at: None,
            last_outcome: None,
        };

        {
            let mut store = self.dispatch_store.write().await;
            store.items.insert(item_id.clone(), metadata);
        }

        self.persist_dispatch().await;
        self.get_dispatch_item(&item_id).await
    }

    pub async fn update_dispatch_item_status(
        &self,
        item_id: &str,
        status: DesktopDispatchStatus,
    ) -> Result<DesktopDispatchItem, DesktopStateError> {
        validate_dispatch_status_transition(status)?;

        {
            let mut store = self.dispatch_store.write().await;
            let item = store
                .items
                .get_mut(item_id)
                .ok_or_else(|| DesktopStateError::DispatchItemNotFound(item_id.to_string()))?;

            if item.status == DesktopDispatchStatus::Delivering {
                return Err(DesktopStateError::InvalidDispatchItem(
                    "cannot change dispatch item state while it is delivering".to_string(),
                ));
            }

            item.status = status;
            item.updated_at = unix_timestamp_millis();
        }

        self.persist_dispatch().await;
        self.get_dispatch_item(item_id).await
    }

    pub async fn deliver_dispatch_item(
        &self,
        item_id: &str,
    ) -> Result<DesktopDispatchItem, DesktopStateError> {
        let item = {
            let mut store = self.dispatch_store.write().await;
            let item = store
                .items
                .get_mut(item_id)
                .ok_or_else(|| DesktopStateError::DispatchItemNotFound(item_id.to_string()))?;

            if item.status == DesktopDispatchStatus::Archived {
                return Err(DesktopStateError::InvalidDispatchItem(
                    "cannot deliver an archived dispatch item".to_string(),
                ));
            }

            if item.status == DesktopDispatchStatus::Delivering {
                return Err(DesktopStateError::InvalidDispatchItem(
                    "dispatch item is already delivering".to_string(),
                ));
            }

            item.status = DesktopDispatchStatus::Delivering;
            item.updated_at = unix_timestamp_millis();
            item.clone()
        };

        self.persist_dispatch().await;

        let delivery = self.execute_dispatch_delivery(&item).await;
        {
            let mut store = self.dispatch_store.write().await;
            let Some(record) = store.items.get_mut(item_id) else {
                return Err(DesktopStateError::DispatchItemNotFound(item_id.to_string()));
            };
            record.updated_at = unix_timestamp_millis();
            match delivery {
                Ok(delivered_session_id) => {
                    record.status = DesktopDispatchStatus::Delivered;
                    record.delivered_at = Some(unix_timestamp_millis());
                    record.last_outcome = Some("Delivered into Code session.".to_string());
                    if record.target_session_id.is_none() {
                        record.target_session_id = Some(delivered_session_id);
                        record.prefer_new_session = false;
                    }
                }
                Err(error) => {
                    record.status = DesktopDispatchStatus::Error;
                    record.last_outcome = Some(error.to_string());
                }
            }
        }

        self.persist_dispatch().await;
        self.get_dispatch_item(item_id).await
    }

    // ── Session manipulation ────────────────────────────────────────

    pub async fn delete_session(&self, session_id: &str) -> Result<bool, DesktopStateError> {
        let removed = self
            .store
            .write()
            .await
            .sessions
            .remove(session_id)
            .is_some();
        if !removed {
            return Err(DesktopStateError::SessionNotFound(session_id.to_string()));
        }
        self.persist().await;
        Ok(true)
    }

    /// Delete every session that has zero messages AND is currently idle.
    /// Optionally skip one session (so a freshly created session the user
    /// is actively staring at isn't nuked mid-render). Returns the ids of
    /// deleted sessions, so the caller can log / invalidate caches.
    ///
    /// Intended as a one-click recovery for the "8 empty conversations"
    /// pile-up regression: legacy code auto-created a Session on every
    /// mount, and after a few route switches users ended up with a wall
    /// of placeholder entries. See [`useAskSession`] (frontend) for the
    /// upstream fix.
    pub async fn cleanup_empty_sessions(&self, except: Option<&str>) -> Vec<String> {
        let mut store = self.store.write().await;
        let ids_to_remove: Vec<String> = store
            .sessions
            .iter()
            .filter(|(id, record)| {
                // Skip the session the caller wants to preserve.
                if Some(id.as_str()) == except {
                    return false;
                }
                // Only delete idle sessions — don't touch a running turn.
                if !matches!(record.metadata.turn_state, DesktopTurnState::Idle) {
                    return false;
                }
                record.session.messages.is_empty()
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids_to_remove {
            store.sessions.remove(id);
        }
        let had_removals = !ids_to_remove.is_empty();
        drop(store);
        if had_removals {
            self.persist().await;
        }
        ids_to_remove
    }

    pub async fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<(), DesktopStateError> {
        let mut store = self.store.write().await;
        let record = store
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        record.metadata.title = normalize_session_title(title);
        record.metadata.updated_at = unix_timestamp_millis();
        let snapshot = DesktopSessionEvent::Snapshot {
            session: record.detail(),
        };
        let _ = record.events.send(snapshot);
        drop(store);
        self.persist().await;
        Ok(())
    }

    pub async fn cancel_session(&self, session_id: &str) -> Result<(), DesktopStateError> {
        // Signal the agentic loop to stop (if running).
        if let Some(token) = self.cancel_tokens.read().await.get(session_id) {
            token.cancel();
        }

        let mut store = self.store.write().await;
        let record = store
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        record.metadata.turn_state = DesktopTurnState::Idle;
        record.metadata.updated_at = unix_timestamp_millis();
        let snapshot = DesktopSessionEvent::Snapshot {
            session: record.detail(),
        };
        let _ = record.events.send(snapshot);
        drop(store);
        self.persist().await;
        Ok(())
    }

    /// Update a session's lifecycle status (Inbox workflow: Todo →
    /// InProgress → NeedsReview → Done → Archived). Inspired by
    /// craft-agents-oss.
    pub async fn set_session_lifecycle_status(
        &self,
        session_id: &str,
        status: DesktopLifecycleStatus,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let mut store = self.store.write().await;
        let record = store
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        record.metadata.lifecycle_status = status;
        record.metadata.updated_at = unix_timestamp_millis();
        let detail = record.detail();
        let sender = record.events.clone();
        drop(store);
        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });
        Ok(detail)
    }

    /// Toggle or set the flagged bit on a session. Flagged sessions
    /// are highlighted in the sidebar so users can mark important
    /// ones for later attention.
    pub async fn set_session_flagged(
        &self,
        session_id: &str,
        flagged: bool,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let mut store = self.store.write().await;
        let record = store
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        record.metadata.flagged = flagged;
        record.metadata.updated_at = unix_timestamp_millis();
        let detail = record.detail();
        let sender = record.events.clone();
        drop(store);
        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });
        Ok(detail)
    }

    /// A2: bind a session to an explicit internal source (raw entry,
    /// wiki page, or inbox item). Persists until explicitly cleared
    /// or overwritten by another bind call.
    ///
    /// Every subsequent `append_user_message` on this session will
    /// resolve the binding's body, prepend it to the system prompt,
    /// and force a SourceFirst-style framing (see `append_user_message`
    /// for the integration details). The binding does NOT auto-clear
    /// on turn completion — the user owns the lifecycle.
    ///
    /// Errors
    /// ──────
    ///   * `SessionNotFound` — no session with `session_id` in store.
    ///
    /// Broadcasts a `DesktopSessionEvent::Snapshot` so every live SSE
    /// subscriber reconciles the new binding without polling.
    pub async fn bind_source(
        &self,
        session_id: &str,
        source: crate::ask_context::binding::SourceRef,
        reason: Option<String>,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let mut store = self.store.write().await;
        let record = store
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        let binding = crate::ask_context::binding::SessionSourceBinding::new(source, reason);
        record.metadata.source_binding = Some(binding);
        record.metadata.updated_at = unix_timestamp_millis();
        let detail = record.detail();
        let sender = record.events.clone();
        drop(store);
        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });
        Ok(detail)
    }

    /// A2: drop a session's source binding. After this call the
    /// session reverts to pure A1 `ContextMode` framing — the next
    /// turn is packaged using whatever mode the frontend supplies
    /// (or `FollowUp` by default) without a prepended bound source.
    ///
    /// No-op semantics
    /// ───────────────
    /// Clearing an already-unbound session is explicitly allowed — we
    /// still write the detail + broadcast so the UI can confirm state.
    /// This matches the "idempotent write" pattern used by
    /// `set_session_flagged` / `set_session_lifecycle_status`.
    ///
    /// Errors
    /// ──────
    ///   * `SessionNotFound` — no session with `session_id` in store.
    pub async fn clear_source_binding(
        &self,
        session_id: &str,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let mut store = self.store.write().await;
        let record = store
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        record.metadata.source_binding = None;
        record.metadata.updated_at = unix_timestamp_millis();
        let detail = record.detail();
        let sender = record.events.clone();
        drop(store);
        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });
        Ok(detail)
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let store = self.store.read().await;
        let record = store
            .sessions
            .get(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        Ok(record.detail())
    }

    pub async fn forward_permission_decision(
        &self,
        session_id: &str,
        request_id: &str,
        decision: &str,
    ) -> Result<(), DesktopStateError> {
        // Validate session exists.
        {
            let store = self.store.read().await;
            let _record = store
                .sessions
                .get(session_id)
                .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        }

        // Forward to the permission gate (if one exists for this session).
        let gates = self.permission_gates.read().await;
        if let Some(gate) = gates.get(session_id) {
            let decision = match decision {
                "allow" => agentic_loop::PermissionDecision::Allow,
                "allow_always" => agentic_loop::PermissionDecision::AllowAlways,
                _ => agentic_loop::PermissionDecision::Deny {
                    reason: "user denied".into(),
                },
            };
            gate.resolve(request_id, decision).await;
        }
        Ok(())
    }

    // ── Scheduled task extended CRUD ──────────────────────────────

    pub async fn delete_scheduled_task(&self, task_id: &str) -> Result<bool, DesktopStateError> {
        let removed = self
            .scheduled_store
            .write()
            .await
            .tasks
            .remove(task_id)
            .is_some();
        if !removed {
            return Err(DesktopStateError::ScheduledTaskNotFound(
                task_id.to_string(),
            ));
        }
        self.persist_scheduled().await;
        Ok(true)
    }

    pub async fn update_scheduled_task(
        &self,
        task_id: &str,
        title: Option<String>,
        prompt: Option<String>,
        enabled: Option<bool>,
    ) -> Result<DesktopScheduledTask, DesktopStateError> {
        let task_id = task_id.to_string();
        let mut store = self.scheduled_store.write().await;
        let task = store
            .tasks
            .get_mut(&task_id)
            .ok_or_else(|| DesktopStateError::ScheduledTaskNotFound(task_id.clone()))?;
        if let Some(t) = title {
            task.title = t;
        }
        if let Some(p) = prompt {
            task.prompt = p;
        }
        if let Some(e) = enabled {
            task.enabled = e;
        }
        drop(store);
        self.persist_scheduled().await;
        self.get_scheduled_task(&task_id).await
    }

    // ── Dispatch item extended CRUD ───────────────────────────────

    pub async fn delete_dispatch_item(&self, item_id: &str) -> Result<bool, DesktopStateError> {
        let removed = self
            .dispatch_store
            .write()
            .await
            .items
            .remove(item_id)
            .is_some();
        if !removed {
            return Err(DesktopStateError::DispatchItemNotFound(item_id.to_string()));
        }
        self.persist_dispatch().await;
        Ok(true)
    }

    pub async fn update_dispatch_item(
        &self,
        item_id: &str,
        title: Option<String>,
        body: Option<String>,
        priority: Option<DesktopDispatchPriority>,
    ) -> Result<DesktopDispatchItem, DesktopStateError> {
        let item_id = item_id.to_string();
        let mut store = self.dispatch_store.write().await;
        let item = store
            .items
            .get_mut(&item_id)
            .ok_or_else(|| DesktopStateError::DispatchItemNotFound(item_id.clone()))?;
        if let Some(t) = title {
            item.title = t;
        }
        if let Some(b) = body {
            item.body = b;
        }
        if let Some(p) = priority {
            item.priority = p;
        }
        drop(store);
        self.persist_dispatch().await;
        self.get_dispatch_item(&item_id).await
    }

    /// Write the permission mode to the project's `.claw/settings.json` file.
    ///
    /// This is the authoritative source of truth for the permission mode.
    /// The agentic loop reads this on each turn via `ConfigLoader::load()`.
    ///
    /// Accepts both naming styles for symmetry with the lifecycle status
    /// API (S-01):
    ///   - camelCase:  `default` | `acceptEdits` | `bypassPermissions` | `plan`
    ///   - snake_case: `default` | `accept_edits` | `bypass_permissions` | `plan`
    ///
    /// Values are normalized to the on-disk keys the runtime config
    /// loader recognizes.
    pub async fn set_permission_mode(
        &self,
        project_path: &str,
        mode: &str,
    ) -> Result<(), DesktopStateError> {
        // Normalize frontend mode labels to config-file labels that
        // `parse_optional_permission_mode` in the runtime crate accepts.
        // Both camelCase and snake_case forms are accepted as input.
        let normalized = match mode {
            "default" => "default",
            "acceptEdits" | "accept_edits" => "acceptEdits",
            "bypassPermissions" | "bypass_permissions" => "danger-full-access",
            "plan" => "plan",
            other => {
                return Err(DesktopStateError::InvalidProvider(format!(
                    "unsupported permission mode: {other} \
                     (expected: default | acceptEdits | bypassPermissions | plan, \
                     or snake_case: accept_edits | bypass_permissions)"
                )));
            }
        };

        let project = PathBuf::from(project_path);
        let claw_dir = project.join(".claw");
        let settings_path = claw_dir.join("settings.json");

        // Read existing settings (if any) and merge permissionMode.
        let mut json_obj: serde_json::Map<String, serde_json::Value> = if settings_path.is_file() {
            match fs::read_to_string(&settings_path) {
                Ok(content) => {
                    serde_json::from_str(&content).unwrap_or_else(|_| serde_json::Map::new())
                }
                Err(_) => serde_json::Map::new(),
            }
        } else {
            serde_json::Map::new()
        };
        json_obj.insert(
            "permissionMode".to_string(),
            serde_json::Value::String(normalized.to_string()),
        );

        // Ensure .claw/ directory exists.
        if let Err(e) = fs::create_dir_all(&claw_dir) {
            return Err(DesktopStateError::InvalidProvider(format!(
                "failed to create {}: {e}",
                claw_dir.display()
            )));
        }

        let serialized = serde_json::to_string_pretty(&serde_json::Value::Object(json_obj))
            .map_err(|e| {
                DesktopStateError::InvalidProvider(format!("failed to serialize settings: {e}"))
            })?;

        fs::write(&settings_path, serialized).map_err(|e| {
            DesktopStateError::InvalidProvider(format!(
                "failed to write {}: {e}",
                settings_path.display()
            ))
        })?;

        Ok(())
    }

    /// Read the current permission mode from `.claw/settings.json`.
    ///
    /// Returns the frontend-facing label (`"default"`, `"acceptEdits"`,
    /// `"bypassPermissions"`, `"plan"`) or `"default"` if the file does not
    /// exist or contains no permissionMode field.
    pub async fn get_permission_mode(
        &self,
        project_path: &str,
    ) -> Result<String, DesktopStateError> {
        let project = PathBuf::from(project_path);
        let settings_path = project.join(".claw").join("settings.json");

        if !settings_path.is_file() {
            return Ok("default".to_string());
        }

        let content = fs::read_to_string(&settings_path)
            .map_err(|e| DesktopStateError::InvalidProvider(e.to_string()))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| DesktopStateError::InvalidProvider(e.to_string()))?;

        let mode = json
            .get("permissionMode")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        // Convert disk format back to frontend format.
        Ok(match mode {
            "danger-full-access" | "dontAsk" => "bypassPermissions".to_string(),
            other => other.to_string(),
        })
    }

    /// Compact a session's message history to free context window.
    pub async fn compact_session_messages(
        &self,
        session_id: &str,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        use runtime::{compact_session, should_compact, CompactionConfig};

        let config = CompactionConfig::default();

        let mut store = self.store.write().await;
        let record = store
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;

        // Refuse compaction while a turn is running: the agentic loop's
        // incremental persistence callback would overwrite our work.
        if record.metadata.turn_state == DesktopTurnState::Running {
            return Err(DesktopStateError::SessionBusy(session_id.to_string()));
        }

        if !should_compact(&record.session, config) {
            // Nothing to compact — return current state.
            return Ok(record.detail());
        }

        let result = compact_session(&record.session, config);
        record.session = result.compacted_session;
        record.metadata.updated_at = unix_timestamp_millis();

        let detail = record.detail();
        let sender = record.events.clone();
        drop(store);

        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });

        Ok(detail)
    }

    /// Initialize the persistent MCP server manager from the project's
    /// `.claw/settings.json` configuration.
    ///
    /// Idempotent — if the manager is already initialized, this is a no-op
    /// and returns the previously discovered tool list. On first call, it:
    ///   1. Loads runtime config via `ConfigLoader`
    ///   2. Creates a `McpServerManager` from the configured MCP servers
    ///   3. Calls `discover_tools()` to spawn subprocesses + list tools
    ///   4. Stores the manager + discovered tools for subsequent calls
    ///
    /// Returns the list of `ManagedMcpTool` available for the LLM to call.
    /// On failure, logs to stderr and returns an empty list (graceful degrade).
    pub async fn ensure_mcp_initialized(&self, project_path: &Path) -> Vec<ManagedMcpTool> {
        // Fast path: already initialized.
        {
            let tools = self.mcp_tools.read().await;
            let manager_guard = self.mcp_manager.lock().await;
            if manager_guard.is_some() {
                return tools.clone();
            }
        }

        // Load MCP config from the project.
        let loader = ConfigLoader::default_for(project_path);
        let runtime_config = match loader.load() {
            Ok(rc) => rc,
            Err(error) => {
                eprintln!("[MCP init] failed to load runtime config: {error}");
                return Vec::new();
            }
        };

        let servers = runtime_config.mcp().servers();
        if servers.is_empty() {
            // No MCP servers configured. Store an empty manager so we
            // don't retry on every call.
            let mut guard = self.mcp_manager.lock().await;
            *guard = Some(McpServerManager::from_servers(servers));
            return Vec::new();
        }

        let mut manager = McpServerManager::from_servers(servers);
        let discovered = match manager.discover_tools().await {
            Ok(tools) => {
                eprintln!(
                    "[MCP init] connected {} server(s), discovered {} tool(s)",
                    servers.len(),
                    tools.len()
                );
                tools
            }
            Err(e) => {
                eprintln!("[MCP init] tool discovery error: {e}");
                Vec::new()
            }
        };

        // Store manager and discovered tools for later use.
        {
            let mut guard = self.mcp_manager.lock().await;
            *guard = Some(manager);
        }
        {
            let mut tools_guard = self.mcp_tools.write().await;
            *tools_guard = discovered.clone();
        }

        discovered
    }

    /// Call a tool on the initialized MCP server manager.
    ///
    /// `qualified_tool_name` is the full `mcp__server__tool` identifier.
    /// Returns the formatted tool result as a string, or an error message.
    pub async fn mcp_call_tool(
        &self,
        qualified_tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let mut guard = self.mcp_manager.lock().await;
        let manager = guard
            .as_mut()
            .ok_or_else(|| "MCP manager not initialized".to_string())?;

        match manager
            .call_tool(qualified_tool_name, Some(arguments))
            .await
        {
            Ok(response) => {
                // McpToolCallResult has content: Vec<McpToolCallContent>,
                // structured_content, is_error. Format as JSON for the LLM.
                if let Some(result) = response.result {
                    serde_json::to_string_pretty(&result)
                        .map_err(|e| format!("failed to serialize MCP result: {e}"))
                } else if let Some(error) = response.error {
                    Err(format!("MCP error: {}", error.message))
                } else {
                    Ok("{}".to_string())
                }
            }
            Err(e) => Err(format!("MCP call failed: {e}")),
        }
    }

    pub async fn create_session(
        &self,
        request: CreateDesktopSessionRequest,
    ) -> DesktopSessionDetail {
        let session_number = self.next_session_id.fetch_add(1, Ordering::Relaxed);
        let session_id = format!("desktop-session-{session_number}");
        let now = unix_timestamp_millis();
        let title = normalize_session_title(
            request
                .title
                .unwrap_or_else(|| "New session".to_string())
                .trim(),
        );
        let project_name = request
            .project_name
            .unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string());
        let project_path = request
            .project_path
            .unwrap_or_else(|| default_project_path().to_string());

        let record = session_record(SessionMetadata {
            id: session_id.clone(),
            title,
            preview: "Fresh local Code session".to_string(),
            bucket: DesktopSessionBucket::Today,
            created_at: now,
            updated_at: now,
            project_name,
            project_path,
            environment_label: DEFAULT_ENVIRONMENT_LABEL.to_string(),
            model_label: DEFAULT_MODEL_LABEL.to_string(),
            turn_state: DesktopTurnState::Idle,
            lifecycle_status: DesktopLifecycleStatus::Todo,
            flagged: false,
            last_context_basis: None,
            // A2: fresh sessions start with no binding; populated by
            // a later `bind_source` call.
            source_binding: None,
        });

        let detail = record.detail();
        let snapshot = DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        };
        let sender = record.events.clone();

        self.store.write().await.sessions.insert(session_id, record);

        self.persist().await;
        let _ = sender.send(snapshot);
        detail
    }

    /// Fork a session: create a new session with messages up to `message_index`.
    ///
    /// Preserves ALL fields from the parent session (compaction, usage,
    /// version, etc.) — only the messages vector is truncated to the fork
    /// point. Previously this used `RuntimeSession::default()` which reset
    /// everything, losing compaction state and causing duplicate compactions.
    /// See docs/audit-lessons.md L-10.
    pub async fn fork_session(
        &self,
        parent_session_id: &str,
        message_index: Option<usize>,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let (parent_session, parent_metadata) = {
            let store = self.store.read().await;
            let record = store
                .sessions
                .get(parent_session_id)
                .ok_or_else(|| DesktopStateError::SessionNotFound(parent_session_id.to_string()))?;
            // Clone the full RuntimeSession to preserve all fields.
            (record.session.clone(), record.metadata.clone())
        };

        let session_number = self.next_session_id.fetch_add(1, Ordering::Relaxed);
        let session_id = format!("desktop-session-{session_number}");
        let now = unix_timestamp_millis();

        // Clone the parent session (preserves compaction, usage, etc.) and
        // truncate its message list at the fork point.
        let mut forked_session = parent_session;
        if let Some(idx) = message_index {
            if idx < forked_session.messages.len() {
                forked_session.messages.truncate(idx + 1);
            }
            // If idx is out of range, keep all messages (permissive).
        }
        forked_session.fork = Some(RuntimeSessionFork {
            parent_session_id: parent_session_id.to_string(),
            branch_name: None,
        });

        let record = session_record(SessionMetadata {
            id: session_id.clone(),
            title: format!("{} (fork)", parent_metadata.title),
            preview: parent_metadata.preview.clone(),
            bucket: DesktopSessionBucket::Today,
            created_at: now,
            updated_at: now,
            project_name: parent_metadata.project_name.clone(),
            project_path: parent_metadata.project_path.clone(),
            environment_label: parent_metadata.environment_label.clone(),
            model_label: parent_metadata.model_label.clone(),
            turn_state: DesktopTurnState::Idle,
            lifecycle_status: DesktopLifecycleStatus::Todo,
            flagged: false,
            // A1.1: fresh fork starts with no basis; it gets stamped
            // on the next user turn. (Inheriting parent's basis would
            // be confusing — fork is effectively a new conversation.)
            last_context_basis: None,
            // A2: fork does NOT inherit the parent's source_binding.
            // Forks frequently pivot topic; carrying a binding across
            // would surprise the user. The forked session starts
            // fresh and can be re-bound explicitly if desired.
            source_binding: None,
        });

        let mut store_record = record;
        store_record.session = forked_session;

        let detail = store_record.detail();
        let sender = store_record.events.clone();
        self.store
            .write()
            .await
            .sessions
            .insert(session_id, store_record);

        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });

        Ok(detail)
    }

    /// Truncate a string at a char boundary, never splitting a multi-byte char.
    fn safe_truncate(s: &str, max_bytes: usize) -> &str {
        if s.len() <= max_bytes {
            return s;
        }
        let mut end = max_bytes;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }

    /// M4: render the `EnrichStatus::Reused::reason` field with a stable
    /// tag prefix (`pending:` / `approved:` / `rejected:` / `silent:` /
    /// `content_duplicate:`) so the frontend can branch on decision
    /// kind without having to decode the full `IngestDecision` JSON.
    /// Non-reuse decisions fall back to the raw `reason()` string —
    /// `RefreshedContent` is handled separately (through `Ingested +
    /// Reused` mapping in the caller).
    fn enrich_reuse_reason(decision: &crate::url_ingest::IngestDecision) -> String {
        use crate::url_ingest::IngestDecision;
        match decision {
            IngestDecision::ReusedWithPendingInbox { reason } => {
                format!("pending:{reason}")
            }
            IngestDecision::ReusedApproved { reason } => format!("approved:{reason}"),
            IngestDecision::ReusedAfterReject { reason } => {
                format!("rejected:{reason}")
            }
            IngestDecision::ReusedSilent { reason } => format!("silent:{reason}"),
            IngestDecision::ContentDuplicate {
                matching_url,
                matching_raw_id,
            } => {
                if matching_url.is_empty() {
                    format!("content_duplicate:match={matching_raw_id:05}")
                } else {
                    format!("content_duplicate:src={matching_url}")
                }
            }
            other => other.reason(),
        }
    }

    /// A2 + R1.1: resolve a `SourceRef` to a body string plus a
    /// reliability gate. The body is read from `wiki_store`:
    ///
    ///   * `Raw { id }` → `wiki_store::read_raw_entry(paths, id)` →
    ///     body + `frontmatter.source` for the article-shape gate.
    ///   * `Wiki { slug }` → `wiki_store::read_wiki_page(paths, slug)`
    ///     → body. Wiki pages are by definition article-shaped
    ///     (already maintained), so `is_article` is always `true`.
    ///   * `Inbox { id }` → scan `list_inbox_entries(paths)` for the
    ///     matching `id`, then resolve `source_raw_id` via
    ///     `read_raw_entry`. Inbox items are pointers at raws, not
    ///     bodies themselves.
    ///
    /// Returns `None` on any lookup failure (entry deleted, slug
    /// missing, inbox id not found, `source_raw_id` stale). The
    /// caller degrades gracefully — the turn still runs, just without
    /// the bound-source prepend.
    ///
    /// On success, returns a [`ResolvedBoundSource`] carrying:
    ///   * `body` — already truncated to the source's token budget
    ///     (`token_budget_for_source`), so callers don't need to
    ///     call `truncate_source_body` themselves.
    ///   * `is_article` — `wiki_store::is_full_article` verdict on
    ///     the underlying source string. Callers MUST gate
    ///     `format_bound_source` / `grounding_instruction_block` on
    ///     this flag; when `false`, push
    ///     `format_archived_link_sentinel` instead so the LLM is
    ///     not asked to summarize an empty / non-article body.
    ///   * `kind_label` — stable identifier (e.g. `"wechat_text"`,
    ///     `"article"`) that
    ///     `binding::format_archived_link_sentinel` consumes to pick
    ///     a tailored Chinese phrasing for the user-facing message.
    fn resolve_bound_source_body(
        source: &crate::ask_context::binding::SourceRef,
    ) -> Option<ResolvedBoundSource> {
        use crate::ask_context::binding::{
            token_budget_for_source, truncate_source_body, SourceRef,
        };

        // Resolve `WikiPaths` once — matches the pattern
        // `url_ingest::resolve_wiki_paths` uses elsewhere.
        let root = wiki_store::default_root();
        if let Err(e) = wiki_store::init_wiki(&root) {
            eprintln!("[bind_source] init_wiki failed: {e}");
            return None;
        }
        let paths = wiki_store::WikiPaths::resolve(&root);

        // R1.1: `(body, source_string)`. The source_string is the
        // raw entry's `frontmatter.source` (e.g. "url",
        // "wechat-article", "wechat-text"); for `SourceRef::Wiki`
        // we synthesize "wiki" since wiki pages are by definition
        // article-shaped (they were already absorbed/maintained).
        let (body, source_string): (String, String) = match source {
            SourceRef::Raw { id, .. } => match wiki_store::read_raw_entry(&paths, *id) {
                Ok((entry, body)) => (body, entry.source),
                Err(e) => {
                    eprintln!("[bind_source] read_raw_entry({id}) failed: {e}");
                    return None;
                }
            },
            SourceRef::Wiki { slug, .. } => match wiki_store::read_wiki_page(&paths, slug) {
                Ok((_summary, body)) => (body, "wiki".to_string()),
                Err(e) => {
                    eprintln!("[bind_source] read_wiki_page({slug}) failed: {e}");
                    return None;
                }
            },
            SourceRef::Inbox { id, .. } => {
                // Inbox items point at raws via `source_raw_id`. Scan
                // the full list (no `read_inbox_by_id` helper exists
                // in wiki_store yet) — cheap enough since inboxes stay
                // small in practice.
                let inbox_entries = match wiki_store::list_inbox_entries(&paths) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("[bind_source] list_inbox_entries failed: {e}");
                        return None;
                    }
                };
                let entry = inbox_entries.iter().find(|e| e.id == *id)?;
                let raw_id = entry.source_raw_id?;
                match wiki_store::read_raw_entry(&paths, raw_id) {
                    Ok((entry, body)) => (body, entry.source),
                    Err(e) => {
                        eprintln!("[bind_source] inbox#{id} → raw#{raw_id} read failed: {e}");
                        return None;
                    }
                }
            }
        };

        let budget = token_budget_for_source(source);
        // R1.1: gate "is this body safe to summarize / ground on?"
        // - Wiki pages: always article-shaped (already maintained).
        // - Raw entries: depends on `frontmatter.source` —
        //   `is_full_article` returns true only for fetched articles
        //   / documents.
        let is_article = match source {
            SourceRef::Wiki { .. } => true,
            SourceRef::Raw { .. } | SourceRef::Inbox { .. } => {
                wiki_store::is_full_article(&source_string)
            }
        };
        let kind_label = match source {
            SourceRef::Wiki { .. } => "wiki",
            _ => wiki_store::raw_entry_kind_label(&source_string),
        };
        Some(ResolvedBoundSource {
            body: truncate_source_body(&body, budget),
            is_article,
            kind_label,
        })
    }

    /// If the message contains a URL, try to fetch its content and
    /// return an enriched message with the article text prepended.
    ///
    /// Returns `(enriched_or_original_message, EnrichStatus option, BgIngestPlan option)`:
    ///   * No URL detected → (original, None, None).
    ///   * Success → (enriched prompt with article body, Some(Success), None).
    ///   * Playwright quality-reject for a WeChat URL → (system-notice
    ///     prompt telling the LLM to ask the user to paste content
    ///     manually, Some(RejectedQuality), None). Preserves the
    ///     original Ask-layer contract: the LLM must not hallucinate
    ///     content when the fetch failed.
    ///   * Non-WeChat URL where simple fetch didn't persist → (original,
    ///     None, Some(BgIngestPlan)). The caller is responsible for
    ///     spawning a background Playwright pass; when that pass
    ///     completes, the caller should rebuild a session detail and
    ///     broadcast a `Snapshot` so the UI's SSE subscription can
    ///     reconcile to the late outcome.
    ///   * All other failure paths → (original, Some(status), None) so
    ///     the UI can surface an inline notice without distorting what
    ///     the LLM sees.
    ///
    /// M2: the three previously-duplicated fetch + write + inbox blocks
    /// (simple / synchronous Playwright / background Playwright) now
    /// funnel through `url_ingest::ingest_url`. Adapter selection still
    /// follows the original behaviour: simple fetch first, then
    /// Playwright for WeChat sync, or background Playwright for other
    /// URLs that the simple fetch couldn't satisfy.
    ///
    /// M2.1: the background Playwright branch no longer `spawn`s inside
    /// this helper — it returns the plan so `append_user_message` can
    /// spawn with access to the `DesktopState` + session event sender
    /// and broadcast a `Snapshot` once the outcome lands.
    ///
    /// A1: accepts a `mode` so `SourceFirst` / `Combine` turns get
    /// the new "新素材：..." prefix instead of the legacy `FollowUp`
    /// framing. The prefix change lives in
    /// `ask_context::format_enriched_source` so this function just
    /// delegates.
    async fn maybe_enrich_url(
        message: String,
        mode: crate::ask_context::ContextMode,
    ) -> (String, Option<EnrichStatus>, Option<BgIngestPlan>) {
        // Quick check: does the message contain a URL?
        let url = match message
            .split_whitespace()
            .find(|w| w.starts_with("http://") || w.starts_with("https://"))
        {
            Some(u) => u
                .trim_end_matches(|c: char| !c.is_ascii() || matches!(c, '.' | ',' | ')' | ']'))
                .to_string(),
            None => return (message, None, None),
        };

        eprintln!("[enrich_url] detected: {url}");

        // ── Simple HTTP fetch first (fast, 5s timeout) ────────────
        let simple_outcome = crate::url_ingest::ingest_url(crate::url_ingest::IngestRequest {
            url: &url,
            origin_tag: "url-fetch".into(),
            prefer_playwright: Some(false),
            fetch_timeout: std::time::Duration::from_secs(5),
            allow_text_fallback: None,
            force: false,
        })
        .await;
        eprintln!(
            "[enrich_url] simple fetch outcome: {}",
            simple_outcome.as_display()
        );

        if let crate::url_ingest::IngestOutcome::Ingested {
            ref entry,
            ref title,
            ref body,
            ref decision,
            ..
        } = simple_outcome
        {
            // A1: delegate the prefix/wrapper to ask_context so
            // SourceFirst / Combine get the new "新素材" framing.
            // FollowUp continues to emit bit-for-bit the prior
            // "请基于以下文章内容..." wording.
            let enriched = crate::ask_context::format_enriched_source(
                mode,
                title,
                Self::safe_truncate(body, 6000),
                &message,
            );
            // M4: when the ingest wrote a new raw because content
            // drifted on an existing URL (RefreshedContent), surface
            // that through EnrichStatus::Reused with a `refreshed:`
            // reason prefix so the UI can render "内容已更新" rather
            // than a fresh "saved" banner.
            let status = match decision {
                crate::url_ingest::IngestDecision::RefreshedContent {
                    previous_raw_id, ..
                } => EnrichStatus::Reused {
                    title: title.clone(),
                    raw_id: entry.id,
                    reason: format!("refreshed:prev={previous_raw_id}"),
                },
                _ => EnrichStatus::Success {
                    title: title.clone(),
                    raw_id: entry.id,
                },
            };
            return (enriched, Some(status), None);
        }

        // M3: canonical URL dedupe short-circuit. If the simple fetch
        // returned `ReusedExisting`, the dedupe layer found a prior
        // raw landing for this canonical URL and skipped the network.
        // We still want to enrich the LLM prompt with the existing
        // body, AND surface a `Reused` status to the UI so the user
        // knows why no "saved" banner fired.
        if let crate::url_ingest::IngestOutcome::ReusedExisting {
            ref entry,
            ref decision,
            ..
        } = simple_outcome
        {
            // Load the prior raw body so the LLM context stays
            // identical to a fresh ingest — otherwise the dedupe
            // path silently regresses the conversation quality.
            // A1: same `format_enriched_source` delegation as the
            // fresh-ingest arm; keeps the FollowUp / SourceFirst /
            // Combine framings consistent across both paths.
            let (enriched, title_for_status) = match crate::url_ingest::load_reused_body(entry.id) {
                Some((reused_title, reused_body)) => (
                    crate::ask_context::format_enriched_source(
                        mode,
                        &reused_title,
                        Self::safe_truncate(&reused_body, 6000),
                        &message,
                    ),
                    reused_title,
                ),
                None => (message.clone(), entry.slug.clone()),
            };
            return (
                enriched,
                Some(EnrichStatus::Reused {
                    title: title_for_status,
                    raw_id: entry.id,
                    reason: Self::enrich_reuse_reason(decision),
                }),
                None,
            );
        }

        // ── Playwright fallback: sync for WeChat, background otherwise ─
        let is_wechat = url.contains("weixin.qq.com");
        eprintln!(
            "[enrich_url] simple fetch did not produce ingest, trying Playwright (sync={is_wechat})"
        );

        if is_wechat {
            let pw_outcome = crate::url_ingest::ingest_url(crate::url_ingest::IngestRequest {
                url: &url,
                origin_tag: "playwright-fetch".into(),
                prefer_playwright: Some(true),
                fetch_timeout: std::time::Duration::from_secs(45),
                allow_text_fallback: None,
                force: false,
            })
            .await;
            eprintln!(
                "[enrich_url] playwright outcome: {}",
                pw_outcome.as_display()
            );

            match pw_outcome {
                crate::url_ingest::IngestOutcome::Ingested {
                    entry,
                    title,
                    body,
                    decision,
                    ..
                } => {
                    // A1: same delegation as the simple-fetch arm.
                    let enriched = crate::ask_context::format_enriched_source(
                        mode,
                        &title,
                        Self::safe_truncate(&body, 6000),
                        &message,
                    );
                    // M4 parity with the simple-fetch arm — mark
                    // RefreshedContent as a Reused (with a
                    // `refreshed:` reason prefix) even though a new
                    // raw was written.
                    let status = match decision {
                        crate::url_ingest::IngestDecision::RefreshedContent {
                            previous_raw_id,
                            ..
                        } => EnrichStatus::Reused {
                            title,
                            raw_id: entry.id,
                            reason: format!("refreshed:prev={previous_raw_id}"),
                        },
                        _ => EnrichStatus::Success {
                            title,
                            raw_id: entry.id,
                        },
                    };
                    return (enriched, Some(status), None);
                }
                crate::url_ingest::IngestOutcome::ReusedExisting {
                    entry, decision, ..
                } => {
                    // Same pattern as the simple-fetch dedupe arm —
                    // load the prior body so the LLM still has
                    // context, but tag the status as Reused so the
                    // UI shows "已在素材库 #NNNNN" instead of a fresh
                    // "saved" banner.
                    // A1: same delegation as the simple-fetch dedupe arm.
                    let (enriched, title_for_status) =
                        match crate::url_ingest::load_reused_body(entry.id) {
                            Some((reused_title, reused_body)) => (
                                crate::ask_context::format_enriched_source(
                                    mode,
                                    &reused_title,
                                    Self::safe_truncate(&reused_body, 6000),
                                    &message,
                                ),
                                reused_title,
                            ),
                            None => (message.clone(), entry.slug.clone()),
                        };
                    return (
                        enriched,
                        Some(EnrichStatus::Reused {
                            title: title_for_status,
                            raw_id: entry.id,
                            reason: Self::enrich_reuse_reason(&decision),
                        }),
                        None,
                    );
                }
                crate::url_ingest::IngestOutcome::RejectedQuality { reason } => {
                    // Preserve the Ask contract: tell the LLM
                    // explicitly that the fetch failed so it doesn't
                    // hallucinate content — identical to pre-M2 text.
                    let enriched = format!(
                        "[系统通知] 链接抓取失败（{}）。请告知用户手动复制内容粘贴，\
                         或指明是否需要基于 URL 本身回答。\n\n用户原始消息：{}",
                        reason, message
                    );
                    return (
                        enriched,
                        Some(EnrichStatus::RejectedQuality { reason }),
                        None,
                    );
                }
                crate::url_ingest::IngestOutcome::PrerequisiteMissing { dep, hint } => {
                    return (
                        message,
                        Some(EnrichStatus::PrerequisiteMissing { dep, hint }),
                        None,
                    );
                }
                crate::url_ingest::IngestOutcome::FetchFailed { error } => {
                    return (
                        message,
                        Some(EnrichStatus::FetchFailed {
                            reason: format!("{error}"),
                        }),
                        None,
                    );
                }
                crate::url_ingest::IngestOutcome::InvalidUrl { reason } => {
                    return (message, Some(EnrichStatus::FetchFailed { reason }), None);
                }
                // `IngestedInboxSuppressed` and `FallbackToText` can't
                // happen on this path (no fallback requested, and a
                // fresh raw id can't collide with an existing inbox
                // pending entry). Treat defensively as success-ish.
                crate::url_ingest::IngestOutcome::IngestedInboxSuppressed { entry, .. } => {
                    return (
                        message,
                        Some(EnrichStatus::Success {
                            title: String::new(),
                            raw_id: entry.id,
                        }),
                        None,
                    );
                }
                crate::url_ingest::IngestOutcome::FallbackToText { .. } => {
                    return (message, None, None);
                }
            }
        }

        // Non-WeChat URL where simple fetch didn't land a raw entry:
        // hand back a plan so `append_user_message` can spawn the
        // background Playwright pass with access to the session event
        // sender. The bg task will broadcast a `Snapshot` with the
        // late-arriving `enrich_status`, letting the Ask SSE client
        // reconcile once the raw lands — see M2.1 (Worker A Task A-1).
        let plan = BgIngestPlan {
            url: url.clone(),
            origin_tag: "playwright-fetch".to_string(),
        };

        // Mirror the pre-M2 contract: return ORIGINAL message on every
        // path that doesn't have sync Playwright content.
        (message, None, Some(plan))
    }

    pub async fn append_user_message(
        &self,
        session_id: &str,
        message: String,
        mode: Option<crate::ask_context::ContextMode>,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        // A1: resolve the effective context mode. `None` (old HTTP
        // bodies, WeChat bridges, scheduled tasks, dispatch items)
        // maps to FollowUp so legacy behaviour is preserved.
        let user_mode = mode.unwrap_or_default();

        // A2: peek at the session binding (if any) before we start
        // URL enrichment. Resolution fails gracefully — on any
        // wiki_store lookup error we fall back to the pre-A2 path
        // (no bound-source prepend, no mode override) so an orphaned
        // binding never blocks the LLM call. The peek is a separate
        // short-lived read lock so it doesn't race with the main
        // write lock below; if a parallel `bind_source` / `clear`
        // lands between the peek and the write, at worst we use a
        // stale binding — which is bounded by the user's perception
        // (they just clicked Send, the binding at Send-time is what
        // the turn ran with).
        let session_binding: Option<crate::ask_context::binding::SessionSourceBinding> = {
            let store = self.store.read().await;
            store
                .sessions
                .get(session_id)
                .and_then(|record| record.metadata.source_binding.clone())
        };
        // Resolve outside the read lock — wiki_store I/O is
        // synchronous but potentially slow (filesystem hit). If it
        // fails we log + degrade to pre-A2 behaviour.
        //
        // R1.1: the resolver now returns `ResolvedBoundSource` so
        // each downstream caller can gate `format_bound_source` /
        // `grounding_instruction_block` on `is_article`. When false
        // (e.g. user bound a `wechat-text` raw written as fallback
        // when URL fetch failed), the prompt assembler injects a
        // sentinel via `format_archived_link_sentinel` instead of
        // pretending the body is an article.
        let bound_body: Option<(crate::ask_context::binding::SourceRef, ResolvedBoundSource)> =
            session_binding.as_ref().and_then(|b| {
                Self::resolve_bound_source_body(&b.source)
                    .map(|resolved| (b.source.clone(), resolved))
            });
        if session_binding.is_some() && bound_body.is_none() {
            eprintln!(
                "[bind_source] session {session_id} has binding but resolve failed; degrading to non-bound turn"
            );
        }
        // A2: when a binding resolves successfully, force SourceFirst
        // framing unless the user explicitly picked `Combine` (which
        // keeps history + asks the LLM to dual-source). `FollowUp`
        // with a binding is upgraded to `SourceFirst` — the binding
        // is the whole point of this turn.
        let effective_mode =
            if bound_body.is_some() && user_mode != crate::ask_context::ContextMode::Combine {
                crate::ask_context::ContextMode::SourceFirst
            } else {
                user_mode
            };

        // URL interception: fetch content for LLM context.
        // CRITICAL: only the original URL goes into session history.
        // Enriched content is injected as a temporary system message for
        // the current turn only — it does NOT persist in the session,
        // preventing context pollution and LLM hallucination.
        let (enriched, enrich_status, bg_plan) =
            Self::maybe_enrich_url(message.clone(), effective_mode).await;
        let has_enrichment = enriched != message;

        // A3 — Fresh-Link auto-binding.
        //
        // When the user's message contains a URL AND enrichment
        // succeeded AND no persistent `SessionSourceBinding` is
        // active (A2 takes precedence), we auto-promote the fresh
        // raw entry to a turn-scoped `bound_source`. This gives
        // the UI a binding chip for "turn-local" sources and lets
        // the rest of the pipeline treat the enriched content like
        // an A2-bound source (force SourceFirst unless Combine).
        //
        // Intentionally NOT done:
        //  * No write into `SessionMetadata.source_binding` — A3 is
        //    turn-local. If the next turn has no new URL, the
        //    auto-bind naturally expires (enrich_status is None).
        //  * No re-injection of the raw body into the system prompt
        //    — A1's `maybe_enrich_url` already prepended the enriched
        //    content above. Calling `format_bound_source` again
        //    would duplicate the body.
        //  * No `resolve_bound_source_body` hit on wiki_store — same
        //    reason: the enriched string is already authoritative
        //    for this turn.
        //
        // The flag flows into `ContextBasis.auto_bound = true` so
        // the frontend chip can render "自动绑定" rather than the
        // "绑定来源" label used by A2.
        let auto_bound_source: Option<crate::ask_context::binding::SourceRef> =
            if bound_body.is_none() {
                match enrich_status.as_ref() {
                    Some(EnrichStatus::Success { title, raw_id })
                    | Some(EnrichStatus::Reused { title, raw_id, .. }) => {
                        Some(crate::ask_context::binding::SourceRef::Raw {
                            id: *raw_id,
                            title: title.clone(),
                        })
                    }
                    _ => None,
                }
            } else {
                None
            };
        // A3: when auto-bind fires, force SourceFirst just like A2
        // session bindings do — unless the user picked Combine. This
        // re-trims history on top of whatever `user_mode` asked for,
        // so a fresh-URL turn always sees the reset boundary marker.
        let effective_mode = if auto_bound_source.is_some()
            && effective_mode != crate::ask_context::ContextMode::Combine
        {
            crate::ask_context::ContextMode::SourceFirst
        } else {
            effective_mode
        };

        let user_message = ConversationMessage::user_text(message.clone());
        let session_id = session_id.to_string();

        let (mut detail, sender, session, previous_message_count, project_path) = {
            let mut store = self.store.write().await;
            let record = store
                .sessions
                .get_mut(&session_id)
                .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.clone()))?;

            if record.metadata.turn_state == DesktopTurnState::Running {
                return Err(DesktopStateError::SessionBusy(session_id));
            }

            let session = record.session.clone();
            let previous_message_count = session.messages.len();
            let project_path = record.metadata.project_path.clone();
            record.metadata.updated_at = unix_timestamp_millis();
            record.metadata.preview = truncate_preview(&message);
            record.metadata.bucket = DesktopSessionBucket::Today;
            record.metadata.turn_state = DesktopTurnState::Running;
            // Auto-transition lifecycle: Todo → InProgress on first message,
            // Done/Archived → InProgress if user resumes an old session.
            // NeedsReview stays (user is responding to the review).
            if matches!(
                record.metadata.lifecycle_status,
                DesktopLifecycleStatus::Todo
                    | DesktopLifecycleStatus::Done
                    | DesktopLifecycleStatus::Archived
            ) {
                record.metadata.lifecycle_status = DesktopLifecycleStatus::InProgress;
            }
            // A5-Polish: derive a real title from the user's first
            // message when the session still carries one of the
            // well-known default placeholders. Pre-fix, only the
            // backend-authored "New session" triggered this; the
            // frontend's useAskSession creates sessions with
            // "Ask · new conversation" (see apps/desktop-shell/src/
            // features/ask/useAskSession.ts), so those sessions
            // never got a meaningful title even after the user sent
            // real content. The sidebar ended up full of identical
            // "Ask · new conversation" rows.
            if is_default_session_title(&record.metadata.title) {
                record.metadata.title = session_title_from_message(&message);
            }
            record.session.messages.push(user_message.clone());

            // A1.1: compute + persist the turn's ContextBasis onto
            // SessionMetadata BEFORE building `detail`, so the detail
            // we return (and the Snapshot we broadcast right after)
            // already carries the stable field. Persisted in
            // SessionMetadata.last_context_basis so subsequent
            // `record.detail()` calls (idle refresh, GET after reload,
            // SSE reconnect) still see it until the NEXT user turn
            // overwrites it. Fixes A1's short-turn subscription race
            // where ContextBasisLabel failed to render when running→
            // idle happened faster than the SSE subscribe.
            //
            // A2: `source_bytes` is set to the bound-source body size
            // when a binding resolved, so the frontend chip shows the
            // combined token footprint. If both URL enrichment AND a
            // binding ran, the binding wins (it's the explicit pin).
            let source_bytes: Option<usize> = if let Some((_, resolved)) = &bound_body {
                Some(resolved.body.len())
            } else if has_enrichment {
                Some(enriched.len())
            } else {
                None
            };
            let history_turns_included = crate::ask_context::history_turns_after_packaging(
                effective_mode,
                &record.session.messages,
            );
            let mut basis = crate::ask_context::ContextBasis::new(
                effective_mode,
                history_turns_included,
                source_bytes,
            );
            // A2: attach the bound source to the basis so the UI chip
            // can render without subscribing to extra events. A2 path
            // (explicit SessionSourceBinding) wins over A3 auto-bind
            // — if both exist, the explicit pin is the authoritative
            // source and the auto_bound flag stays false.
            //
            // A4: on both the A2 and A3 paths, also stamp
            // `grounding_applied = true` so the frontend can render a
            // "✓ Grounded" badge. The A2 branch gets the rules
            // through `format_bound_source` (agentic/fallback prompt
            // assembly below); the A3 branch gets them through
            // `grounding_instruction_block()` tacked on at the end
            // of `system_prompt_text`. Either way the LLM saw the
            // Grounded Mode guardrails, so the flag is true.
            if let Some((source, resolved)) = &bound_body {
                // R1.1: only stamp `grounding_applied = true` when the
                // bound source is article-shaped — when it's not, the
                // prompt assembler pushes
                // `format_archived_link_sentinel` instead of the
                // Grounded Mode rules, so the UI's "Grounded" badge
                // would lie. The `bound_source_is_article` flag tells
                // the frontend which chip to render (warning vs
                // green Grounded).
                basis = basis
                    .with_bound_source(Some(source.clone()))
                    .with_grounding_applied(resolved.is_article)
                    .with_bound_source_is_article(Some(resolved.is_article));
            } else if let Some(source) = &auto_bound_source {
                // A3: no explicit binding, but enrichment produced a
                // raw → auto-bind for this turn. Mark the basis so
                // the UI can render the "自动绑定" chip and so
                // downstream consumers know the source is turn-local.
                //
                // A3 is by construction article-shaped: the auto-bind
                // only fires after a successful `EnrichStatus::Success`
                // / `Reused`, which means a real URL fetch produced
                // a `url`/`wechat-article` raw. Stamp
                // `bound_source_is_article = Some(true)` so the
                // frontend renders the green Grounded chip.
                basis = basis
                    .with_bound_source(Some(source.clone()))
                    .with_auto_bound(true)
                    .with_grounding_applied(true)
                    .with_bound_source_is_article(Some(true));
            }
            record.metadata.last_context_basis = Some(basis);

            (
                record.detail(),
                record.events.clone(),
                session,
                previous_message_count,
                project_path,
            )
        };

        // M2 side-channel: stamp the enrich outcome onto the detail
        // before it goes on the wire. Frontend keys on this field to
        // render inline success / failure banners. Cleared by
        // subsequent refresh calls since every other `detail()`
        // builder defaults `enrich_status` back to `None`.
        detail.enrich_status = enrich_status;

        // A1.1: `detail.context_basis` is now populated inside
        // `record.detail()` from the persistent
        // `SessionMetadata.last_context_basis` we stamped above.
        // No transient-side-channel assignment needed here.

        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });

        // ── M2.1: background Playwright ingest + late Snapshot broadcast ──
        // When `maybe_enrich_url` returns a plan, the simple fetch
        // could not persist the URL and a background Playwright pass
        // was requested. We spawn it here (not inside `maybe_enrich_url`)
        // so we have access to the session event sender + the
        // `DesktopState` store: once the bg outcome lands we rebuild
        // the detail with the new `enrich_status` and broadcast a
        // `Snapshot`. `useAskSSE` keys on the snapshot's
        // `enrich_status` so the UI reconciles to the late result
        // without an extra poll.
        if let Some(plan) = bg_plan {
            let state_clone = self.clone();
            let sender_clone = sender.clone();
            let session_id_clone = session_id.clone();
            tokio::spawn(async move {
                let outcome = crate::url_ingest::ingest_url(crate::url_ingest::IngestRequest {
                    url: &plan.url,
                    origin_tag: plan.origin_tag.clone(),
                    prefer_playwright: Some(true),
                    fetch_timeout: std::time::Duration::from_secs(60),
                    allow_text_fallback: None,
                    force: false,
                })
                .await;
                eprintln!("[enrich_url:bg] {}", outcome.as_display());

                // Map the orchestrator outcome onto the side-channel
                // `EnrichStatus` the UI renders. We only broadcast
                // a snapshot when the outcome is informative;
                // `InvalidUrl` / `FallbackToText` should not happen on
                // this path (the URL was already validated + no
                // fallback requested) so we skip them defensively.
                let late_status: Option<EnrichStatus> = match outcome {
                    crate::url_ingest::IngestOutcome::Ingested {
                        entry,
                        title,
                        decision:
                            crate::url_ingest::IngestDecision::RefreshedContent {
                                previous_raw_id, ..
                            },
                        ..
                    } => Some(EnrichStatus::Reused {
                        title,
                        raw_id: entry.id,
                        reason: format!("refreshed:prev={previous_raw_id}"),
                    }),
                    crate::url_ingest::IngestOutcome::Ingested { entry, title, .. } => {
                        Some(EnrichStatus::Success {
                            title,
                            raw_id: entry.id,
                        })
                    }
                    crate::url_ingest::IngestOutcome::IngestedInboxSuppressed { entry, .. } => {
                        Some(EnrichStatus::Success {
                            title: entry.slug.clone(),
                            raw_id: entry.id,
                        })
                    }
                    crate::url_ingest::IngestOutcome::ReusedExisting {
                        entry, decision, ..
                    } => Some(EnrichStatus::Reused {
                        title: entry.slug.clone(),
                        raw_id: entry.id,
                        reason: Self::enrich_reuse_reason(&decision),
                    }),
                    crate::url_ingest::IngestOutcome::RejectedQuality { reason } => {
                        Some(EnrichStatus::RejectedQuality { reason })
                    }
                    crate::url_ingest::IngestOutcome::FetchFailed { error } => {
                        Some(EnrichStatus::FetchFailed {
                            reason: error.to_string(),
                        })
                    }
                    crate::url_ingest::IngestOutcome::PrerequisiteMissing { dep, hint } => {
                        Some(EnrichStatus::PrerequisiteMissing { dep, hint })
                    }
                    crate::url_ingest::IngestOutcome::InvalidUrl { .. }
                    | crate::url_ingest::IngestOutcome::FallbackToText { .. } => None,
                };

                let Some(late_status) = late_status else {
                    return;
                };

                // Rebuild the detail from the current record and
                // stamp on the late `enrich_status`. We do not
                // mutate the record (only transient side-channel
                // data) so a `read()` lock is sufficient.
                let store = state_clone.store.read().await;
                let Some(record) = store.sessions.get(&session_id_clone) else {
                    return;
                };
                let mut late_detail = record.detail();
                late_detail.enrich_status = Some(late_status);
                let _ = sender_clone.send(DesktopSessionEvent::Snapshot {
                    session: late_detail,
                });
            });
        }

        // ── Spawn agentic loop ───────────────────────────────────
        let cancel_token = CancellationToken::new();
        let permission_gate = Arc::new(agentic_loop::PermissionGate::new(
            sender.clone(),
            session_id.clone(),
        ));
        self.permission_gates
            .write()
            .await
            .insert(session_id.clone(), permission_gate.clone());
        self.cancel_tokens
            .write()
            .await
            .insert(session_id.clone(), cancel_token.clone());

        // Try to resolve credentials for the agentic loop, in priority order:
        //   1. ANTHROPIC_API_KEY env var (direct mode, simplest setup)
        //   2. .claude/settings.json `direct_api_key` field (per-project key)
        //   3. managed auth (codex-openai)
        //   4. managed auth (qwen-code)
        let runtime_client = resolve_runtime_credentials(self, &PathBuf::from(&project_path)).await;

        let state = self.clone();
        let turn_executor = Arc::clone(&self.turn_executor);

        // A5.1 — OpenAiCompat providers (Kimi, DeepSeek, Qwen, etc.)
        // take their own streaming path. Before A5.1 we forced them
        // into `execute_live_turn` → vendored sync `run_turn`, which
        // never emitted `TextDelta` events, so the UI showed "等一会
        // 儿、整段落下" instead of real streaming. The new path below
        // speaks OpenAI ChatCompletions with `stream: true` directly
        // and broadcasts a `TextDelta` per incoming chunk.
        let openai_compat_client: Option<DesktopManagedAuthRuntimeClient> = match &runtime_client {
            Ok(c) if c.provider_kind == DesktopManagedAuthProviderKind::OpenAiCompat => {
                Some(c.clone())
            }
            _ => None,
        };

        if let Some(oai_client) = openai_compat_client {
            // Resolve a human-friendly model_label up-front so session
            // metadata reflects the *actual* provider the user picked
            // in Settings (pre-A5.1 every session was labeled "Opus 4.6"
            // regardless of providers.json active = moonshot-kimi).
            let friendly_label = friendly_openai_compat_label(
                &oai_client.provider_id,
                oai_client.default_model.as_deref(),
            );
            {
                let mut store = self.store.write().await;
                if let Some(record) = store.sessions.get_mut(&session_id) {
                    record.metadata.model_label = friendly_label.clone();
                }
            }

            // Flatten the current session messages into OpenAI chat
            // format. A1/A2/A3 context framing is already applied by
            // the system_prompt assembly below (see `system_prompt_text`).
            let session_for_stream = session.clone();
            let marker_for_openai = crate::ask_context::boundary_marker_for(effective_mode);
            // R1.1: gate the bound-source body on `is_article`. When the
            // user bound a non-article raw (wechat-text, kefu-text, etc.),
            // emit the sentinel instead of `format_bound_source` so the
            // LLM doesn't hallucinate a summary of an empty body.
            let bound_prefix_for_openai: Option<String> = bound_body.as_ref().map(|(s, r)| {
                if r.is_article {
                    crate::ask_context::binding::format_bound_source(s, &r.body)
                } else {
                    crate::ask_context::binding::format_archived_link_sentinel(s, r.kind_label)
                }
            });
            let system_prompt_text_openai = build_system_prompt_for_openai_compat(
                &project_path,
                &bound_prefix_for_openai,
                marker_for_openai,
                has_enrichment.then(|| enriched.as_str()),
                auto_bound_source.as_ref(),
            );

            let http_client = self.http_client.clone();
            let state = self.clone();
            let sender_for_stream = sender.clone();
            let session_id_for_stream = session_id.clone();
            let cancel_token_for_stream = cancel_token.clone();
            let model_for_stream = oai_client
                .default_model
                .clone()
                .unwrap_or_else(|| "moonshot-v1-auto".to_string());
            let tool_specs_for_stream = if model_supports_openai_tools(&model_for_stream) {
                let specs: Vec<serde_json::Value> = tools::mvp_tool_specs()
                    .iter()
                    .map(crate::openai_tool_schema::to_openai_function_tool)
                    .collect();
                if specs.is_empty() {
                    None
                } else {
                    eprintln!(
                        "[openai-compat-stream] session={session_id} model={model_for_stream} enabling tool_calls with {} tool specs",
                        specs.len()
                    );
                    Some(specs)
                }
            } else {
                eprintln!(
                    "[openai-compat-stream] session={session_id} model={model_for_stream} not in tool support whitelist; text-only mode"
                );
                None
            };
            let workspace_path_for_stream = PathBuf::from(&project_path);
            let bypass_permissions_for_stream = {
                let loader = ConfigLoader::default_for(&workspace_path_for_stream);
                match loader.load() {
                    Ok(rc) => matches!(
                        rc.permission_mode()
                            .map(permission_mode_from_config)
                            .unwrap_or(PermissionMode::WorkspaceWrite),
                        PermissionMode::DangerFullAccess
                    ),
                    Err(_) => false,
                }
            };
            let permission_gate_for_stream = permission_gate.clone();
            let user_message_text = message.clone();
            let previous_count_for_stream = previous_message_count;
            let friendly_label_for_stream = friendly_label;

            tokio::spawn(async move {
                // P0-1: guard this path the same way the agentic spawn
                // is guarded. If `run_streaming_turn` or
                // `finalize_openai_compat_turn` panics / unwinds before
                // the post-finalize cleanup runs, the guard's Drop
                // resets `turn_state` to Idle and broadcasts a
                // terminal Snapshot so SSE subscribers recover
                // immediately.
                let mut guard = SessionCleanupGuard::new(
                    state.clone(),
                    session_id_for_stream.clone(),
                    sender_for_stream.clone(),
                );

                // Existing history + the newly appended user message.
                // Keep internal blocks intact so OpenAI-compat multi-turn
                // loops can feed assistant.tool_use and role:tool results
                // back into the next ChatCompletions request.
                let mut msgs = session_for_stream.messages.clone();
                msgs.push(ConversationMessage::user_text(user_message_text));

                // P1-1: throttled session.updated_at bump so long
                // streams don't trip the frontend isStale soft-recovery
                // (30s threshold). The closure uses try_write so stream
                // ticks never block on a write lock — if we can't get
                // the lock this cycle, we'll bump on the next tick.
                let state_for_tick = state.clone();
                let session_id_for_tick = session_id_for_stream.clone();
                let on_stream_tick: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                    if let Ok(mut store) = state_for_tick.store.try_write() {
                        if let Some(record) = store.sessions.get_mut(&session_id_for_tick) {
                            record.metadata.updated_at = unix_timestamp_millis();
                        }
                    }
                });

                let config = openai_compat_streaming::StreamingTurnConfig {
                    base_url: oai_client.base_url.clone(),
                    api_key: oai_client.bearer_token.clone(),
                    model: model_for_stream,
                    messages: msgs,
                    system_prompt: Some(system_prompt_text_openai),
                    on_stream_tick: Some(on_stream_tick),
                    tool_specs: tool_specs_for_stream,
                    tool_policy: None,
                    workspace_path: Some(workspace_path_for_stream),
                    permission_gate: Some(permission_gate_for_stream),
                    bypass_permissions: bypass_permissions_for_stream,
                    tool_timeout_secs: crate::tool_execution::DEFAULT_TOOL_TIMEOUT_SECS,
                };

                let result = openai_compat_streaming::run_streaming_turn(
                    &http_client,
                    config,
                    &sender_for_stream,
                    &session_id_for_stream,
                    &cancel_token_for_stream,
                )
                .await;

                // Persist the assistant reply (or a failure marker)
                // into the session store and broadcast the final
                // Message + Snapshot so the UI promotes the streaming
                // buffer to a real assistant message.
                state
                    .finalize_openai_compat_turn(
                        &session_id_for_stream,
                        session_for_stream,
                        previous_count_for_stream,
                        result,
                        sender_for_stream,
                        friendly_label_for_stream,
                    )
                    .await;

                // Cleanup: drop cancel token + permission gate entries
                // so a stale gate from this turn doesn't block the
                // next one. We mimic what `SessionCleanupGuard` does
                // below in the agentic path.
                if let Ok(mut tokens) = state.cancel_tokens.try_write() {
                    tokens.remove(&session_id_for_stream);
                }
                if let Ok(mut gates) = state.permission_gates.try_write() {
                    gates.remove(&session_id_for_stream);
                }

                // Finalize + cleanup succeeded: suppress the guard's
                // terminal Snapshot + state reset. The guard's Drop
                // only runs if this block is bypassed by an unwind.
                guard.fired = true;
            });

            return Ok(detail);
        }

        match runtime_client {
            Ok(client) => {
                // Use the new agentic loop with real tool execution.
                //
                // This path is for AnthropicCompat providers ONLY (e.g.
                // a compatible gateway or a direct Anthropic API key).
                // OpenAiCompat
                // providers are routed to execute_live_turn above.
                let bridge_base_url = client.base_url.clone();
                let model_override = client
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "default".to_string());
                let tool_specs = tools::mvp_tool_specs();
                let project_path_buf = PathBuf::from(&project_path);
                let claude_md_discovery =
                    system_prompt::find_claude_md_with_source(&project_path_buf);
                let workspace_skills = system_prompt::find_workspace_skills(&project_path_buf);
                if !workspace_skills.is_empty() {
                    eprintln!(
                        "[skills] loaded {} workspace skill(s) from {}/.claude/skills/",
                        workspace_skills.len(),
                        project_path_buf.display()
                    );
                }
                let mut system_prompt_text =
                    system_prompt::build_system_prompt_with_source_and_skills(
                        &project_path_buf,
                        &tool_specs,
                        claude_md_discovery.as_ref(),
                        &workspace_skills,
                    );

                // v2 bugfix: Inject URL-fetched content into the managed-auth
                // system prompt. Previously only the Err(no-auth) fallback
                // branch injected `url_context`; the Ok(managed-auth) branch
                // used `agentic_loop` and discarded `enriched` entirely, so
                // the LLM saw only the raw URL. Fix: append enriched content
                // to the system prompt string here so both branches behave
                // identically.
                //
                // A2: if a session-level binding resolved, prepend the
                // bound source BEFORE the boundary marker. The LLM reads
                // the binding as "the canonical source for this session",
                // the marker as "and this turn is a fresh task", then
                // finally any URL enrichment as "oh and there's also
                // this fresh URL content". Ordering matters — the
                // binding is top-priority.
                if let Some((source, resolved)) = &bound_body {
                    // R1.1: gate the bound-source body on `is_article`.
                    // For non-article raws (wechat-text, kefu-text,
                    // archived-link-only), push the sentinel system
                    // message that tells the LLM the source has no
                    // fetched body and instructs it to ask the user to
                    // refetch / paste — instead of pretending to summarize
                    // an empty body.
                    eprintln!(
                        "[bind_source] injecting bound {} ({}) into system_prompt (agentic, is_article={})",
                        source.display_kind(),
                        source.binding_key(),
                        resolved.is_article,
                    );
                    if resolved.is_article {
                        system_prompt_text.push_str(
                            &crate::ask_context::binding::format_bound_source(
                                source,
                                &resolved.body,
                            ),
                        );
                    } else {
                        system_prompt_text.push_str(
                            &crate::ask_context::binding::format_archived_link_sentinel(
                                source,
                                resolved.kind_label,
                            ),
                        );
                    }
                }
                //
                // A1: insert the mode-specific boundary marker BEFORE the
                // enriched content. For SourceFirst this is the "新任务开始"
                // reset instruction; for Combine it's the "结合历史与新素材"
                // dual-sourcing instruction. FollowUp emits no marker, so
                // the text is appended verbatim as before.
                if let Some(marker) = crate::ask_context::boundary_marker_for(effective_mode) {
                    system_prompt_text.push_str(marker);
                }
                if has_enrichment {
                    eprintln!(
                        "[live_turn] injecting url_context: {} chars into system_prompt (agentic, mode={:?})",
                        enriched.len(),
                        effective_mode
                    );
                    system_prompt_text.push_str("\n\n");
                    system_prompt_text.push_str(&enriched);
                }

                // A4 — Grounded Mode instruction block for the A3 path.
                //
                // A2 (explicit session binding): `format_bound_source`
                // already appended the rules inline with the body, so
                // we skip the tacking-on here to avoid duplicating the
                // instruction list.
                //
                // A3 (auto-bind from URL enrich): the enriched body was
                // pushed via `maybe_enrich_url` / `format_enriched_source`
                // above, which does NOT carry Grounded Mode rules. Append
                // the standalone instruction block so the LLM operates
                // under the same guardrails A2 gets. The block lands
                // at the very end of the system prompt — after the
                // enriched content — so the "上方" phrasing in the
                // rules still correctly points at the material the
                // LLM just read.
                if bound_body.is_none() && auto_bound_source.is_some() {
                    system_prompt_text
                        .push_str(crate::ask_context::binding::grounding_instruction_block());
                }

                // Load runtime config ONCE and extract both permission mode
                // and hooks. Defaults to WorkspaceWrite (not DangerFullAccess)
                // so write operations trigger the permission dialog.
                //
                // SG-02: The permission mode is captured ONCE at turn start
                // and remains fixed for the entire agentic loop. If a user
                // changes `permissionMode` mid-turn via the UI (which calls
                // set_permission_mode() → writes .claude/settings.json),
                // the in-flight turn will NOT observe the change — it will
                // complete using the mode that was active when the user
                // pressed Send. The next turn will pick up the new mode.
                //
                // This is intentional: re-reading the config on every tool
                // invocation would race with the filesystem and make
                // permission decisions non-deterministic within a turn.
                // The user-facing consequence is documented in
                // docs/audit-lessons.md L-06.
                let (bypass_permissions, hooks_config) = {
                    let loader = ConfigLoader::default_for(&project_path_buf);
                    match loader.load() {
                        Ok(rc) => {
                            let bypass = match rc
                                .permission_mode()
                                .map(permission_mode_from_config)
                                .unwrap_or(PermissionMode::WorkspaceWrite)
                            {
                                PermissionMode::DangerFullAccess => true,
                                _ => false,
                            };
                            // L-12: load hooks from the runtime config.
                            // RuntimeConfig has a feature_config.hooks field
                            // of type RuntimeHookConfig.
                            let hooks = rc.hooks().clone();
                            (bypass, Some(hooks))
                        }
                        Err(_) => (false, None), // Safe default: prompt the user.
                    }
                };

                // Initialize MCP servers on first use (idempotent).
                // This spawns subprocess connections for MCP servers
                // declared in .claw/settings.json and keeps them alive
                // for subsequent tool calls. See docs/audit-lessons.md L-09.
                let _ = self.ensure_mcp_initialized(&project_path_buf).await;

                // Build incremental persistence callback.
                //
                // The callback is invoked synchronously from inside the
                // agentic loop. Each invocation spawns a tokio task that:
                //   1. Updates the in-memory record (write lock, fast)
                //   2. Calls persist() to write the whole store to disk
                //
                // To prevent lost updates when two callbacks fire in rapid
                // succession (e.g., a 20-iteration turn), we serialize the
                // persist jobs via a per-session tokio Mutex. This ensures
                // write-lock acquisition order matches callback invocation
                // order (FIFO). Without this serialization, tasks N and N+1
                // can race on the store write lock, and the older snapshot
                // can overwrite the newer one.
                // In-memory update only — disk flush is deferred to
                // `finalize_agentic_turn` at turn end. Previous code ran
                // a full-store JSON serialize per iteration (see
                // docs/performance-report.md: persist() was ~677µs/call
                // and dominated turn latency). For a 20-iteration turn
                // this is a ~13.5ms savings. Crash recovery is handled
                // by the startup reconcile pass (L-03) + the fact that
                // the message log is always consistent because we push
                // messages before calling the callback.
                //
                // The per-session serial Mutex is kept to preserve
                // update ordering between concurrent spawned tasks.
                let persist_state = state.clone();
                let persist_session_id = session_id.clone();
                let persist_serial = Arc::new(Mutex::new(()));
                let on_iteration_complete: Arc<dyn Fn(&RuntimeSession) + Send + Sync> =
                    Arc::new(move |updated_session: &RuntimeSession| {
                        let s = persist_state.clone();
                        let sid = persist_session_id.clone();
                        let serial = Arc::clone(&persist_serial);
                        let session_snapshot = updated_session.clone();
                        tokio::spawn(async move {
                            let _persist_guard = serial.lock().await;
                            let mut store = s.store.write().await;
                            if let Some(record) = store.sessions.get_mut(&sid) {
                                record.session = session_snapshot;
                                record.metadata.updated_at = unix_timestamp_millis();
                            }
                            // No s.persist().await here — finalize_agentic_turn
                            // handles disk flush at turn end.
                        });
                    });

                // P1-1: throttled session.updated_at bump so long
                // streams don't trip the frontend isStale soft-recovery
                // (30s threshold). The closure uses try_write so stream
                // ticks never block on a write lock — if we can't get
                // the lock this cycle, we'll bump on the next tick.
                let tick_state = state.clone();
                let tick_session_id = session_id.clone();
                let on_stream_tick: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                    if let Ok(mut store) = tick_state.store.try_write() {
                        if let Some(record) = store.sessions.get_mut(&tick_session_id) {
                            record.metadata.updated_at = unix_timestamp_millis();
                        }
                    }
                });

                let config = agentic_loop::AgenticLoopConfig {
                    bridge_base_url,
                    bearer_token: client.bearer_token,
                    model: model_override,
                    project_path: project_path_buf,
                    system_prompt: Some(system_prompt_text),
                    bypass_permissions,
                    on_iteration_complete: Some(on_iteration_complete),
                    on_stream_tick: Some(on_stream_tick),
                    mcp_servers: Vec::new(), // legacy field, unused now
                    hooks: hooks_config,
                    http_client: self.http_client.clone(),
                    mcp_manager: Arc::clone(&self.mcp_manager),
                    mcp_tools: self.mcp_tools.read().await.clone(),
                    // A1: tell the agentic loop how to package history
                    // for this turn. `SourceFirst` truncates to the last
                    // two messages inside `build_api_request`; `FollowUp`
                    // and `Combine` pass the full array.
                    context_mode: effective_mode,
                };

                let mut session_for_loop = session;
                // Only push the original user message (URL only).
                // Do NOT inject enriched content here — the callback
                // writes session_for_loop back to storage, so anything
                // pushed here becomes permanent.
                session_for_loop.messages.push(user_message);

                tokio::spawn(async move {
                    // Drop guard (hoisted to module scope as
                    // `SessionCleanupGuard`): best-effort synchronous
                    // cleanup using try_write so we don't deadlock if
                    // the runtime is shutting down, plus a terminal
                    // Snapshot broadcast so SSE subscribers can recover
                    // immediately if this task unwinds before finalize
                    // runs. If try_write fails, the startup reconciliation
                    // pass (see with_executor) will reset any stuck
                    // session state on the next launch.
                    let mut guard =
                        SessionCleanupGuard::new(state.clone(), session_id.clone(), sender.clone());

                    let result = agentic_loop::run_agentic_loop(
                        session_for_loop,
                        config,
                        sender.clone(),
                        session_id.clone(),
                        permission_gate,
                        cancel_token,
                    )
                    .await;

                    // Normal path: finalize handles cleanup; mark guard as fired.
                    guard.fired = true;
                    state
                        .finalize_agentic_turn(&session_id, result, sender)
                        .await;
                });
            }
            Err(_) => {
                // No managed auth credentials available — fall back to the old
                // synchronous turn executor (no local tool execution).
                //
                // A1 limitation: this fallback path hands off to the
                // vendored `execute_turn`, which builds its own LLM
                // request from the full `session.messages`. We can't
                // trim history here without touching the vendored
                // crate, so `SourceFirst` degrades to FollowUp for
                // the purposes of history packaging on this path.
                // What we *can* do is prepend the mode-specific
                // boundary marker to `url_context` so at least the
                // system-prompt instruction propagates.
                let marker_for_fallback = crate::ask_context::boundary_marker_for(effective_mode);
                // A2 + R1.1: prepend the bound source (if resolved) to
                // the fallback url_context so the vendored turn
                // executor sees it too. Ordering matches the agentic
                // path: binding → marker → enrichment.
                //
                // R1.1: gate on `is_article` so non-article raws emit
                // the sentinel instead of a confidence-feigning bound
                // body. Mirrors the agentic + openai-compat paths.
                let bound_prefix: Option<String> = bound_body.as_ref().map(|(s, r)| {
                    if r.is_article {
                        crate::ask_context::binding::format_bound_source(s, &r.body)
                    } else {
                        crate::ask_context::binding::format_archived_link_sentinel(
                            s,
                            r.kind_label,
                        )
                    }
                });
                let fallback_url_context = match (&bound_prefix, has_enrichment) {
                    (Some(bind), true) => Some(match marker_for_fallback {
                        Some(m) => format!("{bind}{m}\n\n{enriched}"),
                        None => format!("{bind}{enriched}"),
                    }),
                    (Some(bind), false) => Some(match marker_for_fallback {
                        Some(m) => format!("{bind}{m}"),
                        None => bind.clone(),
                    }),
                    (None, true) => Some(match marker_for_fallback {
                        Some(m) => format!("{m}\n\n{enriched}"),
                        None => enriched.clone(),
                    }),
                    (None, false) => {
                        // No enrichment and no binding: if we still
                        // have a marker (e.g. SourceFirst without a
                        // URL), surface it as a standalone url_context
                        // block so it shapes the LLM's answer.
                        marker_for_fallback.map(std::string::ToString::to_string)
                    }
                };

                // A4 — append the Grounded Mode instruction block to
                // the fallback's url_context when the turn has an A3
                // auto-bind (enrichment produced a raw, no A2 binding
                // is pinned). The A2 path's `bound_prefix` already
                // carries the rules inline via `format_bound_source`,
                // so we skip it to avoid duplication. For the vendored
                // turn executor, `url_context` is the sole carrier of
                // system-prompt-adjacent framing — without this append
                // the fallback path would drop Grounded Mode silently.
                let fallback_url_context = if bound_body.is_none() && auto_bound_source.is_some() {
                    let rules = crate::ask_context::binding::grounding_instruction_block();
                    Some(match fallback_url_context {
                        Some(ctx) => format!("{ctx}{rules}"),
                        None => rules.to_string(),
                    })
                } else {
                    fallback_url_context
                };
                // P0-2: the fallback spawn had no drop guard either —
                // if `run_background_turn`'s outer async chain unwinds
                // the session stays stuck in `Running`. Clone the
                // pieces the guard needs here (before `session_id`
                // moves into the method call) and capture a sender
                // for the terminal Snapshot broadcast.
                let guard_state = state.clone();
                let guard_session_id = session_id.clone();
                let guard_sender = sender.clone();
                tokio::spawn(async move {
                    let mut guard =
                        SessionCleanupGuard::new(guard_state, guard_session_id, guard_sender);

                    // Pass original message to turn executor (not enriched).
                    // Enriched content travels inside the request struct so
                    // execute_live_turn can inject it into the system prompt
                    // without racing on a global env var.
                    state
                        .run_background_turn(
                            session_id,
                            session,
                            previous_message_count,
                            DesktopTurnRequest {
                                message: message.clone(),
                                project_path,
                                url_context: fallback_url_context,
                            },
                            turn_executor,
                        )
                        .await;

                    // `run_background_turn` finished normally (it already
                    // resets `turn_state` to Idle and broadcasts a
                    // Snapshot). Suppress the guard's recovery path.
                    guard.fired = true;
                });
            }
        }

        Ok(detail)
    }

    pub async fn subscribe(
        &self,
        session_id: &str,
    ) -> Result<
        (
            DesktopSessionEvent,
            broadcast::Receiver<DesktopSessionEvent>,
        ),
        DesktopStateError,
    > {
        let store = self.store.read().await;
        let record = store
            .sessions
            .get(session_id)
            .ok_or_else(|| DesktopStateError::SessionNotFound(session_id.to_string()))?;
        Ok((
            DesktopSessionEvent::Snapshot {
                session: record.detail(),
            },
            record.events.subscribe(),
        ))
    }

    /// Fan-out a session-agnostic event (e.g. `AbsorbProgress`,
    /// `AbsorbComplete`) to every live session's broadcast channel.
    /// Each active SSE subscriber (one per session via
    /// `stream_session_events`) then receives the event regardless of
    /// which session the client happened to open — matching the §2.1
    /// spec line "SSE 事件 (通过 `/api/desktop/sessions/{id}/events`)".
    ///
    /// Session-scoped events (Snapshot / Message / PermissionRequest /
    /// TextDelta) do NOT use this helper — they're sent to a single
    /// session's sender via the session record directly, since those
    /// events carry an explicit `session_id`.
    ///
    /// Backpressure: each send is `broadcast::Sender::send` which
    /// returns `Err(SendError)` when there are zero receivers — we
    /// silently drop in that case (no connected SSE client on that
    /// session). Slow receivers eventually see `Lagged` in their recv
    /// loop (handled by `stream_session_events`).
    pub async fn broadcast_session_event(&self, event: DesktopSessionEvent) {
        let _ = self.skill_events.send(event.clone());
        let store = self.store.read().await;
        for record in store.sessions.values() {
            // Cloning the event per session is O(n) but the event
            // payload is small (progress / complete counters); for
            // realistic session counts (<20) this is negligible.
            let _ = record.events.send(event.clone());
        }
    }

    async fn persist(&self) {
        let Some(persistence) = &self.persistence else {
            return;
        };

        let snapshot = {
            let store = self.store.read().await;
            PersistedDesktopState::from_store(
                self.next_session_id.load(Ordering::Relaxed),
                &store.sessions,
            )
        };
        let persistence = Arc::clone(persistence);

        match tokio::task::spawn_blocking(move || persistence.save(&snapshot)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("desktop persistence save failed: {error}"),
            Err(error) => eprintln!("desktop persistence worker crashed: {error}"),
        }
    }

    async fn persist_scheduled(&self) {
        let Some(persistence) = &self.scheduled_persistence else {
            return;
        };

        let snapshot = {
            let store = self.scheduled_store.read().await;
            PersistedDesktopScheduledState {
                next_task_id: self.next_task_id.load(Ordering::Relaxed),
                tasks: store.tasks.values().cloned().collect(),
            }
        };
        let persistence = Arc::clone(persistence);

        match tokio::task::spawn_blocking(move || persistence.save(&snapshot)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("desktop scheduled save failed: {error}"),
            Err(error) => eprintln!("desktop scheduled worker crashed: {error}"),
        }
    }

    async fn persist_dispatch(&self) {
        let Some(persistence) = &self.dispatch_persistence else {
            return;
        };

        let snapshot = {
            let store = self.dispatch_store.read().await;
            PersistedDesktopDispatchState {
                next_item_id: self.next_dispatch_item_id.load(Ordering::Relaxed),
                items: store.items.values().cloned().collect(),
            }
        };
        let persistence = Arc::clone(persistence);

        match tokio::task::spawn_blocking(move || persistence.save(&snapshot)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("desktop dispatch save failed: {error}"),
            Err(error) => eprintln!("desktop dispatch worker crashed: {error}"),
        }
    }

    async fn get_scheduled_task(
        &self,
        task_id: &str,
    ) -> Result<DesktopScheduledTask, DesktopStateError> {
        let session_context = self.session_context().await;
        let now = unix_timestamp_millis();
        let store = self.scheduled_store.read().await;
        let task = store
            .tasks
            .get(task_id)
            .ok_or_else(|| DesktopStateError::ScheduledTaskNotFound(task_id.to_string()))?;
        Ok(build_scheduled_task(task, &session_context, now))
    }

    async fn get_dispatch_item(
        &self,
        item_id: &str,
    ) -> Result<DesktopDispatchItem, DesktopStateError> {
        let session_context = self.session_context().await;
        let store = self.dispatch_store.read().await;
        let item = store
            .items
            .get(item_id)
            .ok_or_else(|| DesktopStateError::DispatchItemNotFound(item_id.to_string()))?;
        Ok(build_dispatch_item(item, &session_context))
    }

    fn ensure_scheduler(&self) {
        if self
            .scheduler_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let state = self.clone();
            tokio::spawn(async move {
                state.scheduler_loop().await;
            });
        }
    }

    async fn scheduler_loop(self) {
        loop {
            let due_task_ids = self.collect_due_scheduled_task_ids().await;
            for task_id in due_task_ids {
                if let Err(error) = self.start_scheduled_task_run(&task_id, false).await {
                    if !matches!(
                        error,
                        DesktopStateError::ScheduledTaskBusy(_)
                            | DesktopStateError::ScheduledTaskNotFound(_)
                    ) {
                        eprintln!("scheduled task `{task_id}` could not start: {error}");
                    }
                }
            }
            sleep(Duration::from_secs(30)).await;
        }
    }

    async fn collect_due_scheduled_task_ids(&self) -> Vec<String> {
        let session_context = self.session_context().await;
        let now = unix_timestamp_millis();
        let store = self.scheduled_store.read().await;
        store
            .tasks
            .values()
            .filter(|task| is_task_due(task, &session_context, now))
            .map(|task| task.id.clone())
            .collect()
    }

    async fn start_scheduled_task_run(
        &self,
        task_id: &str,
        manual: bool,
    ) -> Result<DesktopScheduledTask, DesktopStateError> {
        let session_context = self.session_context().await;
        let task = {
            let mut store = self.scheduled_store.write().await;
            let task = store
                .tasks
                .get_mut(task_id)
                .ok_or_else(|| DesktopStateError::ScheduledTaskNotFound(task_id.to_string()))?;

            if task.running {
                return Err(DesktopStateError::ScheduledTaskBusy(task_id.to_string()));
            }

            if let Some(blocked_reason) = scheduled_task_blocked_reason(task, &session_context) {
                return Err(DesktopStateError::InvalidScheduledTask(blocked_reason));
            }

            if !manual && !task.enabled {
                return Err(DesktopStateError::InvalidScheduledTask(
                    "scheduled task is disabled".to_string(),
                ));
            }

            task.running = true;
            task.updated_at = unix_timestamp_millis();
            task.clone()
        };

        self.persist_scheduled().await;

        let state = self.clone();
        tokio::spawn(async move {
            state.execute_scheduled_task_run(task).await;
        });

        self.get_scheduled_task(task_id).await
    }

    async fn execute_scheduled_task_run(&self, task: ScheduledTaskMetadata) {
        let started_at = unix_timestamp_millis();
        let outcome = if let Some(session_id) = &task.target_session_id {
            // A1: scheduled tasks run on the FollowUp default (no
            // per-task mode selector today). Passing `None` keeps
            // the legacy behaviour unchanged.
            match self
                .append_user_message(session_id, task.prompt.clone(), None)
                .await
            {
                Ok(session) => ScheduledTaskRunOutcome::success(
                    Some(session.id.clone()),
                    format!("Queued into {}.", session.title),
                ),
                Err(error) => ScheduledTaskRunOutcome::error(error.to_string()),
            }
        } else {
            let session = self
                .create_session(CreateDesktopSessionRequest {
                    title: Some(task.title.clone()),
                    project_name: Some(task.project_name.clone()),
                    project_path: Some(task.project_path.clone()),
                })
                .await;

            match self
                .append_user_message(&session.id, task.prompt.clone(), None)
                .await
            {
                Ok(session) => ScheduledTaskRunOutcome::success(
                    Some(session.id.clone()),
                    format!("Started a fresh session: {}.", session.title),
                ),
                Err(error) => ScheduledTaskRunOutcome::error(error.to_string()),
            }
        };

        {
            let mut store = self.scheduled_store.write().await;
            let Some(record) = store.tasks.get_mut(&task.id) else {
                return;
            };
            record.running = false;
            record.updated_at = unix_timestamp_millis();
            record.last_run_at = Some(started_at);
            record.last_run_status = Some(if outcome.is_error {
                DesktopScheduledRunStatus::Error
            } else {
                DesktopScheduledRunStatus::Success
            });
            record.last_outcome = Some(outcome.message);
        }

        self.persist_scheduled().await;
    }

    async fn execute_dispatch_delivery(
        &self,
        item: &DispatchItemMetadata,
    ) -> Result<SessionId, DesktopStateError> {
        let target_session_id = match &item.target_session_id {
            Some(session_id) if !item.prefer_new_session => session_id.clone(),
            _ => {
                self.create_session(CreateDesktopSessionRequest {
                    title: Some(item.title.clone()),
                    project_name: Some(item.project_name.clone()),
                    project_path: Some(item.project_path.clone()),
                })
                .await
                .id
            }
        };

        self.append_user_message(&target_session_id, item.body.clone(), None)
            .await?;
        Ok(target_session_id)
    }

    async fn session_context(&self) -> DesktopSessionContext {
        let store = self.store.read().await;
        let mut trusted_project_paths = BTreeSet::from([default_project_path().to_string()]);
        let mut session_titles = HashMap::new();
        for record in store.sessions.values() {
            trusted_project_paths.insert(record.metadata.project_path.clone());
            session_titles.insert(record.metadata.id.clone(), record.metadata.title.clone());
        }
        DesktopSessionContext {
            trusted_project_paths,
            session_titles,
        }
    }

    async fn current_project_path(&self) -> String {
        let store = self.store.read().await;
        store
            .sessions
            .values()
            .max_by_key(|record| record.metadata.updated_at)
            .map(|record| record.metadata.project_path.clone())
            .unwrap_or_else(|| default_project_path().to_string())
    }

    async fn run_background_turn(
        &self,
        session_id: SessionId,
        session: RuntimeSession,
        previous_message_count: usize,
        request: DesktopTurnRequest,
        turn_executor: Arc<dyn DesktopTurnExecutor + Send + Sync>,
    ) {
        let _turn_guard = self.turn_lock.lock().await;
        let fallback_session = session.clone();
        let fallback_message = request.message.clone();
        let request_for_worker = request.clone();

        let turn_result = tokio::task::spawn_blocking(move || {
            turn_executor.execute_turn(session, request_for_worker)
        })
        .await
        .unwrap_or_else(|error| {
            fallback_turn_result(
                fallback_session,
                &fallback_message,
                DEFAULT_MODEL_LABEL.to_string(),
                format!("desktop runtime task crashed: {error}"),
            )
        });

        let (detail, sender, new_messages) = {
            let mut store = self.store.write().await;
            let Some(record) = store.sessions.get_mut(&session_id) else {
                return;
            };

            record.metadata.updated_at = unix_timestamp_millis();
            record.metadata.bucket = DesktopSessionBucket::Today;
            record.metadata.turn_state = DesktopTurnState::Idle;
            record.metadata.model_label = turn_result.model_label.clone();
            record.session = turn_result.session;

            let new_messages = record
                .session
                .messages
                .iter()
                .skip(previous_message_count + 1)
                .cloned()
                .collect::<Vec<_>>();

            (record.detail(), record.events.clone(), new_messages)
        };

        self.persist().await;

        for message in new_messages {
            let _ = sender.send(DesktopSessionEvent::Message {
                session_id: session_id.clone(),
                message: DesktopConversationMessage::from(&message),
            });
        }
        let _ = sender.send(DesktopSessionEvent::Snapshot { session: detail });
    }

    /// Finalize an agentic loop turn: update session store, set Idle, persist, broadcast.
    async fn finalize_agentic_turn(
        &self,
        session_id: &str,
        result: Result<agentic_loop::AgenticTurnResult, agentic_loop::AgenticError>,
        sender: broadcast::Sender<DesktopSessionEvent>,
    ) {
        // Clean up per-session state.
        self.permission_gates.write().await.remove(session_id);
        self.cancel_tokens.write().await.remove(session_id);

        let (new_session, model_label) = match result {
            Ok(turn) => (turn.session, turn.model_label),
            Err(error) => {
                eprintln!("agentic loop error for session {session_id}: {error}");
                // On error, broadcast an error message and reset to Idle.
                let error_message = ConversationMessage {
                    role: runtime::MessageRole::Assistant,
                    blocks: vec![runtime::ContentBlock::Text {
                        text: format!("Error: {error}"),
                    }],
                    usage: None,
                };
                let _ = sender.send(DesktopSessionEvent::Message {
                    session_id: session_id.to_string(),
                    message: DesktopConversationMessage::from(&error_message),
                });
                // Try to get the current session to return.
                let store = self.store.read().await;
                if let Some(record) = store.sessions.get(session_id) {
                    let mut session = record.session.clone();
                    session.push_message(error_message).unwrap_or_default();
                    (session, DEFAULT_MODEL_LABEL.to_string())
                } else {
                    return;
                }
            }
        };

        // Update session store.
        let detail = {
            let mut store = self.store.write().await;
            let Some(record) = store.sessions.get_mut(session_id) else {
                return;
            };
            record.metadata.updated_at = unix_timestamp_millis();
            record.metadata.bucket = DesktopSessionBucket::Today;
            record.metadata.turn_state = DesktopTurnState::Idle;
            record.metadata.model_label = model_label;
            // Auto-transition to NeedsReview when turn ends successfully,
            // so the inbox flags it for user attention. If user already
            // marked as Done/Archived, don't override.
            if record.metadata.lifecycle_status == DesktopLifecycleStatus::InProgress {
                record.metadata.lifecycle_status = DesktopLifecycleStatus::NeedsReview;
            }
            record.session = new_session;
            record.detail()
        };

        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot { session: detail });
    }

    /// A5.1 — Finalize an OpenAI-compat streaming turn: persist the
    /// accumulated assistant text into the session, emit a Message
    /// event so the frontend promotes the streaming buffer to a
    /// permanent bubble, and broadcast the final Snapshot.
    ///
    /// `previous_message_count` is the length of `session.messages`
    /// BEFORE the user message for this turn was pushed. We broadcast
    /// only new assistant-side messages; the user turn is already
    /// carried in the POST /sessions/:id/messages response and does
    /// not need SSE replay (see P1-1: replaying it produced a
    /// duplicate bubble on the frontend).
    async fn finalize_openai_compat_turn(
        &self,
        session_id: &str,
        _session: RuntimeSession,
        previous_message_count: usize,
        result: Result<(Vec<ConversationMessage>, Option<String>), String>,
        sender: broadcast::Sender<DesktopSessionEvent>,
        model_label_hint: String,
    ) {
        // Push the user's message if the caller didn't already (they
        // didn't — `session` is the pre-turn clone).
        // Note: the outer `append_user_message` already persisted the
        // user message into the session at line 3757 via
        // `record.session.messages.push(user_message.clone())` BEFORE
        // cloning. Here we only need to add the assistant reply.

        let (messages_to_push, upstream_model, error_text) = match result {
            Ok((messages, model)) => (messages, model, None),
            Err(error) => {
                eprintln!("[openai-compat-stream] session={session_id} turn failed: {error}");
                let err_msg = ConversationMessage {
                    role: runtime::MessageRole::Assistant,
                    blocks: vec![runtime::ContentBlock::Text {
                        text: format!("Desktop runtime couldn't stream this turn.\n\n{error}"),
                    }],
                    usage: None,
                };
                (vec![err_msg], None, Some(error))
            }
        };

        // Prefer the upstream-echoed model id (OpenAI's chunks carry
        // `.model`) over the placeholder from providers.json.
        let final_label = match (upstream_model, &error_text) {
            (Some(m), None) => {
                // Keep the human-friendly provider prefix, swap in the
                // real model: e.g. "Moonshot (Kimi) · moonshot-v1-8k".
                match model_label_hint.split_once(" · ") {
                    Some((p, _)) => format!("{p} · {m}"),
                    None => format!("{model_label_hint} · {m}"),
                }
            }
            _ => model_label_hint,
        };

        let detail = {
            let mut store = self.store.write().await;
            let Some(record) = store.sessions.get_mut(session_id) else {
                return;
            };
            let mut final_session = record.session.clone();
            for msg in &messages_to_push {
                final_session.push_message(msg.clone()).unwrap_or_default();
            }
            record.metadata.updated_at = unix_timestamp_millis();
            record.metadata.bucket = DesktopSessionBucket::Today;
            record.metadata.turn_state = DesktopTurnState::Idle;
            record.metadata.model_label = final_label;
            if record.metadata.lifecycle_status == DesktopLifecycleStatus::InProgress {
                record.metadata.lifecycle_status = DesktopLifecycleStatus::NeedsReview;
            }
            record.session = final_session;
            record.detail()
        };

        self.persist().await;

        // Broadcast only new assistant-side messages (P1-1 fix).
        // The user turn at index `previous_message_count` is already
        // returned by POST /sessions/:id/messages, so re-broadcasting
        // it via SSE would produce a duplicate bubble on the frontend.
        // We skip one extra element past the user turn.
        let store = self.store.read().await;
        if let Some(record) = store.sessions.get(session_id) {
            let tail = record
                .session
                .messages
                .iter()
                .skip(previous_message_count + 1)
                .cloned()
                .collect::<Vec<_>>();
            drop(store);
            for message in tail {
                let _ = sender.send(DesktopSessionEvent::Message {
                    session_id: session_id.to_string(),
                    message: DesktopConversationMessage::from(&message),
                });
            }
        }
        let _ = sender.send(DesktopSessionEvent::Snapshot { session: detail });
    }
}

impl Default for DesktopState {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopSessionRecord {
    fn detail(&self) -> DesktopSessionDetail {
        DesktopSessionDetail {
            id: self.metadata.id.clone(),
            title: self.metadata.title.clone(),
            preview: self.metadata.preview.clone(),
            created_at: self.metadata.created_at,
            updated_at: self.metadata.updated_at,
            project_name: self.metadata.project_name.clone(),
            project_path: self.metadata.project_path.clone(),
            environment_label: self.metadata.environment_label.clone(),
            model_label: self.metadata.model_label.clone(),
            turn_state: self.metadata.turn_state,
            lifecycle_status: self.metadata.lifecycle_status,
            flagged: self.metadata.flagged,
            session: DesktopSessionData::from(&self.session),
            // `enrich_status` is a per-turn side-channel (see
            // `append_user_message`); the default builder leaves it
            // `None` and only that function populates it.
            enrich_status: None,
            // A1.1: `context_basis` now sources from the persistent
            // `SessionMetadata.last_context_basis` — stamped by
            // `append_user_message` on each new user turn, survives
            // reload / SSE reconnect / idle refresh, and is
            // overwritten when the next turn starts. Legacy sessions
            // (pre-A1.1 on-disk) lack the field and deserialize to
            // `None` via `#[serde(default)]` — UI tolerates null.
            context_basis: self.metadata.last_context_basis.clone(),
            // A2: session-level binding, echoed on every detail
            // build (list/get/SSE/snapshot). Persists across turns
            // until `clear_source_binding` or `bind_source` with a
            // new ref runs.
            source_binding: self.metadata.source_binding.clone(),
        }
    }

    fn summary(&self) -> DesktopSessionSummary {
        DesktopSessionSummary {
            id: self.metadata.id.clone(),
            title: self.metadata.title.clone(),
            preview: self.metadata.preview.clone(),
            bucket: self.metadata.bucket,
            created_at: self.metadata.created_at,
            updated_at: self.metadata.updated_at,
            project_name: self.metadata.project_name.clone(),
            project_path: self.metadata.project_path.clone(),
            environment_label: self.metadata.environment_label.clone(),
            model_label: self.metadata.model_label.clone(),
            turn_state: self.metadata.turn_state,
            lifecycle_status: self.metadata.lifecycle_status,
            flagged: self.metadata.flagged,
        }
    }

    fn search_hit(&self, normalized_query: &str) -> Option<DesktopSearchHit> {
        let snippet = session_search_snippet(self, normalized_query)?;
        Some(DesktopSearchHit {
            session_id: self.metadata.id.clone(),
            title: self.metadata.title.clone(),
            project_name: self.metadata.project_name.clone(),
            project_path: self.metadata.project_path.clone(),
            bucket: self.metadata.bucket,
            preview: self.metadata.preview.clone(),
            snippet,
            updated_at: self.metadata.updated_at,
        })
    }
}

impl DesktopCustomizeState {
    fn empty(project_path: String) -> Self {
        Self {
            project_path,
            model_id: DEFAULT_MODEL_ID.to_string(),
            model_label: DEFAULT_MODEL_LABEL.to_string(),
            permission_mode: DEFAULT_PERMISSION_MODE_LABEL.to_string(),
            summary: DesktopCustomizeSummary {
                loaded_config_count: 0,
                mcp_server_count: 0,
                plugin_count: 0,
                enabled_plugin_count: 0,
                plugin_tool_count: 0,
                pre_tool_hook_count: 0,
                post_tool_hook_count: 0,
            },
            loaded_configs: Vec::new(),
            hooks: DesktopHookConfigView {
                pre_tool_use: Vec::new(),
                post_tool_use: Vec::new(),
            },
            mcp_servers: Vec::new(),
            plugins: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn empty_with_warning(project_path: String, warning: String) -> Self {
        let mut state = Self::empty(project_path);
        state.warnings.push(warning);
        state
    }
}

impl DesktopPersistence {
    fn default_path() -> PathBuf {
        ConfigLoader::default_for(default_project_path())
            .config_home()
            .join("desktop")
            .join("sessions.json")
    }

    fn load(&self) -> Result<Option<PersistedDesktopState>, String> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };

        serde_json::from_str::<PersistedDesktopState>(&contents)
            .map_err(|error| error.to_string())
            .map(Some)
    }

    fn save(&self, state: &PersistedDesktopState) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let payload = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
        fs::write(&self.path, payload).map_err(|error| error.to_string())
    }
}

impl DesktopScheduledPersistence {
    fn default_path() -> PathBuf {
        ConfigLoader::default_for(default_project_path())
            .config_home()
            .join("desktop")
            .join("scheduled.json")
    }

    fn load(&self) -> Result<Option<PersistedDesktopScheduledState>, String> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };

        serde_json::from_str::<PersistedDesktopScheduledState>(&contents)
            .map_err(|error| error.to_string())
            .map(Some)
    }

    fn save(&self, state: &PersistedDesktopScheduledState) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let payload = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
        fs::write(&self.path, payload).map_err(|error| error.to_string())
    }
}

impl DesktopDispatchPersistence {
    fn default_path() -> PathBuf {
        ConfigLoader::default_for(default_project_path())
            .config_home()
            .join("desktop")
            .join("dispatch.json")
    }

    fn load(&self) -> Result<Option<PersistedDesktopDispatchState>, String> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };

        serde_json::from_str::<PersistedDesktopDispatchState>(&contents)
            .map_err(|error| error.to_string())
            .map(Some)
    }

    fn save(&self, state: &PersistedDesktopDispatchState) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let payload = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
        fs::write(&self.path, payload).map_err(|error| error.to_string())
    }
}

impl Default for DesktopPersistence {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
        }
    }
}

impl Default for DesktopScheduledPersistence {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
        }
    }
}

impl Default for DesktopDispatchPersistence {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
        }
    }
}

impl PersistedDesktopState {
    fn from_store(
        next_session_id: u64,
        sessions: &HashMap<SessionId, DesktopSessionRecord>,
    ) -> Self {
        let mut sessions = sessions
            .values()
            .cloned()
            .map(PersistedDesktopSession::from)
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.metadata.created_at.cmp(&right.metadata.created_at));

        Self {
            next_session_id,
            sessions,
        }
    }

    fn into_records(self) -> Vec<DesktopSessionRecord> {
        self.sessions
            .into_iter()
            .map(PersistedDesktopSession::into_record)
            .collect()
    }
}

impl From<DesktopSessionRecord> for PersistedDesktopSession {
    fn from(value: DesktopSessionRecord) -> Self {
        Self {
            metadata: value.metadata,
            session: DesktopSessionData::from(&value.session),
        }
    }
}

impl PersistedDesktopSession {
    fn into_record(self) -> DesktopSessionRecord {
        let (events, _) = broadcast::channel(BROADCAST_CAPACITY);
        let mut metadata = self.metadata;
        metadata.turn_state = DesktopTurnState::Idle;
        let session = self.session.into_runtime_session_with_metadata(&metadata);
        DesktopSessionRecord {
            metadata,
            session,
            events,
        }
    }
}

// ── Phase 6C: WeChat account management API ────────────────────────
//
// Methods on DesktopState that manage the lifecycle of WeChat iLink
// monitors (spawn/stop/list) and the pending QR login flows. Called
// by the HTTP handlers in desktop-server/src/lib.rs. All methods here
// are cheap — the heavy work (long-polling, login waiting) runs in
// background tokio tasks.

/// R1.2 reliability gate · replay one outbox entry through its
/// transport. Called once per due entry per worker tick from
/// [`DesktopState::spawn_wechat_outbox_worker`].
///
/// Steps:
///   1. Dispatch on `entry.transport`. R1.2 ships kefu only;
///      iLink is a follow-up additive variant.
///   2. Reconstruct the transport client from on-disk config.
///      `KefuClient::new` is cheap (arc + http client); rebuilding
///      it every tick gives up the access-token cache for
///      simplicity — 30s ticks make refresh cost negligible.
///   3. Claim → send → mark with classified error.
///
/// Failure modes that translate to terminal `Failed`:
///   * Config missing (kefu config deleted, account un-enrolled) →
///     `session_expired` kind, immediate Failed.
///   * `errcode=…` in error string → `api` kind, immediate Failed.
///
/// Failure modes that bounce back to `Pending` with backoff:
///   * Generic transport / timeout errors below
///     `MAX_OUTBOX_ATTEMPTS`.
async fn replay_outbox_entry(
    paths: &wiki_store::WikiPaths,
    entry: &wiki_store::wechat_outbox::OutboxEntry,
) {
    use wiki_store::wechat_outbox::{
        mark_outbox_failed, mark_outbox_sending, mark_outbox_sent, OutboxLastError,
        OutboxTransport,
    };

    match &entry.transport {
        OutboxTransport::Kefu {
            external_userid,
            open_kfid,
            msgid: _,
        } => {
            // Step 1: ensure the kefu config is present. Without it
            // we cannot construct a client → terminal-fail with a
            // clear "needs re-login" reason so the UI can guide the
            // user.
            let config = match wechat_kefu::load_config().ok().flatten() {
                Some(c) => c,
                None => {
                    let err = OutboxLastError {
                        kind: "session_expired".to_string(),
                        message: "kefu config missing — re-login required".to_string(),
                    };
                    let _ = mark_outbox_failed(paths, entry.id, err);
                    return;
                }
            };
            let client = wechat_kefu::KefuClient::new(&config.corpid, &config.secret);

            // Step 2: claim. Race with another worker / handler is
            // safe — the `mark_outbox_sending` call is mutex-guarded
            // and refuses non-`Pending` rows.
            if let Err(e) = mark_outbox_sending(paths, entry.id, None) {
                eprintln!(
                    "[outbox worker] claim failed for #{}: {e} (likely raced with another tick)",
                    entry.id
                );
                return;
            }

            // Step 3: actual network call.
            match client.send_text(external_userid, open_kfid, &entry.content).await {
                Ok(()) => {
                    if let Err(e) = mark_outbox_sent(paths, entry.id) {
                        eprintln!(
                            "[outbox worker] mark_sent failed for #{}: {e}",
                            entry.id
                        );
                    } else {
                        eprintln!(
                            "[outbox worker] replay #{} → Sent (attempt {})",
                            entry.id,
                            entry.attempts + 1
                        );
                    }
                }
                Err(send_err) => {
                    let msg = send_err.to_string();
                    let kind = if msg.contains("errcode=") {
                        "api"
                    } else if msg.to_lowercase().contains("timeout") {
                        "timeout"
                    } else {
                        "transport"
                    };
                    let outbox_err = OutboxLastError {
                        kind: kind.to_string(),
                        message: msg,
                    };
                    eprintln!(
                        "[outbox worker] replay #{} failed ({}): {} — backoff or terminal",
                        entry.id, outbox_err.kind, outbox_err.message
                    );
                    let _ = mark_outbox_failed(paths, entry.id, outbox_err);
                }
            }
        }
    }
}

impl DesktopState {
    /// Spawn a long-poll WeChat monitor for `account_id`, wiring it
    /// to this state's session store via a [`DesktopAgentHandler`].
    ///
    /// Idempotent: if a monitor is already registered for this id,
    /// cancels the previous one before spawning a new one. Returns
    /// the cancellation token so callers can stop the monitor later.
    ///
    /// Called by:
    /// * `desktop-server` main at startup for each persisted account
    /// * The QR-login background task, once it observes a Confirmed
    ///   status, so the user can chat with the new bot immediately
    pub async fn spawn_wechat_monitor(&self, account_id: &str) -> Result<(), String> {
        use wechat_ilink::{
            account::load_account,
            desktop_handler::DesktopAgentHandler,
            monitor::{run_monitor, MessageHandler, MonitorConfig, MonitorStatus},
            types::DEFAULT_BASE_URL,
            IlinkClient,
        };

        // If an old monitor exists for this id, cancel it first so we
        // don't end up with two tasks racing on the same cursor.
        if let Some(existing) = self.wechat_monitors.write().await.remove(account_id) {
            existing.cancel.cancel();
        }

        let data = load_account(account_id)
            .map_err(|e| format!("load_account({account_id}) failed: {e}"))?
            .ok_or_else(|| format!("account {account_id} listed but file is missing"))?;

        let token = data
            .token
            .clone()
            .ok_or_else(|| format!("account {account_id} has no bot_token"))?;
        let base_url = data
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let client = IlinkClient::new(base_url, token)
            .map_err(|e| format!("IlinkClient::new failed: {e}"))?;
        // Batch-C §3: derive from server shutdown_cancel so graceful
        // shutdown cascades. Falls back to a root token when the server
        // didn't inject one (tests / standalone use).
        let cancel = self.monitor_cancel_token().await;
        let (status_tx, status_rx) = tokio::sync::watch::channel(MonitorStatus::default());

        let project_path = self.wechat_default_project_path.read().await.clone();

        // S5 D2 override: attach the canonical wiki paths so every
        // inbound WeChat text message is also written to
        // `~/.clawwiki/raw/` and queued into the Inbox. We resolve the
        // root on each spawn so tests that manipulate CLAWWIKI_HOME
        // between spawns see the current value.
        let wiki_root = wiki_store::default_root();
        if let Err(err) = wiki_store::init_wiki(&wiki_root) {
            eprintln!(
                "[wechat] warn: wiki_store::init_wiki({:?}) failed: {err}",
                wiki_root
            );
        }
        let wiki_paths = wiki_store::WikiPaths::resolve(&wiki_root);
        let handler: Arc<dyn MessageHandler> = Arc::new(
            DesktopAgentHandler::new(self.clone(), account_id, project_path)
                .map_err(|e| format!("DesktopAgentHandler::new failed: {e}"))?
                .with_wiki_paths(wiki_paths),
        );

        let config = MonitorConfig {
            account_id: account_id.to_string(),
            client,
            handler,
            cancel: cancel.clone(),
        };

        eprintln!("[wechat] spawning monitor for account={account_id} (via DesktopState)");
        tokio::spawn(async move {
            run_monitor(config, status_tx).await;
        });

        self.wechat_monitors.write().await.insert(
            account_id.to_string(),
            WeChatMonitorHandle { cancel, status_rx },
        );
        Ok(())
    }

    /// Spawn monitors for every persisted account. Called once by
    /// `desktop-server` at startup. Silent no-op when no accounts
    /// exist. Errors on individual accounts are logged and swallowed
    /// so one bad account can't take down startup.
    pub async fn spawn_wechat_monitors_for_all_accounts(&self) {
        let ids = match wechat_ilink::account::list_account_ids() {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("[wechat] failed to list accounts: {e}");
                return;
            }
        };
        if ids.is_empty() {
            eprintln!("[wechat] no accounts persisted yet — skipping monitor startup");
            return;
        }
        for account_id in ids {
            if let Err(e) = self.spawn_wechat_monitor(&account_id).await {
                eprintln!("[wechat] could not spawn monitor for {account_id}: {e}");
            }
        }
    }

    /// Stop the monitor for `account_id` (idempotent — no-op if not
    /// registered). Used by the delete-account HTTP route.
    pub async fn stop_wechat_monitor(&self, account_id: &str) {
        if let Some(handle) = self.wechat_monitors.write().await.remove(account_id) {
            handle.cancel.cancel();
            eprintln!("[wechat] cancelled monitor for account={account_id}");
        }
    }

    /// List all persisted WeChat accounts with a summary suitable for
    /// the settings UI. Combines on-disk account data with the
    /// in-memory monitor state to compute a rough connection status.
    pub async fn list_wechat_accounts_summary(&self) -> Vec<WeChatAccountInfo> {
        let ids = match wechat_ilink::account::list_account_ids() {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("[wechat] list_account_ids failed: {e}");
                return Vec::new();
            }
        };
        let monitors = self.wechat_monitors.read().await;
        let mut out = Vec::new();
        for id in ids {
            let data = match wechat_ilink::account::load_account(&id) {
                Ok(Some(d)) => d,
                Ok(None) => continue,
                Err(e) => {
                    eprintln!("[wechat] load_account({id}) failed: {e}");
                    continue;
                }
            };
            let bot_token_preview = data
                .token
                .as_deref()
                .map(format_bot_token_preview)
                .unwrap_or_else(|| "(no token)".to_string());
            let status = if monitors.contains_key(&id) {
                WeChatConnectionStatus::Connected
            } else {
                WeChatConnectionStatus::Disconnected
            };
            out.push(WeChatAccountInfo {
                id: id.clone(),
                display_name: data.user_id.clone().unwrap_or_else(|| id.clone()),
                base_url: data
                    .base_url
                    .clone()
                    .unwrap_or_else(|| wechat_ilink::DEFAULT_BASE_URL.to_string()),
                bot_token_preview,
                saved_at: data.saved_at.clone(),
                status,
            });
        }
        out
    }

    /// Remove a WeChat account completely:
    /// 1. cancels its monitor
    /// 2. deletes its credential files from disk
    /// Returns `Ok(())` even if nothing was running (idempotent).
    pub async fn remove_wechat_account(&self, account_id: &str) -> Result<(), String> {
        self.stop_wechat_monitor(account_id).await;
        wechat_ilink::account::clear_account(account_id)
            .map_err(|e| format!("clear_account failed: {e}"))?;
        Ok(())
    }

    /// Snapshot every registered ilink monitor's [`wechat_ilink::MonitorStatus`].
    /// Ordering is arbitrary — the consumer in the HTTP health route
    /// aggregates across entries so the sequence doesn't matter. An
    /// empty vec means no monitors are currently registered.
    pub async fn wechat_ilink_monitor_statuses(&self) -> Vec<wechat_ilink::MonitorStatus> {
        let monitors = self.wechat_monitors.read().await;
        monitors
            .values()
            .map(|handle| handle.status_rx.borrow().clone())
            .collect()
    }

    // ===================================================================
    // Channel B: Official WeChat Customer Service (kefu)
    // ===================================================================

    pub async fn save_kefu_config(&self, config: wechat_kefu::KefuConfig) -> Result<(), String> {
        wechat_kefu::save_config(&config).map_err(|e| format!("save_config failed: {e}"))
    }

    pub async fn load_kefu_config(&self) -> Result<Option<wechat_kefu::KefuConfig>, String> {
        wechat_kefu::load_config().map_err(|e| format!("load_config failed: {e}"))
    }

    pub async fn create_kefu_account(&self, name: &str) -> Result<String, String> {
        let config = self
            .load_kefu_config()
            .await?
            .ok_or_else(|| "kefu config not found — save config first".to_string())?;
        let client = wechat_kefu::KefuClient::new(&config.corpid, &config.secret);
        let open_kfid = client
            .create_account(name)
            .await
            .map_err(|e| format!("create_account failed: {e}"))?;

        // Persist the open_kfid back into config
        let mut updated = config;
        updated.open_kfid = Some(open_kfid.clone());
        updated.account_name = Some(name.to_string());
        self.save_kefu_config(updated).await?;
        Ok(open_kfid)
    }

    pub async fn get_kefu_contact_url(&self) -> Result<String, String> {
        let config = self
            .load_kefu_config()
            .await?
            .ok_or_else(|| "kefu config not found".to_string())?;
        let open_kfid = config
            .open_kfid
            .as_deref()
            .ok_or_else(|| "no open_kfid — create account first".to_string())?;
        let client = wechat_kefu::KefuClient::new(&config.corpid, &config.secret);
        let url = client
            .get_contact_url(open_kfid)
            .await
            .map_err(|e| format!("get_contact_url failed: {e}"))?;

        // Persist the contact URL
        let mut updated = config;
        updated.contact_url = Some(url.clone());
        self.save_kefu_config(updated).await?;
        Ok(url)
    }

    pub async fn spawn_kefu_monitor(&self) -> Result<(), String> {
        // Stop existing monitor
        self.stop_kefu_monitor().await;

        let config = self
            .load_kefu_config()
            .await?
            .ok_or_else(|| "kefu config not found".to_string())?;
        let open_kfid = config
            .open_kfid
            .clone()
            .ok_or_else(|| "no open_kfid — create account first".to_string())?;

        let client = wechat_kefu::KefuClient::new(&config.corpid, &config.secret);
        // Batch-C §3: shutdown-cascaded child token (see monitor_cancel_token docs).
        let cancel = self.monitor_cancel_token().await;
        let (status_tx, status_rx) =
            tokio::sync::watch::channel(wechat_ilink::MonitorStatus::default());

        let (callback_tx, callback_rx) = tokio::sync::mpsc::channel(64);

        // Store callback sender for the HTTP callback handler
        *self.kefu_callback_tx.write().await = Some(callback_tx);

        let project_path = self.wechat_default_project_path.read().await.clone();
        let wiki_root = wiki_store::default_root();
        let _ = wiki_store::init_wiki(&wiki_root);
        let wiki_paths = wiki_store::WikiPaths::resolve(&wiki_root);

        let handler: Arc<dyn wechat_kefu::monitor::KefuMessageHandler> = Arc::new(
            wechat_kefu::desktop_handler::KefuDesktopHandler::new(self.clone(), project_path)
                .with_wiki_paths(wiki_paths),
        );

        let monitor_config = wechat_kefu::monitor::KefuMonitorConfig {
            client,
            open_kfid,
            handler,
            cancel: cancel.clone(),
            callback_rx,
        };

        eprintln!("[kefu] spawning monitor");
        tokio::spawn(async move {
            wechat_kefu::monitor::run_kefu_monitor(monitor_config, status_tx).await;
        });

        *self.kefu_monitor.write().await = Some(WeChatMonitorHandle { cancel, status_rx });
        Ok(())
    }

    pub async fn stop_kefu_monitor(&self) {
        if let Some(handle) = self.kefu_monitor.write().await.take() {
            handle.cancel.cancel();
            *self.kefu_callback_tx.write().await = None;
            eprintln!("[kefu] monitor stopped");
        }
    }

    /// R1.2 reliability gate · spawn the WeChat outbox replay worker.
    ///
    /// On first call:
    ///   1. Reverts every `Sending` row in `_wechat_outbox.json` back
    ///      to `Pending` (crash-recovery sweep — entries claimed but
    ///      not delivered before a previous crash become retryable).
    ///   2. Spawns a long-running tokio task whose tick interval is
    ///      30s. Each tick scans for `Pending` rows whose
    ///      `next_retry_at` is due (or unset), replays them through
    ///      the transport client, and marks the result.
    ///
    /// Cancellation cascades from `monitor_cancel_token()` so a
    /// process shutdown stops the worker cleanly.
    ///
    /// Idempotent — calling twice spawns one extra worker but the
    /// second worker is harmless (mutex serialization keeps the
    /// state machine consistent). Production calls this exactly
    /// once from `desktop-server::main`.
    pub async fn spawn_wechat_outbox_worker(&self) {
        let cancel = self.monitor_cancel_token().await;
        let wiki_root = wiki_store::default_root();
        let _ = wiki_store::init_wiki(&wiki_root);
        let paths = wiki_store::WikiPaths::resolve(&wiki_root);

        // 1. Crash-recovery sweep on bootstrap. Synchronous so we
        //    log the count before the worker tick begins.
        match wiki_store::wechat_outbox::reconcile_outbox_on_startup(&paths) {
            Ok(0) => {}
            Ok(n) => eprintln!(
                "[outbox worker] reconciled {n} stale Sending row(s) on startup"
            ),
            Err(e) => eprintln!("[outbox worker] reconcile failed: {e}"),
        }

        eprintln!("[outbox worker] spawning replay tick (30s)");
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = cancel.cancelled() => {
                        eprintln!("[outbox worker] shutdown signal received, stopping");
                        return;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                        let now = wiki_store::now_iso8601();
                        let due = match wiki_store::wechat_outbox::list_pending_outbox_due(&paths, &now) {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!("[outbox worker] list_pending_outbox_due failed: {e}");
                                continue;
                            }
                        };
                        if due.is_empty() {
                            continue;
                        }
                        eprintln!("[outbox worker] {} entr(ies) due for retry", due.len());
                        for entry in due {
                            replay_outbox_entry(&paths, &entry).await;
                        }
                    }
                }
            }
        });
    }

    pub async fn kefu_status(&self) -> wechat_kefu::KefuStatus {
        let config = wechat_kefu::load_config().ok().flatten();
        let monitor = self.kefu_monitor.read().await;
        let monitor_status = monitor
            .as_ref()
            .and_then(|h| h.status_rx.borrow().clone().into());

        wechat_kefu::KefuStatus {
            configured: config.is_some(),
            account_created: config.as_ref().and_then(|c| c.open_kfid.as_ref()).is_some(),
            monitor_running: monitor_status
                .as_ref()
                .map(|s: &wechat_ilink::MonitorStatus| s.running)
                .unwrap_or(false),
            last_poll_unix_ms: monitor_status.as_ref().and_then(|s| s.last_poll_unix_ms),
            last_inbound_unix_ms: monitor_status.as_ref().and_then(|s| s.last_inbound_unix_ms),
            consecutive_failures: monitor_status
                .as_ref()
                .map(|s| s.consecutive_failures)
                .unwrap_or(0),
            last_error: monitor_status.as_ref().and_then(|s| s.last_error.clone()),
            capabilities: wechat_kefu::KefuCapabilities::current(),
        }
    }

    /// Forward a callback event to the kefu monitor.
    pub async fn dispatch_kefu_callback(&self, event: wechat_kefu::CallbackEvent) -> bool {
        let tx = self.kefu_callback_tx.read().await;
        if let Some(sender) = tx.as_ref() {
            sender.send(event).await.is_ok()
        } else {
            false
        }
    }

    /// Start kefu monitor if config exists. Called at server startup.
    pub async fn auto_start_kefu_monitor(&self) {
        if let Ok(Some(config)) = wechat_kefu::load_config() {
            if config.open_kfid.is_some() {
                if let Err(e) = self.spawn_kefu_monitor().await {
                    eprintln!("[kefu] auto-start failed: {e}");
                }
            } else {
                eprintln!("[kefu] config exists but no open_kfid — skipping auto-start");
            }
        }
    }

    // --- Pipeline ---

    /// Start the one-scan kefu pipeline.
    pub async fn start_kefu_pipeline(
        &self,
        skip_cf: bool,
        cf_token: Option<String>,
        skip_callback: bool,
        corpid: Option<String>,
        secret: Option<String>,
    ) -> Result<(), String> {
        if let Some(cancel) = self.kefu_pipeline_cancel.write().await.take() {
            cancel.cancel();
            eprintln!("[kefu pipeline] cancelled previous run before restart");
        }

        // Batch-C §3: shutdown-cascaded child token (see monitor_cancel_token docs).
        let cancel = self.monitor_cancel_token().await;
        let (mut pipeline, state_rx) = wechat_kefu::pipeline::KefuPipeline::new(cancel.clone());

        if skip_cf {
            if let Some(token) = cf_token {
                pipeline.skip_cf_register(token);
            }
        }
        if skip_callback {
            if let (Some(c), Some(s)) = (corpid, secret) {
                pipeline.skip_callback_config(c, s);
            }
        }

        *self.kefu_pipeline_state.write().await = Some(state_rx);
        *self.kefu_pipeline_cancel.write().await = Some(cancel);

        let state = self.clone();
        tokio::spawn(async move {
            match pipeline.run().await {
                Ok(result) => {
                    eprintln!(
                        "[kefu pipeline] success! contact_url={}",
                        result.contact_url
                    );
                    // Auto-start monitor after pipeline completes
                    if let Err(e) = state.spawn_kefu_monitor().await {
                        eprintln!("[kefu pipeline] auto-start monitor failed: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("[kefu pipeline] failed: {e}");
                }
            }
        });

        Ok(())
    }

    /// Get current pipeline state.
    pub async fn kefu_pipeline_status(&self) -> Option<wechat_kefu::PipelineState> {
        let guard = self.kefu_pipeline_state.read().await;
        guard.as_ref().map(|rx| rx.borrow().clone())
    }

    /// Cancel running pipeline.
    pub async fn cancel_kefu_pipeline(&self) {
        if let Some(cancel) = self.kefu_pipeline_cancel.write().await.take() {
            cancel.cancel();
            eprintln!("[kefu pipeline] cancelled");
        }
    }

    /// Start a new QR login flow. Fetches the QR code synchronously
    /// from the iLink endpoint, stores a [`PendingLoginSlot`] in the
    /// in-memory map, and spawns a background task that waits for the
    /// user to confirm (up to 5 minutes). On success the background
    /// task persists the account and spawns a monitor for it.
    ///
    /// Returns `(handle, qr_image_content, expires_at_rfc3339)`. The
    /// handle is opaque and used by the frontend to poll status and
    /// cancel. `expires_at` is a hint only — the authoritative state
    /// is in the slot's `created_at + TTL`.
    pub async fn start_wechat_login(
        &self,
        base_url: Option<String>,
    ) -> Result<(String, String, String), String> {
        use std::time::{Duration, SystemTime, UNIX_EPOCH};
        use wechat_ilink::login::{LoginStatus as IlinkLoginStatus, QrLoginSession};

        // Fetch the QR code first so we can fail fast on network error
        // instead of returning a handle that immediately dies.
        let mut session = QrLoginSession::new(base_url)
            .map_err(|e| format!("QrLoginSession::new failed: {e}"))?;
        let qr = session
            .fetch_qr_code()
            .await
            .map_err(|e| format!("fetch_qr_code failed: {e}"))?;

        // The iLink server returns a plain https URL in `qrcode_img_content`
        // (an HTML page, not a raw image). The frontend expects a
        // `data:image/…` data URI, so generate the QR code image locally
        // from the URL that the user needs to scan.
        let qr_data_uri = {
            use qrcode::QrCode;
            let code = QrCode::new(qr.qrcode_img_content.as_bytes())
                .map_err(|e| format!("QR code generation failed: {e}"))?;
            let image = code.render::<qrcode::render::svg::Color>().build();
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(image.as_bytes());
            format!("data:image/svg+xml;base64,{b64}")
        };

        let handle = generate_login_handle();
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let slot = Arc::new(Mutex::new(wechat_ilink::PendingLoginSlot {
            handle: handle.clone(),
            created_at: std::time::Instant::now(),
            qr_image_content: qr_data_uri.clone(),
            state: wechat_ilink::PendingLoginState::Waiting,
            cancel_tx: Some(cancel_tx),
        }));

        self.pending_wechat_logins
            .write()
            .await
            .insert(handle.clone(), slot.clone());

        // Spawn the background task that drives the login. It races
        // against the cancel channel and the 5-minute TTL. On any
        // outcome it updates `slot.state` so the next status poll
        // returns the final state.
        let state = self.clone();
        let handle_for_task = handle.clone();
        tokio::spawn(async move {
            let status_slot = slot.clone();
            let wait_fut = session.wait_for_login(
                Duration::from_secs(wechat_ilink::PENDING_LOGIN_TTL_SECS),
                move |status: IlinkLoginStatus| {
                    // We use try_lock because this callback runs in
                    // the same task as wait_for_login's internal
                    // polling, which already holds the slot? No — it
                    // doesn't. But being defensive here is cheap.
                    let slot = status_slot.clone();
                    tokio::spawn(async move {
                        let mut guard = slot.lock().await;
                        guard.state = match status {
                            IlinkLoginStatus::Wait => wechat_ilink::PendingLoginState::Waiting,
                            IlinkLoginStatus::Scanned => wechat_ilink::PendingLoginState::Scanned,
                            IlinkLoginStatus::Expired => wechat_ilink::PendingLoginState::Expired,
                            IlinkLoginStatus::Confirmed => {
                                // Placeholder — the real Confirmed
                                // with account_id is written after
                                // persist below.
                                wechat_ilink::PendingLoginState::Scanned
                            }
                        };
                    });
                },
            );

            tokio::select! {
                result = wait_fut => {
                    match result {
                        Ok(confirmation) => {
                            // Persist the new account and spawn its
                            // monitor, then mark the slot Confirmed.
                            let normalized =
                                wechat_ilink::account::normalize_account_id(
                                    &confirmation.ilink_bot_id,
                                );
                            let save_result = wechat_ilink::account::save_account(
                                &normalized,
                                wechat_ilink::types::WeixinAccountData {
                                    token: Some(confirmation.bot_token.clone()),
                                    base_url: Some(confirmation.base_url.clone()),
                                    user_id: confirmation.user_id.clone(),
                                    ..Default::default()
                                },
                            );
                            let mut guard = slot.lock().await;
                            match save_result {
                                Ok(_) => {
                                    let spawn_res = state
                                        .spawn_wechat_monitor(&normalized)
                                        .await;
                                    if let Err(e) = spawn_res {
                                        guard.state = wechat_ilink::PendingLoginState::Failed {
                                            error: format!(
                                                "persisted but monitor spawn failed: {e}"
                                            ),
                                        };
                                    } else {
                                        guard.state = wechat_ilink::PendingLoginState::Confirmed {
                                            account_id: normalized,
                                        };
                                    }
                                }
                                Err(e) => {
                                    guard.state = wechat_ilink::PendingLoginState::Failed {
                                        error: format!("save_account failed: {e}"),
                                    };
                                }
                            }
                        }
                        Err(e) => {
                            let mut guard = slot.lock().await;
                            guard.state = wechat_ilink::PendingLoginState::Failed {
                                error: format!("login flow failed: {e}"),
                            };
                        }
                    }
                }
                _ = cancel_rx => {
                    let mut guard = slot.lock().await;
                    if !guard.state.is_terminal() {
                        guard.state = wechat_ilink::PendingLoginState::Cancelled;
                    }
                }
            }

            eprintln!("[wechat] login task {handle_for_task} finished");
        });

        let expires_at = SystemTime::now()
            .checked_add(Duration::from_secs(wechat_ilink::PENDING_LOGIN_TTL_SECS))
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| format!("{}s", d.as_secs()))
            .unwrap_or_default();

        Ok((handle, qr_data_uri, expires_at))
    }

    /// Poll the current status of a QR login flow.
    ///
    /// On each call we also garbage-collect slots that have exceeded
    /// the TTL so the map doesn't grow without bound.
    pub async fn wechat_login_status(&self, handle: &str) -> Option<WeChatLoginStatusSnapshot> {
        let slot = {
            let map = self.pending_wechat_logins.read().await;
            map.get(handle).cloned()
        }?;
        let guard = slot.lock().await;
        // Translate Expired state if we're past TTL and still Waiting.
        let state_tag;
        let account_id;
        let error;
        match &guard.state {
            wechat_ilink::PendingLoginState::Waiting if guard.is_past_ttl() => {
                state_tag = "expired";
                account_id = None;
                error = None;
            }
            wechat_ilink::PendingLoginState::Waiting => {
                state_tag = "waiting";
                account_id = None;
                error = None;
            }
            wechat_ilink::PendingLoginState::Scanned => {
                state_tag = "scanned";
                account_id = None;
                error = None;
            }
            wechat_ilink::PendingLoginState::Confirmed { account_id: id } => {
                state_tag = "confirmed";
                account_id = Some(id.clone());
                error = None;
            }
            wechat_ilink::PendingLoginState::Failed { error: e } => {
                state_tag = "failed";
                account_id = None;
                error = Some(e.clone());
            }
            wechat_ilink::PendingLoginState::Cancelled => {
                state_tag = "cancelled";
                account_id = None;
                error = None;
            }
            wechat_ilink::PendingLoginState::Expired => {
                state_tag = "expired";
                account_id = None;
                error = None;
            }
        };
        Some(WeChatLoginStatusSnapshot {
            status: state_tag.to_string(),
            account_id,
            error,
        })
    }

    /// Fire the cancel signal for a login flow. Best-effort: if the
    /// background task is already past the point where cancel matters,
    /// the next poll will still return Confirmed/Failed.
    pub async fn cancel_wechat_login(&self, handle: &str) -> bool {
        let map = self.pending_wechat_logins.read().await;
        let Some(slot) = map.get(handle).cloned() else {
            return false;
        };
        drop(map);
        let mut guard = slot.lock().await;
        if let Some(tx) = guard.cancel_tx.take() {
            let _ = tx.send(());
        }
        true
    }
}

/// Frontend-facing rollup of a persisted WeChat account's state.
#[derive(Debug, Clone)]
pub struct WeChatAccountInfo {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    /// First 6 / last 4 chars of the bot token plus length, never full.
    pub bot_token_preview: String,
    pub saved_at: Option<String>,
    pub status: WeChatConnectionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeChatConnectionStatus {
    Connected,
    Disconnected,
    SessionExpired,
}

impl WeChatConnectionStatus {
    #[must_use]
    pub fn wire_tag(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::SessionExpired => "session_expired",
        }
    }
}

/// Simple snapshot of the login status used by the status poll handler.
#[derive(Debug, Clone)]
pub struct WeChatLoginStatusSnapshot {
    pub status: String,
    pub account_id: Option<String>,
    pub error: Option<String>,
}

/// Format a bot token for display: `"{first6}...{last4} ({len} chars)"`.
/// Used by the list endpoint so the UI never sees the full secret.
fn format_bot_token_preview(token: &str) -> String {
    let len = token.chars().count();
    if len <= 10 {
        return format!("*** ({len} chars)");
    }
    let first: String = token.chars().take(6).collect();
    let last: String = token.chars().skip(len.saturating_sub(4)).collect();
    format!("{first}...{last} ({len} chars)")
}

/// Generate a URL-safe random 16-byte hex handle for a pending login.
/// Uses `rand::random` for per-call entropy; collision probability is
/// negligible given the map lifetime.
fn generate_login_handle() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // No rand crate in desktop-core (kept lean) — derive entropy from
    // system nanoseconds + thread id. Not cryptographic; only needs to
    // be unguessable and unique within a session.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tid = format!("{:?}", std::thread::current().id());
    format!("{:x}{:x}", now, tid.len() ^ (now as usize))
}

fn execute_live_turn(session: RuntimeSession, request: DesktopTurnRequest) -> DesktopTurnResult {
    let cwd = PathBuf::from(&request.project_path);
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = match loader.load() {
        Ok(config) => config,
        Err(error) => {
            return fallback_turn_result(
                session,
                &request.message,
                DEFAULT_MODEL_LABEL.to_string(),
                format!("failed to load runtime config: {error}"),
            )
        }
    };

    // S0.4 cut day: Phase 3-5 multi-provider override is gone. The
    // active model now comes solely from the runtime_config (which the
    // S2 codex_broker will populate from the managed Codex pool). The
    // canonical resolver below collapses to the legacy env-var credential
    // chain until S2 lands.
    let resolved_model = resolve_model_alias(runtime_config.model().unwrap_or(DEFAULT_MODEL_ID));
    let model_label = humanize_model_label(&resolved_model);
    let mut system_prompt = match load_system_prompt(
        &cwd,
        current_date_string(),
        env::consts::OS,
        detect_os_version(),
    ) {
        Ok(prompt) => prompt,
        Err(error) => {
            return fallback_turn_result(
                session,
                &request.message,
                model_label.clone(),
                format!("failed to build the system prompt: {error}"),
            )
        }
    };

    // Inject URL-fetched content into system prompt (ephemeral, not stored in session).
    // The content travels inside the per-turn request struct instead of a global
    // env var, so concurrent requests never overwrite each other.
    if let Some(ref url_context) = request.url_context {
        if !url_context.is_empty() {
            eprintln!(
                "[live_turn] injecting url_context: {} chars into system_prompt (fallback)",
                url_context.len()
            );
            system_prompt.push(url_context.clone());
        }
    }
    eprintln!(
        "[live_turn] system_prompt final: {} blocks, {} total chars",
        system_prompt.len(),
        system_prompt.iter().map(|s| s.len()).sum::<usize>()
    );

    // feat(W9): inject wiki/index.md as context into the system prompt
    // so the LLM can reference the user's external brain when answering
    // questions. This is Karpathy llm-wiki.md §"Query": "the LLM reads
    // the index first to find relevant pages, then drills into them".
    //
    // Injection is best-effort: if the wiki root doesn't exist or
    // index.md hasn't been built yet, we silently skip. The system
    // prompt already works without the wiki section — this just makes
    // it richer when the user has accumulated knowledge.
    //
    // `system_prompt` is a `Vec<String>` — each entry becomes a
    // separate system-prompt block. We push one block with the
    // wiki index so it doesn't interleave with existing entries.
    {
        let wiki_root = wiki_store::default_root();
        let index_path = wiki_root.join("wiki").join("index.md");
        if let Ok(index_content) = fs::read_to_string(&index_path) {
            let trimmed = index_content.trim();
            if !trimmed.is_empty() {
                let mut section = String::new();
                section.push_str("## Your External Brain (ClawWiki)\n\n");
                section.push_str(
                    "The user has a personal wiki — their \"external brain\" — at ~/.clawwiki/. \
                     Below is the current index of their knowledge base. When answering questions, \
                     consider whether the user's wiki contains relevant concept pages. Reference \
                     them by slug when appropriate.\n\n",
                );
                section.push_str("<wiki-index>\n");
                section.push_str(trimmed);
                section.push_str("\n</wiki-index>");
                system_prompt.push(section);
            }
        }
    }

    let (feature_config, tool_registry) =
        match build_runtime_plugin_state(&cwd, &loader, &runtime_config) {
            Ok(result) => result,
            Err(error) => {
                return fallback_turn_result(
                    session,
                    &request.message,
                    model_label.clone(),
                    format!("failed to initialize tools and plugins: {error}"),
                )
            }
        };

    // ── Credential resolution chain (priority order) ───────────
    //
    // 1. providers.json active provider (highest — user explicitly
    //    configured this in Settings > LLM Gateway)
    // 2. optional private-cloud broker pool
    // 3. default_auth_source (env vars, managed auth, etc.)
    //
    // providers.json is checked FIRST because when a user configures
    // Kimi/DeepSeek/Qwen in the LLM Gateway UI, that choice must
    // override the empty codex_broker pool. Without this ordering,
    // the broker's "pool empty" error swallows the providers.json
    // path and the user-configured provider never gets used.

    // Step 1: scan .claw/providers.json for an active provider.
    // Handles both "anthropic" (native Anthropic Messages API) and
    // "openai_compat" (OpenAI ChatCompletions — Kimi, DeepSeek, etc.).
    // Returns (ProviderClient, model_name) so we can override
    // resolved_model with the provider's configured model.
    let providers_override: Option<(ProviderClient, String)> = {
        let mut result: Option<(ProviderClient, String)> = None;
        let mut roots = vec![cwd.clone()];
        if let Ok(process_cwd) = std::env::current_dir() {
            if process_cwd != cwd {
                roots.push(process_cwd);
            }
        }
        'providers: for root in &roots {
            let providers_path = root.join(".claw").join("providers.json");
            let Ok(raw) = fs::read_to_string(&providers_path) else {
                eprintln!(
                    "[runtime:providers] {}: not found, skipping",
                    providers_path.display()
                );
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
                eprintln!(
                    "[runtime:providers] {}: parse error, skipping",
                    providers_path.display()
                );
                continue;
            };
            let Some(active_id) = parsed
                .get("active")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                eprintln!(
                    "[runtime:providers] {}: no active provider set",
                    providers_path.display()
                );
                continue;
            };
            let Some(entry) = parsed.get("providers").and_then(|p| p.get(active_id)) else {
                eprintln!("[runtime:providers] active={active_id:?} not found in providers map");
                continue;
            };
            let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
            if api_key.trim().is_empty() {
                eprintln!("[runtime:providers] active={active_id:?} has empty api_key, skipping");
                continue;
            }
            let base_url = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
            let model = entry.get("model").and_then(|v| v.as_str()).unwrap_or("");

            match kind {
                "openai_compat" => {
                    if base_url.is_empty() {
                        eprintln!("[runtime:providers] active={active_id:?} openai_compat has empty base_url, skipping");
                        continue;
                    }
                    eprintln!(
                        "[runtime:providers] using OpenAiCompat provider {active_id:?} \
                         base_url={base_url:?} model={model:?}"
                    );
                    let oai_client =
                        OpenAiCompatClient::new(api_key.to_string(), OpenAiCompatConfig::openai())
                            .with_base_url(base_url.to_string());
                    let effective_model = if model.is_empty() {
                        "moonshot-v1-auto".to_string()
                    } else {
                        model.to_string()
                    };
                    result = Some((ProviderClient::OpenAi(oai_client), effective_model));
                    break 'providers;
                }
                "anthropic" => {
                    let effective_base = if base_url.is_empty() {
                        "https://api.anthropic.com"
                    } else {
                        base_url
                    };
                    eprintln!(
                        "[runtime:providers] using Anthropic provider {active_id:?} \
                         base_url={effective_base:?} model={model:?}"
                    );
                    let mut client =
                        AnthropicClient::from_auth(AuthSource::ApiKey(api_key.to_string()));
                    client = client.with_base_url(effective_base.to_string());
                    let effective_model = if model.is_empty() {
                        "claude-sonnet-4-5".to_string()
                    } else {
                        model.to_string()
                    };
                    result = Some((ProviderClient::Anthropic(client), effective_model));
                    break 'providers;
                }
                _ => {
                    eprintln!(
                        "[runtime:providers] active={active_id:?} unknown kind={kind:?}, skipping"
                    );
                    continue;
                }
            }
        }
        result
    };

    // Step 2: if providers.json gave us a client, use it directly.
    // Otherwise fall through to the optional private-cloud broker and
    // finally to the default auth chain.
    let (default_auth, client_override, resolved_model) =
        if let Some((provider_client, provider_model)) = providers_override {
            eprintln!(
                "[runtime] using providers.json override for this turn (model={provider_model:?})"
            );
            (None, Some(provider_client), provider_model)
        } else {
            #[cfg(feature = "private-cloud")]
            {
                match codex_broker::global() {
                    Some(broker) => match broker.build_provider_client() {
                        Ok(client) => {
                            eprintln!(
                            "[runtime] using private-cloud broker for turn (model={resolved_model})"
                        );
                            (None, Some(client), resolved_model)
                        }
                        Err(err) => {
                            eprintln!(
                                "[runtime] private-cloud broker has no usable account ({err}); \
                             falling back to env-var credential chain"
                            );
                            match default_auth_source(&resolved_model, &runtime_config) {
                                Ok(auth) => (auth, None, resolved_model),
                                Err(error) => {
                                    return fallback_turn_result(
                                        session,
                                        &request.message,
                                        model_label.clone(),
                                        format!("failed to resolve model authentication: {error}"),
                                    )
                                }
                            }
                        }
                    },
                    None => match default_auth_source(&resolved_model, &runtime_config) {
                        Ok(auth) => (auth, None, resolved_model),
                        Err(error) => {
                            return fallback_turn_result(
                                session,
                                &request.message,
                                model_label.clone(),
                                format!("failed to resolve model authentication: {error}"),
                            )
                        }
                    },
                }
            }
            #[cfg(not(feature = "private-cloud"))]
            {
                match default_auth_source(&resolved_model, &runtime_config) {
                    Ok(auth) => (auth, None, resolved_model),
                    Err(error) => {
                        return fallback_turn_result(
                            session,
                            &request.message,
                            model_label.clone(),
                            format!("failed to resolve model authentication: {error}"),
                        )
                    }
                }
            }
        };

    // When using an OpenAI-compat provider (Kimi, DeepSeek, etc.),
    // strip tools from the request. These providers have varying
    // degrees of function-calling support, and sending the full
    // Anthropic tool schema causes 400 errors ("invalid scalar
    // type"). For MVP the Ask page works as a pure chat surface
    // without tool use when on OpenAI-compat providers.
    let is_openai_compat_override = matches!(&client_override, Some(ProviderClient::OpenAi(_)));
    if is_openai_compat_override {
        eprintln!("[runtime] OpenAi provider: stripping tools for compatibility");
    }

    let api_client = match DesktopRuntimeClient::new(
        resolved_model.to_string(),
        default_auth,
        tool_registry.clone(),
        client_override,
    ) {
        Ok(client) => client,
        Err(error) => {
            return fallback_turn_result(
                session,
                &request.message,
                model_label.clone(),
                format!("failed to create the runtime client: {error}"),
            )
        }
    };

    let mut runtime = ConversationRuntime::new_with_features(
        session,
        api_client,
        DesktopToolExecutor::new(tool_registry.clone()),
        permission_policy(
            runtime_config
                .permission_mode()
                .map(permission_mode_from_config)
                .unwrap_or(PermissionMode::DangerFullAccess),
            &tool_registry,
        ),
        system_prompt,
        &feature_config,
    );

    match with_workspace_cwd(&cwd, || runtime.run_turn(request.message.clone(), None)) {
        Ok(_) => DesktopTurnResult {
            session: runtime.into_session(),
            model_label,
        },
        Err(error) => {
            let mut failed_session = runtime.into_session();
            failed_session.messages.push(assistant_text(format!(
                "Desktop runtime couldn't execute this turn.\n\n{error}"
            )));
            DesktopTurnResult {
                session: failed_session,
                model_label,
            }
        }
    }
}

fn build_runtime_plugin_state(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &RuntimeConfig,
) -> Result<(RuntimeFeatureConfig, GlobalToolRegistry), String> {
    let plugin_manager = build_plugin_manager(cwd, loader, runtime_config);
    let tool_registry = GlobalToolRegistry::with_plugin_tools(
        plugin_manager
            .aggregated_tools()
            .map_err(|error| error.to_string())?,
    )?;
    Ok((runtime_config.feature_config().clone(), tool_registry))
}

fn build_plugin_manager(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &RuntimeConfig,
) -> PluginManager {
    let plugin_settings = runtime_config.plugins();
    let mut plugin_config = PluginManagerConfig::new(loader.config_home().to_path_buf());
    plugin_config.enabled_plugins = plugin_settings.enabled_plugins().clone();
    plugin_config.external_dirs = plugin_settings
        .external_directories()
        .iter()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path))
        .collect();
    plugin_config.install_root = plugin_settings
        .install_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.registry_path = plugin_settings
        .registry_path()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.bundled_root = plugin_settings
        .bundled_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    PluginManager::new(plugin_config)
}

fn resolve_plugin_path(cwd: &Path, config_home: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if value.starts_with('.') {
        cwd.join(path)
    } else {
        config_home.join(path)
    }
}

fn default_auth_source(
    model: &str,
    runtime_config: &RuntimeConfig,
) -> Result<Option<AuthSource>, String> {
    if detect_provider_kind(model) != ProviderKind::Anthropic {
        return Ok(None);
    }

    resolve_startup_auth_source(|| Ok(runtime_config.oauth().cloned()))
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Validate a `project_path` query parameter from an HTTP request.
///
/// Defends against path traversal abuse (S-02) by enforcing:
///   1. Path is non-empty
///   2. Path does NOT contain `..` segments (which could escape any
///      sandbox we add later)
///   3. Path is canonicalizable to a directory that exists
///
/// Returns the canonical absolute path on success.
///
/// **Note**: this is intentionally permissive — we don't restrict to
/// the user's home directory because legitimate use cases include
/// system-wide projects (e.g. `/opt/myapp`). The main goal is to
/// prevent obvious traversal patterns and to fail fast on typos.
pub fn validate_project_path(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("project_path is empty".to_string());
    }

    // Reject path traversal segments. Note: this is a string-level
    // check and not a security boundary on its own — but combined
    // with canonicalize() below, it makes it harder to construct
    // surprising paths via concatenation in callers.
    let path = Path::new(trimmed);
    for component in path.components() {
        use std::path::Component;
        if matches!(component, Component::ParentDir) {
            return Err(format!(
                "project_path contains '..' segment which is not allowed: {trimmed}"
            ));
        }
    }

    // Canonicalize to an absolute path. This both validates that the
    // directory exists AND collapses any symlink shenanigans.
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("project_path does not exist or is unreadable: {trimmed} ({e})"))?;

    if !canonical.is_dir() {
        return Err(format!(
            "project_path is not a directory: {}",
            canonical.display()
        ));
    }

    Ok(canonical)
}

/// Resolve credentials for the agentic loop in priority order:
///
/// 1. `ANTHROPIC_API_KEY` env var → direct mode (no managed_auth)
/// 2. Project's `.claude/settings.json` → `direct_api_key` field
/// 3. managed_auth provider `codex-openai`
/// 4. managed_auth provider `qwen-code`
///
/// "Direct mode" returns a synthetic `DesktopManagedAuthRuntimeClient`
/// pointing at the Anthropic API directly. The `code_tools_bridge`
/// then forwards to api.anthropic.com with the user's key.
///
/// This lets users get up and running without going through OAuth
/// flow setup. Storing the API key in plaintext settings.json is
/// less secure than the managed_auth flow but matches what most
/// CLI tools do.
async fn resolve_runtime_credentials(
    state: &DesktopState,
    project_path: &Path,
) -> Result<DesktopManagedAuthRuntimeClient, DesktopStateError> {
    // 1. Env var has highest priority — easiest setup, no files to edit.
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.trim().is_empty() {
            return Ok(direct_anthropic_client(key));
        }
    }

    // 2. Project-local direct_api_key from .claude/settings.json.
    let settings_json = project_path.join(".claude").join("settings.json");
    if settings_json.is_file() {
        if let Ok(text) = fs::read_to_string(&settings_json) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(key) = value
                    .get("direct_api_key")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                {
                    return Ok(direct_anthropic_client(key.to_string()));
                }
            }
        }
    }

    // 3. Managed auth: codex-openai → qwen-code fallback chain.
    if let Ok(client) = state.managed_auth_runtime_client("codex-openai").await {
        return Ok(client);
    }
    if let Ok(client) = state.managed_auth_runtime_client("qwen-code").await {
        return Ok(client);
    }

    // 4. .claw/providers.json — user-configured providers from the
    //    settings UI. For Anthropic-compatible gateways the agentic
    //    loop can call the gateway directly since it already speaks the
    //    native Anthropic Messages API.
    //
    //    The providers_config module was removed in S0.4 (codex_broker
    //    owns this surface now), but we still support the on-disk JSON
    //    file as a lightweight local-dev override. Parse it inline.
    //
    //    Try project_path first, then fall back to cwd. The settings UI
    //    saves providers.json relative to cwd (desktop-server's working
    //    directory), but session metadata may carry a different
    //    project_path (e.g. the hardcoded DEFAULT_PROJECT_PATH).
    let search_roots: Vec<PathBuf> = {
        let mut roots = vec![project_path.to_path_buf()];
        if let Ok(cwd) = std::env::current_dir() {
            if cwd != project_path {
                roots.push(cwd);
            }
        }
        roots
    };
    for root in &search_roots {
        let providers_path = root.join(".claw").join("providers.json");
        if !providers_path.is_file() {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&providers_path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let active_id = parsed.get("active").and_then(|v| v.as_str()).unwrap_or("");
        if active_id.is_empty() {
            continue;
        }
        let Some(entry) = parsed.get("providers").and_then(|p| p.get(active_id)) else {
            continue;
        };
        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
        if api_key.trim().is_empty() {
            continue;
        }
        // Support both "anthropic" (native Anthropic Messages API) and
        // "openai_compat" (OpenAI ChatCompletions API — DeepSeek, Kimi,
        // Qwen, GLM, xAI, etc.). Each routes to the appropriate client
        // in the agentic loop via the provider_kind field.
        let (provider_kind, default_base, default_model_str) = match kind {
            "anthropic" => (
                DesktopManagedAuthProviderKind::AnthropicCompat,
                "https://api.anthropic.com",
                "claude-sonnet-4-5",
            ),
            "openai_compat" => (
                DesktopManagedAuthProviderKind::OpenAiCompat,
                "",
                "moonshot-v1-auto",
            ),
            _ => continue,
        };
        let base_url = entry
            .get("base_url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(default_base)
            .to_string();
        if base_url.is_empty() {
            eprintln!("[providers] skipping {active_id:?}: openai_compat requires base_url");
            continue;
        }
        let model = entry
            .get("model")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(default_model_str)
            .to_string();
        eprintln!(
            "[providers] resolved from {}/.claw/providers.json: \
             active={active_id:?} kind={kind:?} base_url={base_url:?} model={model:?}",
            root.display(),
        );
        return Ok(DesktopManagedAuthRuntimeClient {
            provider_id: format!("providers-json:{active_id}"),
            provider_kind,
            base_url,
            bearer_token: api_key.to_string(),
            extra_headers: HashMap::new(),
            default_model: Some(model),
        });
    }

    Err(DesktopStateError::ProviderNotFound(
        "no credentials available — set ANTHROPIC_API_KEY env var, add \
         direct_api_key to .claude/settings.json, configure a provider in \
         Settings, or run codex/qwen login"
            .into(),
    ))
}

/// Build a synthetic `DesktopManagedAuthRuntimeClient` that points the
/// agentic loop at the Anthropic API directly.
fn direct_anthropic_client(api_key: String) -> DesktopManagedAuthRuntimeClient {
    DesktopManagedAuthRuntimeClient {
        provider_id: "direct-anthropic".to_string(),
        provider_kind: DesktopManagedAuthProviderKind::AnthropicCompat,
        base_url: "https://api.anthropic.com".to_string(),
        bearer_token: api_key,
        extra_headers: HashMap::new(),
        default_model: None,
    }
}

/// Construct a `ProviderClient` directly from a [`providers_config::ProviderEntry`].
fn build_provider_client_from_entry(
    entry: &providers_config::ProviderEntry,
) -> Result<ProviderClient, String> {
    use providers_config::ProviderKind as CfgKind;
    match entry.kind {
        CfgKind::Anthropic => {
            if entry.api_key.trim().is_empty() {
                return Err("anthropic provider has empty api_key".to_string());
            }
            let mut client = AnthropicClient::from_auth(AuthSource::ApiKey(entry.api_key.clone()));
            let configured_base = entry.effective_base_url();
            if !configured_base.is_empty() {
                client = client.with_base_url(configured_base);
            }
            Ok(ProviderClient::Anthropic(client))
        }
        CfgKind::OpenAiCompat => {
            let base_url = entry.effective_base_url();
            if base_url.is_empty() {
                return Err("openai_compat provider has empty base_url".to_string());
            }
            if entry.api_key.trim().is_empty() {
                return Err("openai_compat provider has empty api_key".to_string());
            }
            let client =
                OpenAiCompatClient::new(entry.api_key.clone(), OpenAiCompatConfig::openai())
                    .with_base_url(base_url);
            Ok(ProviderClient::OpenAi(client))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderProbeResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
    pub model_echo: Option<String>,
}

pub async fn probe_provider_entry(entry: &providers_config::ProviderEntry) -> ProviderProbeResult {
    let started = std::time::Instant::now();
    let client = match build_provider_client_from_entry(entry) {
        Ok(c) => c,
        Err(err) => {
            return ProviderProbeResult {
                ok: false,
                latency_ms: started.elapsed().as_millis() as u64,
                error: Some(err),
                model_echo: None,
            };
        }
    };
    let request = MessageRequest {
        model: entry.model.clone(),
        max_tokens: 8,
        messages: vec![InputMessage::user_text("ping")],
        system: None,
        tools: None,
        tool_choice: None,
        stream: false,
    };
    let send_fut = client.send_message(&request);
    match tokio::time::timeout(std::time::Duration::from_secs(60), send_fut).await {
        Ok(Ok(response)) => ProviderProbeResult {
            ok: true,
            latency_ms: started.elapsed().as_millis() as u64,
            error: None,
            model_echo: Some(response.model),
        },
        Ok(Err(err)) => ProviderProbeResult {
            ok: false,
            latency_ms: started.elapsed().as_millis() as u64,
            error: Some(err.to_string()),
            model_echo: None,
        },
        Err(_) => ProviderProbeResult {
            ok: false,
            latency_ms: started.elapsed().as_millis() as u64,
            error: Some("request timed out after 60s".to_string()),
            model_echo: None,
        },
    }
}

/// Resolve the default project path that newly-created WeChat sessions
/// should be associated with. Precedence:
///   1. `WECHAT_DEFAULT_PROJECT_PATH` env var
///   2. Current process working directory
///   3. "." (relative cwd)
///
/// Exposed from `desktop-core` so both the HTTP server and the standalone
/// `wechat-login` CLI subcommand resolve the same default.
#[must_use]
pub fn resolve_wechat_default_project_path() -> String {
    if let Ok(path) = std::env::var("WECHAT_DEFAULT_PROJECT_PATH") {
        if !path.trim().is_empty() {
            return path;
        }
    }
    std::env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".to_string())
}

fn permission_policy(mode: PermissionMode, tool_registry: &GlobalToolRegistry) -> PermissionPolicy {
    match tool_registry.permission_specs(None) {
        Ok(specs) => specs.into_iter().fold(
            PermissionPolicy::new(mode),
            |policy, (name, required_permission)| {
                policy.with_tool_requirement(name, required_permission)
            },
        ),
        Err(error) => {
            eprintln!("desktop permission policy fallback: {error}");
            PermissionPolicy::new(mode)
        }
    }
}

fn permission_mode_from_config(value: ResolvedPermissionMode) -> PermissionMode {
    match value {
        ResolvedPermissionMode::ReadOnly => PermissionMode::ReadOnly,
        ResolvedPermissionMode::WorkspaceWrite => PermissionMode::WorkspaceWrite,
        ResolvedPermissionMode::DangerFullAccess => PermissionMode::DangerFullAccess,
    }
}

struct DesktopRuntimeClient {
    runtime: tokio::runtime::Runtime,
    client: ProviderClient,
    model: String,
    tool_registry: GlobalToolRegistry,
}

impl DesktopRuntimeClient {
    /// Build a runtime client, optionally injecting an explicit
    /// [`ProviderClient`] instead of letting the api crate autodetect
    /// the provider from env vars + model name.
    ///
    /// Phase 3 uses this to drive the client from
    /// `.claw/providers.json` (multi-provider registry). When
    /// `client_override` is `None` the legacy env-var path still
    /// applies, preserving backward compat with pre-Phase-3 setups.
    fn new(
        model: String,
        default_auth: Option<AuthSource>,
        tool_registry: GlobalToolRegistry,
        client_override: Option<ProviderClient>,
    ) -> Result<Self, String> {
        let client = match client_override {
            Some(client) => client,
            None => ProviderClient::from_model_with_anthropic_auth(&model, default_auth)
                .map_err(|error| error.to_string())?,
        };
        Ok(Self {
            runtime: tokio::runtime::Runtime::new().map_err(|error| error.to_string())?,
            client,
            model,
            tool_registry,
        })
    }
}

impl RuntimeApiClient for DesktopRuntimeClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let tools = self.tool_registry.definitions(None);
        let has_tools = !tools.is_empty();
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: has_tools.then_some(tools),
            tool_choice: has_tools.then_some(ToolChoice::Auto),
            stream: true,
        };

        self.runtime.block_on(async {
            let mut stream = self
                .client
                .stream_message(&message_request)
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?;
            let mut events = Vec::new();
            let mut pending_tools: BTreeMap<u32, (String, String, String)> = BTreeMap::new();
            let mut saw_stop = false;

            while let Some(event) = stream
                .next_event()
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?
            {
                match event {
                    ApiStreamEvent::MessageStart(start) => {
                        for block in start.message.content {
                            push_output_block(block, 0, &mut events, &mut pending_tools, true);
                        }
                    }
                    ApiStreamEvent::ContentBlockStart(start) => {
                        push_output_block(
                            start.content_block,
                            start.index,
                            &mut events,
                            &mut pending_tools,
                            true,
                        );
                    }
                    ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                        ContentBlockDelta::TextDelta { text } => {
                            if !text.is_empty() {
                                events.push(AssistantEvent::TextDelta(text));
                            }
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some((_, _, input)) = pending_tools.get_mut(&delta.index) {
                                input.push_str(&partial_json);
                            }
                        }
                        ContentBlockDelta::ThinkingDelta { .. }
                        | ContentBlockDelta::SignatureDelta { .. } => {}
                    },
                    ApiStreamEvent::ContentBlockStop(stop) => {
                        if let Some((id, name, input)) = pending_tools.remove(&stop.index) {
                            events.push(AssistantEvent::ToolUse { id, name, input });
                        }
                    }
                    ApiStreamEvent::MessageDelta(delta) => {
                        events.push(AssistantEvent::Usage(TokenUsage {
                            input_tokens: delta.usage.input_tokens,
                            output_tokens: delta.usage.output_tokens,
                            cache_creation_input_tokens: 0,
                            cache_read_input_tokens: 0,
                        }));
                    }
                    ApiStreamEvent::MessageStop(_) => {
                        saw_stop = true;
                        events.push(AssistantEvent::MessageStop);
                    }
                }
            }

            if !saw_stop
                && events.iter().any(|event| {
                    matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                        || matches!(event, AssistantEvent::ToolUse { .. })
                })
            {
                events.push(AssistantEvent::MessageStop);
            }

            if events
                .iter()
                .any(|event| matches!(event, AssistantEvent::MessageStop))
            {
                return Ok(events);
            }

            let response = self
                .client
                .send_message(&MessageRequest {
                    stream: false,
                    ..message_request.clone()
                })
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?;
            Ok(response_to_events(response))
        })
    }
}

struct DesktopToolExecutor {
    tool_registry: GlobalToolRegistry,
}

impl DesktopToolExecutor {
    fn new(tool_registry: GlobalToolRegistry) -> Self {
        Self { tool_registry }
    }
}

impl RuntimeToolExecutor for DesktopToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        self.tool_registry
            .execute(tool_name, &value)
            .map_err(ToolError::new)
    }
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    messages
        .iter()
        .filter_map(|message| {
            let role = match message.role {
                MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            let content = message
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => InputContentBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => InputContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ToolResultContentBlock::Text {
                            text: output.clone(),
                        }],
                        is_error: *is_error,
                    },
                })
                .collect::<Vec<_>>();
            (!content.is_empty()).then(|| InputMessage {
                role: role.to_string(),
                content,
            })
        })
        .collect()
}

fn push_output_block(
    block: OutputContentBlock,
    block_index: u32,
    events: &mut Vec<AssistantEvent>,
    pending_tools: &mut BTreeMap<u32, (String, String, String)>,
    streaming_tool_input: bool,
) {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            pending_tools.insert(block_index, (id, name, initial_input));
        }
        OutputContentBlock::Thinking { .. } | OutputContentBlock::RedactedThinking { .. } => {}
    }
}

fn response_to_events(response: MessageResponse) -> Vec<AssistantEvent> {
    let mut events = Vec::new();
    let mut pending_tools = BTreeMap::new();

    // IM-03: Cap block iteration at u32::MAX to avoid an `expect` panic if
    // a malformed or adversarial API response ever delivers more than
    // 4 billion content blocks. This would realistically never happen from
    // a real API, but the previous `expect("response block index overflow")`
    // would panic the entire agentic loop if it ever did, taking down
    // any concurrent sessions.
    //
    // Taking only u32::MAX blocks preserves existing semantics for all
    // legitimate responses while ensuring the numeric conversion cannot
    // fail. We log a warning if the cap ever triggers so operators can
    // investigate.
    let max_blocks = u32::MAX as usize;
    let total_blocks = response.content.len();
    if total_blocks > max_blocks {
        eprintln!(
            "[response_to_events] warning: truncating {total_blocks} blocks \
             to u32::MAX ({max_blocks}) — response is suspiciously large"
        );
    }

    for (index, block) in response.content.into_iter().take(max_blocks).enumerate() {
        // Safe: index < max_blocks == u32::MAX, so the conversion cannot fail.
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        push_output_block(block, index, &mut events, &mut pending_tools, false);
        if let Some((id, name, input)) = pending_tools.remove(&index) {
            events.push(AssistantEvent::ToolUse { id, name, input });
        }
    }

    events.push(AssistantEvent::Usage(TokenUsage {
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        cache_creation_input_tokens: response.usage.cache_creation_input_tokens,
        cache_read_input_tokens: response.usage.cache_read_input_tokens,
    }));
    events.push(AssistantEvent::MessageStop);
    events
}

fn with_workspace_cwd<T>(
    cwd: &Path,
    work: impl FnOnce() -> Result<T, RuntimeError>,
) -> Result<T, String> {
    let _guard = process_workspace_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let original = env::current_dir().map_err(|error| error.to_string())?;
    env::set_current_dir(cwd).map_err(|error| error.to_string())?;

    let work_result = work().map_err(|error| error.to_string());
    let restore_result = env::set_current_dir(&original).map_err(|error| error.to_string());

    match (work_result, restore_result) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(format!("failed to restore working directory: {error}")),
        (Err(error), Err(restore_error)) => Err(format!(
            "{error}\n\nAdditionally, failed to restore working directory: {restore_error}"
        )),
    }
}

/// Process-wide workspace CWD lock.
///
/// Used by both the legacy `execute_live_turn` path and the new
/// `agentic_loop::execute_tool_in_workspace` to serialize CWD
/// manipulation across concurrent sessions/tasks. Both paths MUST
/// use this same lock or they will race on `std::env::set_current_dir`.
pub(crate) fn process_workspace_lock() -> &'static StdMutex<()> {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| StdMutex::new(()))
}

fn current_date_string() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    now.format(&format_description!("[year]-[month]-[day]"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn detect_os_version() -> String {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("sw_vers", &["-productVersion"])]
    } else {
        &[("uname", &["-r"])]
    };

    for (program, args) in commands {
        if let Ok(output) = Command::new(program).args(*args).output() {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !value.is_empty() {
                    return value;
                }
            }
        }
    }

    "unknown".to_string()
}

fn humanize_model_label(model: &str) -> String {
    match model {
        "claude-opus-4-6" => "Opus 4.6".to_string(),
        "claude-sonnet-4-6" => "Sonnet 4.6".to_string(),
        _ => model.to_string(),
    }
}

/// A5.1 — derive a user-friendly session `model_label` for
/// OpenAI-compat providers resolved from providers.json. Before A5.1
/// every openai_compat session was labeled "Opus 4.6" because
/// execute_live_turn stamped `humanize_model_label(DEFAULT_MODEL_ID)`
/// regardless of the provider override. Now we read the provider_id
/// (which `resolve_runtime_credentials` sets to
/// "providers-json:<active_id>") and fall back to the model string.
fn friendly_openai_compat_label(provider_id: &str, model: Option<&str>) -> String {
    let active_id = provider_id
        .strip_prefix("providers-json:")
        .unwrap_or(provider_id);
    let pretty_provider = match active_id {
        "moonshot-kimi" => Some("Moonshot (Kimi)"),
        "deepseek" => Some("DeepSeek"),
        "qwen" | "qwen-code" => Some("Qwen"),
        "glm" | "zhipu" => Some("智谱 GLM"),
        "xai" | "grok" => Some("xAI Grok"),
        _ => None,
    };
    match (pretty_provider, model) {
        (Some(p), Some(m)) => format!("{p} · {m}"),
        (Some(p), None) => p.to_string(),
        (None, Some(m)) => m.to_string(),
        (None, None) => active_id.to_string(),
    }
}

/// A5.1 — Build a compact system prompt for the OpenAI-compat
/// streaming path. We intentionally do NOT replay the full
/// `load_system_prompt` block used by the Anthropic agentic loop —
/// OpenAI-compat providers don't do tool use from the Ask page and
/// the heavy system prompt noise regresses first-token latency. We
/// do preserve the A1/A2/A3 context-basis framing (bound source
/// prefix + mode marker + enrichment) because that's the part that
/// shapes the LLM's answer for URL-carrying turns.
fn build_system_prompt_for_openai_compat(
    _project_path: &str,
    bound_prefix: &Option<String>,
    marker: Option<&'static str>,
    enrichment: Option<&str>,
    auto_bound_source: Option<&crate::ask_context::binding::SourceRef>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(
        "You are a helpful assistant. Answer in the same language the user \
         writes in — default to Chinese when the user writes in Chinese. \
         Be concise, use Markdown when helpful."
            .to_string(),
    );
    if let Some(bind) = bound_prefix {
        parts.push(bind.clone());
    }
    if let Some(m) = marker {
        parts.push(m.to_string());
    }
    if let Some(content) = enrichment {
        parts.push(content.to_string());
    }
    if bound_prefix.is_none() && auto_bound_source.is_some() {
        parts.push(crate::ask_context::binding::grounding_instruction_block().to_string());
    }
    parts.join("\n\n")
}

/// Determines whether an OpenAI-compatible model is known to support
/// `tool_calls` in the standard ChatCompletions streaming format.
///
/// Returning true only enables the protocol. The actual exposed tool surface
/// is further filtered by `ToolExposurePolicy` (default: safe read-only only).
///
/// This is a conservative whitelist. Unknown models default to false
/// to avoid breaking text-only paths.
///
/// Reasoning models (e.g. deepseek-reasoner) are explicitly excluded
/// because their providers do not support tool_calls.
fn model_supports_openai_tools(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        // DeepSeek family
        "deepseek-chat"
            | "deepseek-v3"
            | "deepseek-v3.1"
            // Moonshot family
            | "moonshot-v1-128k"
            | "kimi-k2"
            // Qwen family
            | "qwen-plus"
            | "qwen-max"
            // OpenAI family
            | "gpt-4o"
            // GLM family
            | "glm-4-plus"
            // xAI family
            | "grok-3"
    )
    // NOT in list (intentional):
    //   "deepseek-reasoner"  - DeepSeek R1 docs: tool_calls not supported
    //   "deepseek-coder"     - tool support unconfirmed
    //   "qwen-coder"         - tool support unconfirmed
    //   "moonshot-v1-32k"    - test coverage missing
    //   "moonshot-v1-8k"     - test coverage missing
}

#[cfg(test)]
mod model_capability_tests {
    use super::*;

    #[test]
    fn deepseek_chat_supports_tools() {
        assert!(model_supports_openai_tools("deepseek-chat"));
    }

    #[test]
    fn deepseek_reasoner_does_not_support_tools() {
        assert!(!model_supports_openai_tools("deepseek-reasoner"));
    }

    #[test]
    fn unknown_model_defaults_to_unsupported() {
        assert!(!model_supports_openai_tools("foobar-3000"));
        assert!(!model_supports_openai_tools(""));
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(model_supports_openai_tools("DeepSeek-Chat"));
        assert!(model_supports_openai_tools("DEEPSEEK-CHAT"));
        assert!(model_supports_openai_tools("  gpt-4o  "));
    }

    #[test]
    fn coder_variants_explicitly_unsupported() {
        // These are conservatively excluded pending verification.
        assert!(!model_supports_openai_tools("deepseek-coder"));
        assert!(!model_supports_openai_tools("qwen-coder"));
    }
}

fn build_customize_state(project_path: String) -> DesktopCustomizeState {
    let cwd = PathBuf::from(&project_path);
    let loader = ConfigLoader::default_for(&cwd);
    let mut warnings = Vec::new();

    let runtime_config = match loader.load() {
        Ok(config) => config,
        Err(error) => {
            warnings.push(format!("failed to load runtime config: {error}"));
            RuntimeConfig::empty()
        }
    };

    let model_id =
        resolve_model_alias(runtime_config.model().unwrap_or(DEFAULT_MODEL_ID)).to_string();
    let model_label = humanize_model_label(&model_id);
    let permission_mode = runtime_config
        .permission_mode()
        .map(permission_mode_from_config_label)
        .unwrap_or_else(|| DEFAULT_PERMISSION_MODE_LABEL.to_string());

    let loaded_configs = runtime_config
        .loaded_entries()
        .iter()
        .map(|entry| DesktopConfigFile {
            source: config_source_label(entry.source).to_string(),
            path: entry.path.display().to_string(),
        })
        .collect::<Vec<_>>();

    let hooks = DesktopHookConfigView {
        pre_tool_use: runtime_config.hooks().pre_tool_use().to_vec(),
        post_tool_use: runtime_config.hooks().post_tool_use().to_vec(),
    };

    let mcp_servers = runtime_config
        .mcp()
        .servers()
        .iter()
        .map(|(name, scoped)| DesktopMcpServer {
            name: name.clone(),
            scope: config_source_label(scoped.scope).to_string(),
            transport: mcp_transport_label(&scoped.config).to_string(),
            target: mcp_target_label(&scoped.config),
        })
        .collect::<Vec<_>>();

    let plugin_manager = build_plugin_manager(&cwd, &loader, &runtime_config);
    let plugins = match plugin_manager.plugin_registry() {
        Ok(registry) => registry
            .plugins()
            .iter()
            .map(|plugin| DesktopPluginView {
                id: plugin.metadata().id.clone(),
                name: plugin.metadata().name.clone(),
                version: plugin.metadata().version.clone(),
                description: plugin.metadata().description.clone(),
                kind: plugin.metadata().kind.to_string(),
                source: plugin.metadata().source.clone(),
                root_path: plugin
                    .metadata()
                    .root
                    .as_ref()
                    .map(|path| path.display().to_string()),
                enabled: plugin.is_enabled(),
                default_enabled: plugin.metadata().default_enabled,
                tool_count: plugin.tools().len(),
                pre_tool_hook_count: plugin.hooks().pre_tool_use.len(),
                post_tool_hook_count: plugin.hooks().post_tool_use.len(),
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            warnings.push(format!("failed to discover plugins: {error}"));
            Vec::new()
        }
    };

    let enabled_plugin_count = plugins.iter().filter(|plugin| plugin.enabled).count();
    let plugin_tool_count = plugins.iter().map(|plugin| plugin.tool_count).sum();

    DesktopCustomizeState {
        project_path,
        model_id,
        model_label,
        permission_mode,
        summary: DesktopCustomizeSummary {
            loaded_config_count: loaded_configs.len(),
            mcp_server_count: mcp_servers.len(),
            plugin_count: plugins.len(),
            enabled_plugin_count,
            plugin_tool_count,
            pre_tool_hook_count: hooks.pre_tool_use.len(),
            post_tool_hook_count: hooks.post_tool_use.len(),
        },
        loaded_configs,
        hooks,
        mcp_servers,
        plugins,
        warnings,
    }
}

fn build_settings_state(project_path: String) -> DesktopSettingsState {
    let cwd = PathBuf::from(&project_path);
    let loader = ConfigLoader::default_for(&cwd);
    let config_home = loader.config_home().display().to_string();
    let desktop_session_store_path = DesktopPersistence::default_path().display().to_string();
    let plugin_manager =
        PluginManager::new(PluginManagerConfig::new(loader.config_home().to_path_buf()));
    let mut warnings = Vec::new();

    let oauth_credentials_path = match credentials_path() {
        Ok(path) => Some(path.display().to_string()),
        Err(error) => {
            warnings.push(format!("failed to resolve OAuth credentials path: {error}"));
            None
        }
    };

    let providers = vec![
        DesktopProviderSetting {
            id: "claw".to_string(),
            label: "Claw".to_string(),
            base_url: read_claw_base_url(),
            auth_status: env_auth_status(&["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"]),
        },
        DesktopProviderSetting {
            id: "xai".to_string(),
            label: "xAI".to_string(),
            base_url: read_xai_base_url(),
            auth_status: env_auth_status(&["XAI_API_KEY"]),
        },
        DesktopProviderSetting {
            id: "openai".to_string(),
            label: "OpenAI".to_string(),
            base_url: read_openai_base_url(),
            auth_status: env_auth_status(&["OPENAI_API_KEY"]),
        },
    ];

    let storage_locations = vec![
        DesktopStorageLocation {
            label: "Config home".to_string(),
            path: config_home.clone(),
            description: "Merged runtime settings, plugin settings, and local desktop metadata."
                .to_string(),
        },
        DesktopStorageLocation {
            label: "Desktop sessions".to_string(),
            path: desktop_session_store_path.clone(),
            description: "Persisted desktop Code sessions and sidebar history.".to_string(),
        },
        DesktopStorageLocation {
            label: "Plugin install root".to_string(),
            path: plugin_manager.install_root().display().to_string(),
            description: "Installed and synced bundled plugins.".to_string(),
        },
        DesktopStorageLocation {
            label: "Plugin registry".to_string(),
            path: plugin_manager.registry_path().display().to_string(),
            description: "Installed plugin registry metadata.".to_string(),
        },
    ];

    DesktopSettingsState {
        project_path,
        config_home,
        desktop_session_store_path,
        oauth_credentials_path,
        providers,
        storage_locations,
        warnings,
    }
}

fn config_source_label(source: ConfigSource) -> &'static str {
    match source {
        ConfigSource::User => "User",
        ConfigSource::Project => "Project",
        ConfigSource::Local => "Local",
    }
}

fn permission_mode_from_config_label(mode: ResolvedPermissionMode) -> String {
    match mode {
        ResolvedPermissionMode::ReadOnly => "Read only".to_string(),
        ResolvedPermissionMode::WorkspaceWrite => "Workspace write".to_string(),
        ResolvedPermissionMode::DangerFullAccess => "Danger full access".to_string(),
    }
}

fn mcp_transport_label(config: &McpServerConfig) -> &'static str {
    match config {
        McpServerConfig::Stdio(_) => "stdio",
        McpServerConfig::Sse(_) => "sse",
        McpServerConfig::Http(_) => "http",
        McpServerConfig::Ws(_) => "ws",
        McpServerConfig::Sdk(_) => "sdk",
        McpServerConfig::ManagedProxy(_) => "managed_proxy",
    }
}

fn mcp_target_label(config: &McpServerConfig) -> String {
    match config {
        McpServerConfig::Stdio(config) => config.command.clone(),
        McpServerConfig::Sse(config) | McpServerConfig::Http(config) => config.url.clone(),
        McpServerConfig::Ws(config) => config.url.clone(),
        McpServerConfig::Sdk(config) => config.name.clone(),
        McpServerConfig::ManagedProxy(config) => format!("{} ({})", config.id, config.url),
    }
}

fn env_auth_status(keys: &[&str]) -> String {
    if keys
        .iter()
        .any(|key| env::var_os(key).is_some_and(|value| !value.is_empty()))
    {
        "Configured in environment".to_string()
    } else {
        "Not configured".to_string()
    }
}

fn read_openai_base_url() -> String {
    env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string())
}

fn session_search_snippet(record: &DesktopSessionRecord, normalized_query: &str) -> Option<String> {
    for candidate in [
        record.metadata.title.as_str(),
        record.metadata.preview.as_str(),
        record.metadata.project_name.as_str(),
        record.metadata.project_path.as_str(),
    ] {
        if candidate.to_ascii_lowercase().contains(normalized_query) {
            return Some(truncate_snippet(candidate));
        }
    }

    for message in &record.session.messages {
        for block in &message.blocks {
            let Some(searchable) = searchable_block_text(block) else {
                continue;
            };
            if searchable.to_ascii_lowercase().contains(normalized_query) {
                return Some(truncate_snippet(searchable));
            }
        }
    }

    None
}

fn searchable_block_text(block: &ContentBlock) -> Option<&str> {
    match block {
        ContentBlock::Text { text } => Some(text.as_str()),
        ContentBlock::ToolUse { input, .. } => Some(input.as_str()),
        ContentBlock::ToolResult { output, .. } => Some(output.as_str()),
    }
}

fn truncate_snippet(value: &str) -> String {
    const LIMIT: usize = 160;
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= LIMIT {
        collapsed
    } else {
        collapsed.chars().take(LIMIT - 1).collect::<String>() + "…"
    }
}

fn fallback_turn_result(
    mut session: RuntimeSession,
    user_message: &str,
    model_label: String,
    error: String,
) -> DesktopTurnResult {
    session
        .messages
        .push(ConversationMessage::user_text(user_message.to_string()));
    session.messages.push(assistant_text(format!(
        "Desktop runtime couldn't execute this turn.\n\n{error}"
    )));

    DesktopTurnResult {
        session,
        model_label,
    }
}

fn default_true() -> bool {
    true
}

fn default_dispatch_status() -> DesktopDispatchStatus {
    DesktopDispatchStatus::Unread
}

fn build_scheduled_task(
    task: &ScheduledTaskMetadata,
    session_context: &DesktopSessionContext,
    now: u64,
) -> DesktopScheduledTask {
    let blocked_reason = scheduled_task_blocked_reason(task, session_context);
    let next_run_at = next_scheduled_run_at(task, now);
    DesktopScheduledTask {
        id: task.id.clone(),
        title: task.title.clone(),
        prompt: task.prompt.clone(),
        project_name: task.project_name.clone(),
        project_path: task.project_path.clone(),
        schedule: task.schedule.clone(),
        schedule_label: scheduled_schedule_label(&task.schedule),
        target: scheduled_task_target(task, session_context),
        enabled: task.enabled,
        blocked_reason,
        status: if task.running {
            DesktopScheduledTaskStatus::Running
        } else {
            DesktopScheduledTaskStatus::Idle
        },
        created_at: task.created_at,
        updated_at: task.updated_at,
        last_run_at: task.last_run_at,
        next_run_at,
        last_run_status: task.last_run_status,
        last_outcome: task.last_outcome.clone(),
    }
}

fn build_dispatch_item(
    item: &DispatchItemMetadata,
    session_context: &DesktopSessionContext,
) -> DesktopDispatchItem {
    DesktopDispatchItem {
        id: item.id.clone(),
        title: item.title.clone(),
        body: item.body.clone(),
        project_name: item.project_name.clone(),
        project_path: item.project_path.clone(),
        source: DesktopDispatchSource {
            kind: item.source_kind,
            label: item.source_label.clone(),
        },
        priority: item.priority,
        target: dispatch_target(item, session_context),
        status: item.status,
        created_at: item.created_at,
        updated_at: item.updated_at,
        delivered_at: item.delivered_at,
        last_outcome: item.last_outcome.clone(),
    }
}

fn is_task_due(
    task: &ScheduledTaskMetadata,
    session_context: &DesktopSessionContext,
    now: u64,
) -> bool {
    task.enabled
        && !task.running
        && scheduled_task_blocked_reason(task, session_context).is_none()
        && next_scheduled_run_at(task, now).is_some_and(|next_run_at| next_run_at <= now)
}

fn validate_dispatch_status_transition(
    status: DesktopDispatchStatus,
) -> Result<(), DesktopStateError> {
    if status == DesktopDispatchStatus::Delivering {
        return Err(DesktopStateError::InvalidDispatchItem(
            "deliveries must be started through the deliver action".to_string(),
        ));
    }

    Ok(())
}

fn normalize_dispatch_title(title: &str) -> Result<String, DesktopStateError> {
    let normalized = normalize_session_title(title);
    if normalized == "New session" && title.trim().is_empty() {
        return Err(DesktopStateError::InvalidDispatchItem(
            "dispatch title cannot be empty".to_string(),
        ));
    }
    Ok(normalized)
}

fn normalize_dispatch_body(body: &str) -> Result<String, DesktopStateError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(DesktopStateError::InvalidDispatchItem(
            "dispatch body cannot be empty".to_string(),
        ));
    }
    Ok(body.to_string())
}

fn normalize_scheduled_title(title: &str) -> Result<String, DesktopStateError> {
    let normalized = normalize_session_title(title);
    if normalized == "New session" && title.trim().is_empty() {
        return Err(DesktopStateError::InvalidScheduledTask(
            "scheduled task title cannot be empty".to_string(),
        ));
    }
    Ok(normalized)
}

fn normalize_scheduled_prompt(prompt: &str) -> Result<String, DesktopStateError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(DesktopStateError::InvalidScheduledTask(
            "scheduled task prompt cannot be empty".to_string(),
        ));
    }
    Ok(prompt.to_string())
}

fn validate_scheduled_schedule(
    schedule: &DesktopScheduledSchedule,
) -> Result<(), DesktopStateError> {
    match schedule {
        DesktopScheduledSchedule::Hourly { interval_hours } => {
            if *interval_hours == 0 {
                return Err(DesktopStateError::InvalidScheduledTask(
                    "hourly schedules must run every 1 hour or more".to_string(),
                ));
            }
        }
        DesktopScheduledSchedule::Weekly { days, hour, minute } => {
            if days.is_empty() {
                return Err(DesktopStateError::InvalidScheduledTask(
                    "weekly schedules need at least one day".to_string(),
                ));
            }
            if *hour > 23 || *minute > 59 {
                return Err(DesktopStateError::InvalidScheduledTask(
                    "weekly schedules must use a valid local time".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn scheduled_task_target(
    task: &ScheduledTaskMetadata,
    session_context: &DesktopSessionContext,
) -> DesktopScheduledTaskTarget {
    match &task.target_session_id {
        Some(session_id) => DesktopScheduledTaskTarget {
            kind: DesktopScheduledTaskTargetKind::ExistingSession,
            session_id: Some(session_id.clone()),
            label: session_context
                .session_titles
                .get(session_id)
                .cloned()
                .unwrap_or_else(|| "Missing session".to_string()),
        },
        None => DesktopScheduledTaskTarget {
            kind: DesktopScheduledTaskTargetKind::NewSession,
            session_id: None,
            label: "Start a fresh session".to_string(),
        },
    }
}

fn dispatch_target(
    item: &DispatchItemMetadata,
    session_context: &DesktopSessionContext,
) -> DesktopDispatchTarget {
    match &item.target_session_id {
        Some(session_id) if !item.prefer_new_session => DesktopDispatchTarget {
            kind: DesktopDispatchTargetKind::ExistingSession,
            session_id: Some(session_id.clone()),
            label: session_context
                .session_titles
                .get(session_id)
                .cloned()
                .unwrap_or_else(|| "Missing session".to_string()),
        },
        _ => DesktopDispatchTarget {
            kind: DesktopDispatchTargetKind::NewSession,
            session_id: item.target_session_id.clone(),
            label: if let Some(session_id) = &item.target_session_id {
                session_context
                    .session_titles
                    .get(session_id)
                    .cloned()
                    .unwrap_or_else(|| "Delivered into new session".to_string())
            } else {
                "Start a fresh session".to_string()
            },
        },
    }
}

fn scheduled_task_blocked_reason(
    task: &ScheduledTaskMetadata,
    session_context: &DesktopSessionContext,
) -> Option<String> {
    if !session_context
        .trusted_project_paths
        .contains(&task.project_path)
    {
        return Some("This project path is not trusted for scheduled execution yet.".to_string());
    }

    if let Some(session_id) = &task.target_session_id {
        if !session_context.session_titles.contains_key(session_id) {
            return Some("The target session is no longer available.".to_string());
        }
    }

    None
}

fn scheduled_schedule_label(schedule: &DesktopScheduledSchedule) -> String {
    match schedule {
        DesktopScheduledSchedule::Hourly { interval_hours } => {
            if *interval_hours == 1 {
                "Every hour".to_string()
            } else {
                format!("Every {interval_hours} hours")
            }
        }
        DesktopScheduledSchedule::Weekly { days, hour, minute } => {
            let days = days
                .iter()
                .map(|day| day.short_label())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Weekly on {days} at {:02}:{:02}", hour, minute)
        }
    }
}

fn next_scheduled_run_at(task: &ScheduledTaskMetadata, now: u64) -> Option<u64> {
    match &task.schedule {
        DesktopScheduledSchedule::Hourly { interval_hours } => {
            let interval_millis = u64::from(*interval_hours) * 60 * 60 * 1000;
            let anchor = task.last_run_at.unwrap_or(task.created_at);
            Some(anchor.saturating_add(interval_millis))
        }
        DesktopScheduledSchedule::Weekly { days, hour, minute } => next_weekly_run_at(
            days,
            *hour,
            *minute,
            task.last_run_at.unwrap_or(now.max(task.created_at)),
        ),
    }
}

fn next_weekly_run_at(days: &[DesktopWeekday], hour: u8, minute: u8, anchor: u64) -> Option<u64> {
    let offset = local_offset();
    let anchor_local = offset_date_time_from_millis(anchor).to_offset(offset);
    let start_date = anchor_local.date();

    for day_offset in 0..14 {
        let date = start_date + TimeDuration::days(day_offset);
        if !days.contains(&DesktopWeekday::from(date.weekday())) {
            continue;
        }

        let local_candidate =
            PrimitiveDateTime::new(date, time::Time::from_hms(hour, minute, 0).ok()?)
                .assume_offset(offset);
        if local_candidate > anchor_local {
            return Some(offset_date_time_to_millis(local_candidate));
        }
    }

    None
}

fn local_offset() -> UtcOffset {
    OffsetDateTime::now_local()
        .map(|now| now.offset().to_owned())
        .unwrap_or(UtcOffset::UTC)
}

fn offset_date_time_from_millis(timestamp: u64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp) * 1_000_000)
        .expect("millisecond timestamp should fit into OffsetDateTime")
}

fn offset_date_time_to_millis(timestamp: OffsetDateTime) -> u64 {
    (timestamp.unix_timestamp_nanos() / 1_000_000) as u64
}

fn top_tab(id: &str, label: &str, kind: DesktopTabKind, closable: bool) -> DesktopTopTab {
    DesktopTopTab {
        id: id.to_string(),
        label: label.to_string(),
        kind,
        closable,
    }
}

fn settings_group(id: &str, label: &str, description: &str) -> DesktopSettingsGroup {
    DesktopSettingsGroup {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
    }
}

fn nav_action(
    id: &str,
    label: &str,
    icon: &str,
    target_tab_id: &str,
    kind: DesktopTabKind,
) -> DesktopSidebarAction {
    DesktopSidebarAction {
        id: id.to_string(),
        label: label.to_string(),
        icon: icon.to_string(),
        target_tab_id: target_tab_id.to_string(),
        kind,
    }
}

fn session_record(metadata: SessionMetadata) -> DesktopSessionRecord {
    let (events, _) = broadcast::channel(BROADCAST_CAPACITY);
    DesktopSessionRecord {
        metadata,
        session: RuntimeSession::new(),
        events,
    }
}

fn bucket_from_label(label: &str) -> DesktopSessionBucket {
    match label {
        "Today" => DesktopSessionBucket::Today,
        "Yesterday" => DesktopSessionBucket::Yesterday,
        _ => DesktopSessionBucket::Older,
    }
}

fn seeded_sessions() -> Vec<DesktopSessionRecord> {
    let now = unix_timestamp_millis();
    vec![
        seeded_record(
            "desktop-session-1",
            "New session",
            "1. 仔细分析",
            DesktopSessionBucket::Today,
            now.saturating_sub(10 * 60 * 1000),
            now.saturating_sub(2 * 60 * 1000),
            vec![
                ConversationMessage::user_text("1. 仔细分析"),
                assistant_text(
                    "We have enough context to start shaping the desktop shell around the Rust runtime.",
                ),
            ],
        ),
        seeded_record(
            "desktop-session-2",
            "Analyze cross-platform AI assistant desktop shell",
            "Map the Rust runtime to a desktop-first workbench.",
            DesktopSessionBucket::Today,
            now.saturating_sub(3 * 60 * 60 * 1000),
            now.saturating_sub(150 * 60 * 1000),
            vec![
                ConversationMessage::user_text(
                    "Analyze cross-platform AI assistant desktop shell.",
                ),
                assistant_text(
                    "Start with session persistence, shell tabs, and a local streaming API.",
                ),
            ],
        ),
        seeded_record(
            "desktop-session-3",
            "Start local development client for testing",
            "Wire a local server and a lightweight workspace client.",
            DesktopSessionBucket::Yesterday,
            now.saturating_sub(28 * 60 * 60 * 1000),
            now.saturating_sub(27 * 60 * 60 * 1000),
            vec![
                ConversationMessage::user_text("Start local development client for testing."),
                assistant_text("Use a Rust HTTP+SSE layer so the UI stays thin."),
            ],
        ),
        seeded_record(
            "desktop-session-4",
            "Sync local code to GitHub repository",
            "Prepare desktop shell scaffolding before syncing broader changes.",
            DesktopSessionBucket::Older,
            now.saturating_sub(72 * 60 * 60 * 1000),
            now.saturating_sub(70 * 60 * 60 * 1000),
            vec![
                ConversationMessage::user_text("Sync local code to GitHub repository."),
                assistant_text(
                    "Keep the desktop app in isolated crates and apps so the CLI stays stable.",
                ),
            ],
        ),
    ]
}

fn seeded_record(
    id: &str,
    title: &str,
    preview: &str,
    bucket: DesktopSessionBucket,
    created_at: u64,
    updated_at: u64,
    messages: Vec<ConversationMessage>,
) -> DesktopSessionRecord {
    let (events, _) = broadcast::channel(BROADCAST_CAPACITY);
    DesktopSessionRecord {
        metadata: SessionMetadata {
            id: id.to_string(),
            title: title.to_string(),
            preview: preview.to_string(),
            bucket,
            created_at,
            updated_at,
            project_name: DEFAULT_PROJECT_NAME.to_string(),
            project_path: default_project_path().to_string(),
            environment_label: DEFAULT_ENVIRONMENT_LABEL.to_string(),
            model_label: DEFAULT_MODEL_LABEL.to_string(),
            turn_state: DesktopTurnState::Idle,
            lifecycle_status: DesktopLifecycleStatus::Todo,
            flagged: false,
            last_context_basis: None,
            // A2: seeded fixtures start with no binding.
            source_binding: None,
        },
        session: {
            let mut session = RuntimeSession::new();
            session.version = 1;
            session.messages = messages;
            session
        },
        events,
    }
}

fn assistant_text(text: impl Into<String>) -> ConversationMessage {
    ConversationMessage::assistant(vec![ContentBlock::Text { text: text.into() }])
}

fn normalize_session_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        "New session".to_string()
    } else {
        trimmed.to_string()
    }
}

fn session_title_from_message(message: &str) -> String {
    normalize_session_title(&truncate_title(message))
}

/// Titles the desktop runtime or the frontend stamps on a freshly
/// created session before the user sends anything. When we see any
/// of these on `append_user_message`, we replace them with a real
/// title derived from the first message.
///
/// Recognition strategy:
///   1. Exact-match a short whitelist of backend-authored / translated
///      placeholders (`New session` / `新会话` / `新对话`).
///   2. Structural match for the frontend default emitted by
///      `useAskSession.ts`: `"Ask <SEP> new conversation"` where
///      `<SEP>` is any single non-alphanumeric character. This is
///      how we catch both the canonical U+00B7 middle-dot variant
///      (what the frontend ships) and the U+003F ASCII-question-mark
///      variant that older Windows persist paths produced by stripping
///      the multi-byte char. A further variant (em-dash, bullet, …)
///      would auto-qualify without another code change.
///
/// The alphanumeric check defends against a user intentionally naming
/// a session "Ask a new conversation" — we only accept separators
/// that clearly look like decorative characters.
fn is_default_session_title(title: &str) -> bool {
    let t = title.trim();
    if matches!(t, "New session" | "新会话" | "新对话") {
        return true;
    }
    if !t.starts_with("Ask ") {
        return false;
    }
    let lower = t.to_lowercase();
    if !lower.ends_with(" new conversation") {
        return false;
    }
    // Slice middle bytes: `t[4 .. len-17]`. `"Ask "` is 4 ASCII bytes
    // and `" new conversation"` is 17 ASCII bytes, so both endpoints
    // land on UTF-8 char boundaries — anything in the middle (ASCII
    // or multi-byte) is safe to re-scan as chars.
    let middle_bytes = match t
        .len()
        .checked_sub("Ask ".len() + " new conversation".len())
    {
        Some(n) if n > 0 => &t["Ask ".len()..t.len() - " new conversation".len()],
        _ => return false,
    };
    let mut chars = middle_bytes.chars();
    let Some(sep) = chars.next() else {
        return false;
    };
    if chars.next().is_some() {
        return false;
    }
    !sep.is_alphanumeric()
}

#[cfg(test)]
#[test]
fn default_session_title_recognizes_real_variants() {
    // Exact matches.
    assert!(is_default_session_title("New session"));
    assert!(is_default_session_title("新会话"));
    assert!(is_default_session_title("新对话"));
    // Frontend-emitted default (U+00B7 middle dot).
    assert!(is_default_session_title("Ask · new conversation"));
    // Windows persist-roundtrip-corrupted variant (U+003F '?').
    assert!(is_default_session_title("Ask ? new conversation"));
    // Case insensitivity on the "new conversation" suffix.
    assert!(is_default_session_title("Ask · New conversation"));
    // Another plausible corruption (em dash) should also match
    // without a whitelist edit.
    assert!(is_default_session_title("Ask — new conversation"));
    // A user-typed real title must NOT be silently overwritten.
    assert!(!is_default_session_title("Ask a new conversation"));
    assert!(!is_default_session_title("Ask about the deploy"));
    assert!(!is_default_session_title("random title"));
    // Explicit empty / unrelated.
    assert!(!is_default_session_title(""));
    assert!(!is_default_session_title("Ask"));
}

fn truncate_title(message: &str) -> String {
    const LIMIT: usize = 72;
    let first_line = message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("New session")
        .trim();
    if first_line.chars().count() <= LIMIT {
        first_line.to_string()
    } else {
        first_line.chars().take(LIMIT - 1).collect::<String>() + "…"
    }
}

fn truncate_preview(message: &str) -> String {
    const LIMIT: usize = 88;
    let trimmed = message.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }

    trimmed.chars().take(LIMIT - 1).collect::<String>() + "…"
}

fn unix_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::{
        AppendDesktopMessageRequest, CreateDesktopDispatchItemRequest,
        CreateDesktopScheduledTaskRequest, CreateDesktopSessionRequest, DesktopDispatchPriority,
        DesktopDispatchStatus, DesktopLifecycleStatus, DesktopPersistence,
        DesktopScheduledRunStatus, DesktopScheduledSchedule, DesktopSessionBucket, DesktopState,
        DesktopStateError, DesktopTurnState,
    };
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn seeded_state_exposes_default_workbench() {
        let state = DesktopState::default();
        let workbench = state.workbench().await;

        assert_eq!(workbench.project_name, "Warwolf");
        assert!(workbench.active_session_id.is_some());
        assert_eq!(workbench.session_sections.len(), 3);
        assert_eq!(workbench.composer.model_label, "Opus 4.6");
    }

    #[tokio::test]
    async fn create_and_append_message_updates_session() {
        let state = DesktopState::default();
        let session = state
            .create_session(CreateDesktopSessionRequest {
                title: Some("Desktop phase 1".to_string()),
                project_name: None,
                project_path: None,
            })
            .await;

        let detail = state
            .append_user_message(
                &session.id,
                AppendDesktopMessageRequest {
                    message: "Hook up the session view.".to_string(),
                    mode: None,
                }
                .message,
                None,
            )
            .await
            .expect("message append should succeed");

        assert_eq!(detail.session.messages.len(), 1);
        assert_eq!(detail.preview, "Hook up the session view.");
        assert_eq!(detail.turn_state, DesktopTurnState::Running);

        let completed = loop {
            let detail = state
                .get_session(&session.id)
                .await
                .expect("session should still exist");
            if detail.turn_state == DesktopTurnState::Idle {
                break detail;
            }
            sleep(Duration::from_millis(10)).await;
        };

        assert_eq!(completed.session.messages.len(), 2);
        assert_eq!(completed.turn_state, DesktopTurnState::Idle);
    }

    #[tokio::test]
    async fn scheduled_tasks_can_be_created_and_run_manually() {
        let state = DesktopState::default();
        let task = state
            .create_scheduled_task(CreateDesktopScheduledTaskRequest {
                title: "Workspace sweep".to_string(),
                prompt: "Review the workspace and queue the next implementation step.".to_string(),
                project_name: None,
                project_path: None,
                target_session_id: None,
                schedule: DesktopScheduledSchedule::Hourly { interval_hours: 4 },
            })
            .await
            .expect("scheduled task should be created");

        assert_eq!(task.title, "Workspace sweep");
        assert_eq!(task.status, super::DesktopScheduledTaskStatus::Idle);

        let running = state
            .run_scheduled_task_now(&task.id)
            .await
            .expect("manual run should queue");
        assert_eq!(running.status, super::DesktopScheduledTaskStatus::Running);

        let completed = loop {
            let scheduled = state.scheduled().await;
            let task = scheduled
                .tasks
                .iter()
                .find(|candidate| candidate.id == running.id)
                .cloned()
                .expect("scheduled task should still exist");
            if task.status == super::DesktopScheduledTaskStatus::Idle {
                break task;
            }
            sleep(Duration::from_millis(10)).await;
        };

        assert_eq!(
            completed.last_run_status,
            Some(DesktopScheduledRunStatus::Success)
        );
        assert!(completed.last_outcome.is_some());
        assert!(
            state.list_sessions().await.len() >= 5,
            "manual run should create an additional desktop session"
        );
    }

    #[tokio::test]
    async fn dispatch_items_can_be_created_and_delivered() {
        let state = DesktopState::default();
        let item = state
            .create_dispatch_item(CreateDesktopDispatchItemRequest {
                title: "Inbox follow-up".to_string(),
                body: "Continue the desktop implementation from the dispatch inbox.".to_string(),
                project_name: None,
                project_path: None,
                target_session_id: None,
                priority: DesktopDispatchPriority::High,
            })
            .await
            .expect("dispatch item should be created");

        assert_eq!(item.status, DesktopDispatchStatus::Unread);

        let delivered = state
            .deliver_dispatch_item(&item.id)
            .await
            .expect("dispatch item should be delivered");
        assert_eq!(delivered.status, DesktopDispatchStatus::Delivered);
        assert!(delivered.delivered_at.is_some());
        assert_eq!(state.list_sessions().await.len(), 5);
    }

    #[tokio::test]
    async fn scheduled_tasks_can_be_updated() {
        let state = DesktopState::default();
        let task = state
            .create_scheduled_task(CreateDesktopScheduledTaskRequest {
                title: "Workspace sweep".to_string(),
                prompt: "Review the workspace and queue the next implementation step.".to_string(),
                project_name: None,
                project_path: None,
                target_session_id: None,
                schedule: DesktopScheduledSchedule::Hourly { interval_hours: 4 },
            })
            .await
            .expect("scheduled task should be created");

        let updated = state
            .update_scheduled_task(
                &task.id,
                Some("Updated sweep".to_string()),
                Some("Inspect the current workspace state.".to_string()),
                Some(false),
            )
            .await
            .expect("scheduled task should be updated");

        assert_eq!(updated.title, "Updated sweep");
        assert_eq!(updated.prompt, "Inspect the current workspace state.");
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn dispatch_items_can_be_updated() {
        let state = DesktopState::default();
        let item = state
            .create_dispatch_item(CreateDesktopDispatchItemRequest {
                title: "Inbox follow-up".to_string(),
                body: "Continue the desktop implementation from the dispatch inbox.".to_string(),
                project_name: None,
                project_path: None,
                target_session_id: None,
                priority: DesktopDispatchPriority::Normal,
            })
            .await
            .expect("dispatch item should be created");

        let updated = state
            .update_dispatch_item(
                &item.id,
                Some("Escalated follow-up".to_string()),
                Some("Continue from the new managed auth regression.".to_string()),
                Some(DesktopDispatchPriority::High),
            )
            .await
            .expect("dispatch item should be updated");

        assert_eq!(updated.title, "Escalated follow-up");
        assert_eq!(
            updated.body,
            "Continue from the new managed auth regression."
        );
        assert_eq!(updated.priority, DesktopDispatchPriority::High);
    }

    #[tokio::test]
    async fn legacy_persisted_sessions_load_with_metadata_backfill() {
        let path = legacy_sessions_fixture_path();
        let payload = r#"{
  "next_session_id": 15,
  "sessions": [
    {
      "metadata": {
        "id": "desktop-session-4",
        "title": "Legacy session",
        "preview": "Legacy preview",
        "bucket": "older",
        "created_at": 1774960754306,
        "updated_at": 1774967954306,
        "project_name": "Warwolf",
        "project_path": "/Users/champion/Documents/develop/Warwolf/open-claude-code",
        "environment_label": "Local",
        "model_label": "Opus 4.6",
        "turn_state": "idle"
      },
      "session": {
        "version": 1,
        "messages": [
          {
            "role": "user",
            "blocks": [
              {
                "type": "text",
                "text": "Legacy prompt"
              }
            ],
            "usage": null
          }
        ]
      }
    }
  ]
}"#;
        fs::write(&path, payload).expect("legacy fixture should be written");

        let state = DesktopState::with_executor(
            Arc::new(super::MockTurnExecutor),
            Some(Arc::new(DesktopPersistence { path: path.clone() })),
            None,
            None,
        );

        let sessions = state.list_sessions().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "desktop-session-4");

        let detail = state
            .get_session("desktop-session-4")
            .await
            .expect("legacy session should load");
        assert_eq!(detail.session.session_id, "desktop-session-4");
        assert_eq!(detail.session.created_at_ms, 1774960754306);
        assert_eq!(detail.session.updated_at_ms, 1774967954306);
        assert_eq!(detail.session.messages.len(), 1);

        let _ = fs::remove_file(path);
    }

    // ── A1.1 persistence of ContextBasis on SessionMetadata ─────────

    /// Legacy session metadata JSON (pre-A1.1) doesn't have the
    /// `last_context_basis` field. It must deserialize cleanly with
    /// `last_context_basis = None` (serde default).
    #[test]
    fn session_metadata_legacy_json_without_last_context_basis_deserializes_to_none() {
        let legacy_json = r#"{
            "id": "desktop-session-legacy",
            "title": "Legacy session",
            "preview": "hello",
            "bucket": "today",
            "created_at": 0,
            "updated_at": 0,
            "project_name": "test",
            "project_path": "/tmp/x",
            "environment_label": "Local",
            "model_label": "Claude",
            "turn_state": "idle",
            "lifecycle_status": "todo",
            "flagged": false
        }"#;
        let meta: super::SessionMetadata = serde_json::from_str(legacy_json)
            .expect("legacy SessionMetadata JSON (pre-A1.1) must deserialize");
        assert!(
            meta.last_context_basis.is_none(),
            "legacy metadata must map last_context_basis to None, got {:?}",
            meta.last_context_basis
        );
    }

    /// Fresh metadata with a basis round-trips cleanly, and the field
    /// is included in the output when Some (UI relies on this).
    #[test]
    fn session_metadata_with_basis_round_trips_through_json() {
        let meta = super::SessionMetadata {
            id: "desktop-session-rt".to_string(),
            title: "Round-trip".to_string(),
            preview: "p".to_string(),
            bucket: super::DesktopSessionBucket::Today,
            created_at: 1,
            updated_at: 2,
            project_name: "proj".to_string(),
            project_path: "/tmp/proj".to_string(),
            environment_label: "Local".to_string(),
            model_label: "Claude".to_string(),
            turn_state: super::DesktopTurnState::Idle,
            lifecycle_status: super::DesktopLifecycleStatus::InProgress,
            flagged: false,
            last_context_basis: Some(crate::ask_context::ContextBasis::new(
                crate::ask_context::ContextMode::SourceFirst,
                0,
                Some(400),
            )),
            // A2: no binding — ensures the round-trip test still
            // covers the legacy shape where binding is absent.
            source_binding: None,
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        assert!(
            json.contains("\"last_context_basis\""),
            "Some(basis) must serialize (skip_serializing_if only skips None): {json}"
        );
        assert!(
            json.contains("\"source_first\""),
            "basis.mode must serialize as snake_case: {json}"
        );
        let decoded: super::SessionMetadata = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.last_context_basis, meta.last_context_basis);
    }

    /// When basis is None, the field must be omitted from the JSON
    /// (so old consumers / disk footprint stays tidy).
    #[test]
    fn session_metadata_with_none_basis_omits_field_in_json() {
        let meta = super::SessionMetadata {
            id: "desktop-session-none".to_string(),
            title: "None".to_string(),
            preview: "p".to_string(),
            bucket: super::DesktopSessionBucket::Today,
            created_at: 1,
            updated_at: 2,
            project_name: "proj".to_string(),
            project_path: "/tmp/proj".to_string(),
            environment_label: "Local".to_string(),
            model_label: "Claude".to_string(),
            turn_state: super::DesktopTurnState::Idle,
            lifecycle_status: super::DesktopLifecycleStatus::Todo,
            flagged: false,
            last_context_basis: None,
            source_binding: None,
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        assert!(
            !json.contains("last_context_basis"),
            "None should be skipped by skip_serializing_if: {json}"
        );
        // A2: None binding is also skipped — disk footprint stays
        // tidy and legacy consumers don't see an unexpected field.
        assert!(
            !json.contains("source_binding"),
            "None source_binding should be skipped too: {json}"
        );
    }

    /// A2: SessionMetadata with a populated source_binding round-trips
    /// through JSON unchanged and the field is included in the output
    /// (UI relies on the snake_case shape).
    #[test]
    fn session_metadata_with_binding_round_trips() {
        use crate::ask_context::binding::{SessionSourceBinding, SourceRef};

        let meta = super::SessionMetadata {
            id: "desktop-session-bind-rt".to_string(),
            title: "Bound".to_string(),
            preview: "p".to_string(),
            bucket: super::DesktopSessionBucket::Today,
            created_at: 1,
            updated_at: 2,
            project_name: "proj".to_string(),
            project_path: "/tmp/proj".to_string(),
            environment_label: "Local".to_string(),
            model_label: "Claude".to_string(),
            turn_state: super::DesktopTurnState::Idle,
            lifecycle_status: super::DesktopLifecycleStatus::InProgress,
            flagged: false,
            last_context_basis: None,
            source_binding: Some(SessionSourceBinding {
                source: SourceRef::Wiki {
                    slug: "foo-bar".to_string(),
                    title: "Foo Bar".to_string(),
                },
                bound_at: 1_700_000_000_000,
                binding_reason: Some("pinned".to_string()),
            }),
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        assert!(
            json.contains("\"source_binding\""),
            "Some(binding) must serialize: {json}"
        );
        assert!(
            json.contains("\"wiki\""),
            "source.kind must encode as wiki: {json}"
        );
        assert!(
            json.contains("\"foo-bar\""),
            "source.slug must round-trip: {json}"
        );
        let decoded: super::SessionMetadata = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.source_binding, meta.source_binding);
    }

    #[test]
    fn validate_project_path_rejects_empty() {
        let r = super::validate_project_path("");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("empty"));
    }

    #[test]
    fn validate_project_path_rejects_traversal() {
        let r = super::validate_project_path("/tmp/../etc");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains(".."));

        let r2 = super::validate_project_path("./..");
        assert!(r2.is_err());
    }

    #[test]
    fn validate_project_path_rejects_nonexistent() {
        let r = super::validate_project_path("/this/path/does/not/exist/anywhere");
        assert!(r.is_err());
    }

    #[test]
    fn validate_project_path_accepts_temp_dir() {
        // std::env::temp_dir() always exists
        let tmp = std::env::temp_dir();
        let result = super::validate_project_path(&tmp.display().to_string());
        assert!(
            result.is_ok(),
            "expected temp dir to be accepted, got: {result:?}"
        );
    }

    #[test]
    fn validate_project_path_rejects_files() {
        // Create a temporary file and verify validation rejects it.
        let path = std::env::temp_dir().join(format!(
            "validate-test-file-{}-{}",
            std::process::id(),
            super::unix_timestamp_millis()
        ));
        fs::write(&path, b"hello").unwrap();
        let result = super::validate_project_path(&path.display().to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a directory"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn permission_mode_accepts_snake_case() {
        // Cannot easily test set_permission_mode without async + filesystem.
        // Instead, verify the normalization logic by direct comparison —
        // call the public API on a temp dir to confirm both forms are accepted.
        // Async version below.
    }

    #[tokio::test]
    async fn permission_mode_accepts_both_naming_styles() {
        let state = super::DesktopState::with_executor(
            std::sync::Arc::new(super::MockTurnExecutor),
            None,
            None,
            None,
        );

        // Need a real directory for set_permission_mode to write to.
        let tmp = std::env::temp_dir().join(format!(
            "perm-mode-test-{}-{}",
            std::process::id(),
            super::unix_timestamp_millis()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let path_str = tmp.display().to_string();

        // Both camelCase forms.
        for mode in ["default", "acceptEdits", "bypassPermissions", "plan"] {
            let result = state.set_permission_mode(&path_str, mode).await;
            assert!(
                result.is_ok(),
                "camelCase {mode} should be accepted: {result:?}"
            );
        }

        // Both snake_case forms (the new compatibility additions).
        for mode in ["accept_edits", "bypass_permissions"] {
            let result = state.set_permission_mode(&path_str, mode).await;
            assert!(
                result.is_ok(),
                "snake_case {mode} should be accepted: {result:?}"
            );
        }

        // Invalid still rejected.
        let bad = state.set_permission_mode(&path_str, "workspaceWrite").await;
        assert!(bad.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn session_lifecycle_defaults_and_transitions() {
        let state =
            DesktopState::with_executor(Arc::new(super::MockTurnExecutor), None, None, None);

        // New sessions start as Todo, unflagged.
        let created = state
            .create_session(super::CreateDesktopSessionRequest {
                title: Some("lifecycle test".into()),
                project_name: None,
                project_path: None,
            })
            .await;
        assert_eq!(created.lifecycle_status, DesktopLifecycleStatus::Todo);
        assert!(!created.flagged);

        // Explicit status update.
        let updated = state
            .set_session_lifecycle_status(&created.id, DesktopLifecycleStatus::NeedsReview)
            .await
            .expect("lifecycle update should succeed");
        assert_eq!(
            updated.lifecycle_status,
            DesktopLifecycleStatus::NeedsReview
        );

        // Flag toggle.
        let flagged = state
            .set_session_flagged(&created.id, true)
            .await
            .expect("flag update should succeed");
        assert!(flagged.flagged);

        let unflagged = state
            .set_session_flagged(&created.id, false)
            .await
            .expect("unflag should succeed");
        assert!(!unflagged.flagged);

        // Cycle through all status values.
        for status in [
            DesktopLifecycleStatus::Todo,
            DesktopLifecycleStatus::InProgress,
            DesktopLifecycleStatus::NeedsReview,
            DesktopLifecycleStatus::Done,
            DesktopLifecycleStatus::Archived,
        ] {
            let out = state
                .set_session_lifecycle_status(&created.id, status)
                .await
                .expect("status transition should succeed");
            assert_eq!(out.lifecycle_status, status);
        }

        // Unknown session → error.
        let err = state
            .set_session_lifecycle_status("nonexistent", DesktopLifecycleStatus::Done)
            .await;
        assert!(matches!(err, Err(DesktopStateError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn mcp_call_before_init_returns_error() {
        let state =
            DesktopState::with_executor(Arc::new(super::MockTurnExecutor), None, None, None);
        // Manager is None until ensure_mcp_initialized runs.
        let result = state
            .mcp_call_tool("mcp__foo__bar", serde_json::json!({}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("MCP manager not initialized"));
    }

    #[tokio::test]
    async fn mcp_init_with_empty_config_is_idempotent() {
        let state =
            DesktopState::with_executor(Arc::new(super::MockTurnExecutor), None, None, None);
        // Use a temp dir with no .claw/settings.json → empty config.
        let tmp = std::env::temp_dir().join(format!(
            "mcp-empty-{}-{}",
            std::process::id(),
            super::unix_timestamp_millis()
        ));
        let _ = fs::create_dir_all(&tmp);

        let tools_1 = state.ensure_mcp_initialized(&tmp).await;
        assert!(tools_1.is_empty(), "empty config → no tools");

        // Second call is idempotent (no re-init, returns cached).
        let tools_2 = state.ensure_mcp_initialized(&tmp).await;
        assert!(tools_2.is_empty());

        // mcp_call_tool should now fail with "unknown tool" instead of
        // "manager not initialized" because the manager is set (empty).
        let result = state
            .mcp_call_tool("mcp__foo__bar", serde_json::json!({}))
            .await;
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn startup_reconcile_resets_stuck_running_sessions() {
        // Simulates a crash scenario: persistence file contains a session
        // that was in turn_state=Running when the process died. On load,
        // the reconcile pass should reset it to Idle so the user can
        // continue sending messages.
        let path = legacy_sessions_fixture_path();
        let payload = r#"{
  "next_session_id": 10,
  "sessions": [
    {
      "metadata": {
        "id": "desktop-session-stuck",
        "title": "Was running",
        "preview": "Preview",
        "bucket": "today",
        "created_at": 1774960754306,
        "updated_at": 1774967954306,
        "project_name": "Test",
        "project_path": "/tmp/test",
        "environment_label": "Local",
        "model_label": "Opus 4.6",
        "turn_state": "running"
      },
      "session": {
        "version": 1,
        "messages": []
      }
    }
  ]
}"#;
        fs::write(&path, payload).expect("fixture should be written");

        let state = DesktopState::with_executor(
            Arc::new(super::MockTurnExecutor),
            Some(Arc::new(DesktopPersistence { path: path.clone() })),
            None,
            None,
        );

        let detail = state
            .get_session("desktop-session-stuck")
            .await
            .expect("stuck session should load");

        // The turn state should have been reset to Idle by the reconcile pass.
        assert_eq!(
            detail.turn_state,
            DesktopTurnState::Idle,
            "startup reconcile must reset Running → Idle"
        );

        let _ = fs::remove_file(path);
    }

    /// Performance benchmark: measures the cost of append/persist/list
    /// operations on a long session. Marked `#[ignore]` so it doesn't
    /// slow down the regular `cargo test` run. Invoke with:
    ///
    ///   cargo test -p desktop-core --release -- --ignored bench_long_session --nocapture
    #[tokio::test]
    #[ignore]
    async fn bench_long_session_append_and_persist() {
        use super::ConversationMessage;
        use std::time::Instant;

        let path = legacy_sessions_fixture_path();
        let state = DesktopState::with_executor(
            Arc::new(super::MockTurnExecutor),
            Some(Arc::new(DesktopPersistence { path: path.clone() })),
            None,
            None,
        );

        // Create one session.
        let session = state
            .create_session(super::CreateDesktopSessionRequest {
                title: Some("bench".into()),
                project_name: Some("bench-project".into()),
                project_path: Some("/tmp/bench".into()),
            })
            .await;
        let session_id = session.id;

        const MESSAGE_COUNT: usize = 200;
        let mut append_durations = Vec::with_capacity(MESSAGE_COUNT);
        let mut persist_durations = Vec::with_capacity(MESSAGE_COUNT);

        for i in 0..MESSAGE_COUNT {
            // Record the message append directly (bypass agentic loop
            // because it requires OAuth). Test raw store + persist path.
            let message = format!(
                "bench message {i} — lorem ipsum dolor sit amet consectetur adipiscing elit"
            );

            let t_append = Instant::now();
            {
                let mut store = state.store.write().await;
                if let Some(record) = store.sessions.get_mut(&session_id) {
                    record
                        .session
                        .push_message(ConversationMessage::user_text(message.clone()))
                        .ok();
                    record
                        .session
                        .push_message(ConversationMessage {
                            role: super::MessageRole::Assistant,
                            blocks: vec![super::ContentBlock::Text {
                                text: "mock reply".to_string(),
                            }],
                            usage: None,
                        })
                        .ok();
                    record.metadata.updated_at = super::unix_timestamp_millis();
                }
            }
            append_durations.push(t_append.elapsed());

            let t_persist = Instant::now();
            state.persist().await;
            persist_durations.push(t_persist.elapsed());
        }

        // Now measure list_sessions with 1 session of 400 messages.
        let t_list = Instant::now();
        let _listed = state.list_sessions().await;
        let list_duration = t_list.elapsed();

        // Measure get_session (full detail serialization).
        let t_get = Instant::now();
        let _ = state.get_session(&session_id).await;
        let get_duration = t_get.elapsed();

        // Print summary statistics.
        fn stats(name: &str, durs: &[std::time::Duration]) {
            let total: std::time::Duration = durs.iter().sum();
            let avg = total / durs.len() as u32;
            let max = durs.iter().max().copied().unwrap_or_default();
            let min = durs.iter().min().copied().unwrap_or_default();
            // Approximate p99 without sorting.
            let mut sorted = durs.to_vec();
            sorted.sort();
            let p50 = sorted[sorted.len() / 2];
            let p99 = sorted[sorted.len() * 99 / 100];
            println!(
                "{name:>10}: n={} total={:.2?} avg={:.2?} p50={:.2?} p99={:.2?} min={:.2?} max={:.2?}",
                durs.len(),
                total,
                avg,
                p50,
                p99,
                min,
                max
            );
        }

        println!("\n═══ Performance Benchmark (long session, {MESSAGE_COUNT} turns) ═══");
        stats("append", &append_durations);
        stats("persist", &persist_durations);
        println!("  list (400 messages): {:.2?}", list_duration);
        println!("  get  (400 messages): {:.2?}", get_duration);
        println!("═════════════════════════════════════════════════════════");

        let _ = fs::remove_file(path);
    }

    fn legacy_sessions_fixture_path() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let suffix = format!(
            "desktop-core-sessions-{}-{}-{}.json",
            std::process::id(),
            super::unix_timestamp_millis(),
            n
        );
        std::env::temp_dir().join(suffix)
    }

    // ── Regression: empty-session cleanup (2026-04 bug) ─────────────
    //
    // Pre-fix, `useAskSession` auto-created a session on every mount,
    // so route switches and strict-mode double invocations piled up
    // 8+ empty "Ask · new conversation" entries. The frontend fix is
    // to stop auto-creating; the backend fix is the cleanup helper
    // that removes leftovers produced before the patch was deployed.

    #[tokio::test]
    async fn cleanup_empty_sessions_removes_zero_message_idle_sessions() {
        let state = DesktopState::default();
        // Take a snapshot of sessions that existed before the test (the
        // default seed ships with a few non-empty records) so we can
        // separate them from the ones we create here.
        let baseline_ids: std::collections::HashSet<String> = state
            .list_sessions()
            .await
            .into_iter()
            .map(|s| s.id)
            .collect();

        // Create three empty sessions (simulating the regression).
        let mut empty_ids = Vec::new();
        for i in 0..3 {
            let summary = state
                .create_session(CreateDesktopSessionRequest {
                    title: Some(format!("Ask · new conversation {i}")),
                    project_name: None,
                    project_path: None,
                })
                .await;
            empty_ids.push(summary.id);
        }

        // Create one session with a real message — must survive cleanup.
        let kept = state
            .create_session(CreateDesktopSessionRequest {
                title: Some("real work".to_string()),
                project_name: None,
                project_path: None,
            })
            .await;
        state
            .append_user_message(&kept.id, "hello".to_string(), None)
            .await
            .expect("append should succeed");

        // Wait for the turn to finish so state is idle before cleanup.
        loop {
            let detail = state.get_session(&kept.id).await.unwrap();
            if detail.turn_state == DesktopTurnState::Idle {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        let deleted = state.cleanup_empty_sessions(None).await;

        // All three freshly-created empty ones got deleted …
        for id in &empty_ids {
            assert!(
                deleted.contains(id),
                "expected empty session {id} to be in deleted list"
            );
        }
        // … and the session with a real message survived.
        assert!(
            !deleted.contains(&kept.id),
            "session with messages must not be deleted"
        );
        let remaining: std::collections::HashSet<String> = state
            .list_sessions()
            .await
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert!(remaining.contains(&kept.id));
        for id in &empty_ids {
            assert!(
                !remaining.contains(id),
                "empty session {id} should no longer be listed"
            );
        }
        // Baseline (seeded) sessions that were non-empty should remain
        // untouched; baseline sessions that were empty (if any) will
        // also have been swept — that's expected behavior.
        for id in &baseline_ids {
            if !deleted.contains(id) {
                assert!(remaining.contains(id));
            }
        }
    }

    #[tokio::test]
    async fn cleanup_empty_sessions_honors_except_parameter() {
        let state = DesktopState::default();
        let a = state
            .create_session(CreateDesktopSessionRequest {
                title: None,
                project_name: None,
                project_path: None,
            })
            .await;
        let b = state
            .create_session(CreateDesktopSessionRequest {
                title: None,
                project_name: None,
                project_path: None,
            })
            .await;

        let deleted = state.cleanup_empty_sessions(Some(&a.id)).await;

        assert!(!deleted.contains(&a.id), "except id must be preserved");
        assert!(
            deleted.contains(&b.id),
            "other empty session should be deleted"
        );
        let remaining: std::collections::HashSet<String> = state
            .list_sessions()
            .await
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert!(remaining.contains(&a.id));
        assert!(!remaining.contains(&b.id));
    }

    #[tokio::test]
    async fn cleanup_empty_sessions_leaves_running_sessions_alone() {
        use super::DesktopTurnState as TS;

        let state = DesktopState::default();
        let running = state
            .create_session(CreateDesktopSessionRequest {
                title: None,
                project_name: None,
                project_path: None,
            })
            .await;

        // Force the session into Running state directly in the store,
        // mirroring what `append_user_message` does before dispatching
        // the turn executor.
        {
            let mut store = state.store.write().await;
            let record = store
                .sessions
                .get_mut(&running.id)
                .expect("session just created");
            record.metadata.turn_state = TS::Running;
        }

        let deleted = state.cleanup_empty_sessions(None).await;
        assert!(
            !deleted.contains(&running.id),
            "running session must not be swept even if empty"
        );
    }

    /// A2: bind_source + clear_source_binding + bind overwrite. Covers
    /// the three lifecycle transitions the frontend relies on:
    ///   * Initial state → source_binding is None.
    ///   * After bind → detail carries the binding; persistent on
    ///     subsequent read-only `get_session` calls.
    ///   * Second bind → the prior binding is replaced (no stack).
    ///   * Clear → detail reverts to source_binding: None.
    ///   * Unknown session → SessionNotFound.
    #[tokio::test]
    async fn bind_clear_source_binding_round_trip() {
        use super::{
            CreateDesktopSessionRequest, DesktopState, DesktopStateError, MockTurnExecutor,
            SessionSourceBinding, SourceRef,
        };

        let state = DesktopState::with_executor(Arc::new(MockTurnExecutor), None, None, None);
        let created = state
            .create_session(CreateDesktopSessionRequest {
                title: Some("bind test".into()),
                project_name: None,
                project_path: None,
            })
            .await;
        assert!(created.source_binding.is_none(), "fresh session unbound");

        // Bind a raw source.
        let bound = state
            .bind_source(
                &created.id,
                SourceRef::Raw {
                    id: 7,
                    title: "First Raw".into(),
                },
                Some("test".into()),
            )
            .await
            .expect("bind should succeed");
        let binding = bound
            .source_binding
            .as_ref()
            .expect("binding populated after bind");
        assert!(matches!(binding.source, SourceRef::Raw { id: 7, .. }));
        assert_eq!(binding.binding_reason.as_deref(), Some("test"));

        // get_session reflects the binding — persistence check.
        let detail = state.get_session(&created.id).await.expect("get works");
        let persisted = detail.source_binding.as_ref().expect("persisted binding");
        assert!(matches!(persisted.source, SourceRef::Raw { id: 7, .. }));

        // Second bind replaces the first — no stack of bindings.
        let rebound = state
            .bind_source(
                &created.id,
                SourceRef::Wiki {
                    slug: "foo".into(),
                    title: "Foo".into(),
                },
                None,
            )
            .await
            .expect("rebind should succeed");
        let b2 = rebound
            .source_binding
            .as_ref()
            .expect("binding after rebind");
        assert!(matches!(b2.source, SourceRef::Wiki { .. }));
        assert_eq!(b2.binding_reason, None);

        // Clear drops the binding entirely.
        let cleared = state
            .clear_source_binding(&created.id)
            .await
            .expect("clear should succeed");
        assert!(cleared.source_binding.is_none());

        // Idempotent: clearing an already-unbound session still succeeds.
        let cleared_again = state
            .clear_source_binding(&created.id)
            .await
            .expect("clear is idempotent");
        assert!(cleared_again.source_binding.is_none());

        // Unknown session → SessionNotFound.
        let err = state
            .bind_source(
                "nonexistent",
                SourceRef::Raw {
                    id: 1,
                    title: "t".into(),
                },
                None,
            )
            .await;
        assert!(matches!(err, Err(DesktopStateError::SessionNotFound(_))));

        let err = state.clear_source_binding("nonexistent").await;
        assert!(matches!(err, Err(DesktopStateError::SessionNotFound(_))));

        // Final sanity: SessionSourceBinding is the type actually returned.
        let b: SessionSourceBinding = SessionSourceBinding::new(
            SourceRef::Inbox {
                id: 1,
                title: "t".into(),
            },
            None,
        );
        assert!(b.bound_at > 0, "bound_at stamped");
    }

    // ── Sprint 1-B.1 · step 3: DesktopSessionEvent::AbsorbProgress + AbsorbComplete
    //
    // These tests pin the wire shape of the two new SSE variants
    // against the `technical-design.md §2.1` examples. Any serde-level
    // drift (e.g. accidentally dropping `rename_all = "snake_case"` on
    // the enum) breaks the frontend event dispatcher silently, so we
    // assert the exact JSON.

    #[test]
    fn absorb_progress_serializes_with_flat_fields_and_snake_case_type() {
        use super::DesktopSessionEvent;
        let ev = DesktopSessionEvent::AbsorbProgress(wiki_maintainer::AbsorbProgressEvent {
            task_id: "absorb-1713072000-a3f2".to_string(),
            processed: 2,
            total: 5,
            current_entry_id: 3,
            action: "create".to_string(),
            page_slug: Some("transformer-architecture".to_string()),
            page_title: Some("Transformer 架构".to_string()),
            error: None,
        });
        let json = serde_json::to_value(&ev).unwrap();
        // The `#[serde(tag = "type", rename_all = "snake_case")]` on
        // the enum + newtype-variant flattening should produce a flat
        // map: all inner fields sit next to the "type" tag.
        assert_eq!(json["type"], "absorb_progress");
        assert_eq!(json["task_id"], "absorb-1713072000-a3f2");
        assert_eq!(json["processed"], 2);
        assert_eq!(json["total"], 5);
        assert_eq!(json["current_entry_id"], 3);
        assert_eq!(json["action"], "create");
        assert_eq!(json["page_slug"], "transformer-architecture");
        assert_eq!(json["page_title"], "Transformer 架构");
        assert!(json["error"].is_null());
    }

    #[test]
    fn absorb_complete_serializes_all_six_counters() {
        use super::DesktopSessionEvent;
        let ev = DesktopSessionEvent::AbsorbComplete {
            task_id: "absorb-1713072000-a3f2".to_string(),
            created: 3,
            updated: 1,
            skipped: 1,
            failed: 0,
            duration_ms: 12_500,
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "absorb_complete");
        assert_eq!(json["task_id"], "absorb-1713072000-a3f2");
        assert_eq!(json["created"], 3);
        assert_eq!(json["updated"], 1);
        assert_eq!(json["skipped"], 1);
        assert_eq!(json["failed"], 0);
        assert_eq!(json["duration_ms"], 12_500);
    }

    #[test]
    fn absorb_progress_round_trip_via_serde_json() {
        use super::DesktopSessionEvent;
        let original = DesktopSessionEvent::AbsorbProgress(wiki_maintainer::AbsorbProgressEvent {
            task_id: "absorb-roundtrip".to_string(),
            processed: 0,
            total: 0,
            current_entry_id: 0,
            action: "skip".to_string(),
            page_slug: None,
            page_title: None,
            error: Some("无法读取 raw entry: EOF".to_string()),
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: DesktopSessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn absorb_complete_round_trip_via_serde_json() {
        use super::DesktopSessionEvent;
        let original = DesktopSessionEvent::AbsorbComplete {
            task_id: "absorb-xyz".to_string(),
            created: 0,
            updated: 0,
            skipped: 0,
            failed: 0,
            duration_ms: 0,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: DesktopSessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn event_name_returns_canonical_snake_case_for_new_variants() {
        use super::DesktopSessionEvent;
        let progress = DesktopSessionEvent::AbsorbProgress(wiki_maintainer::AbsorbProgressEvent {
            task_id: "absorb-event-name".to_string(),
            processed: 0,
            total: 0,
            current_entry_id: 0,
            action: "create".to_string(),
            page_slug: None,
            page_title: None,
            error: None,
        });
        let complete = DesktopSessionEvent::AbsorbComplete {
            task_id: "absorb-1-a".to_string(),
            created: 0,
            updated: 0,
            skipped: 0,
            failed: 0,
            duration_ms: 0,
        };
        assert_eq!(progress.event_name(), "absorb_progress");
        assert_eq!(complete.event_name(), "absorb_complete");
    }

    #[tokio::test]
    async fn skill_event_subscriber_receives_absorb_events_without_session() {
        use super::{DesktopSessionEvent, DesktopState};

        let state = DesktopState::default();
        let mut rx = state.subscribe_skill_events();
        let event = DesktopSessionEvent::AbsorbComplete {
            task_id: "absorb-global-sse".to_string(),
            created: 1,
            updated: 2,
            skipped: 3,
            failed: 0,
            duration_ms: 42,
        };

        state.broadcast_session_event(event.clone()).await;

        let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("skill event should arrive")
            .expect("skill event channel should stay open");
        assert_eq!(received, event);
    }
}

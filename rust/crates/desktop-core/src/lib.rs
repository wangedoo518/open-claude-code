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
    read_xai_base_url, resolve_model_alias, resolve_startup_auth_source, AuthSource,
    ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest, MessageResponse,
    OutputContentBlock, ProviderClient, ProviderKind, StreamEvent as ApiStreamEvent, ToolChoice,
    ToolResultContentBlock,
};
use plugins::{PluginManager, PluginManagerConfig};
use runtime::{
    credentials_path, load_system_prompt, ApiClient as RuntimeApiClient, ApiRequest,
    AssistantEvent, ConfigLoader, ConfigSource, ContentBlock, ConversationMessage,
    ConversationRuntime, McpServerConfig, MessageRole, PermissionMode, PermissionPolicy,
    ResolvedPermissionMode, RuntimeConfig, RuntimeError, RuntimeFeatureConfig,
    Session as RuntimeSession, SessionCompaction as RuntimeSessionCompaction,
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
mod codex_auth;
mod managed_auth;
mod oauth_runtime;

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
const DEFAULT_PROJECT_PATH: &str = "/Users/champion/Documents/develop/Warwolf/open-claude-code";
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
    pub session: DesktopSessionData,
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
}

impl DesktopSessionEvent {
    #[must_use]
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::Snapshot { .. } => "snapshot",
            Self::Message { .. } => "message",
            Self::PermissionRequest { .. } => "permission_request",
            Self::TextDelta { .. } => "text_delta",
        }
    }
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

#[derive(Debug, Clone)]
struct DesktopTurnRequest {
    message: String,
    project_path: String,
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
        let sessions = seeded
            .into_iter()
            .map(|record| (record.metadata.id.clone(), record))
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
                .unwrap_or_else(|| DEFAULT_PROJECT_PATH.to_string())
        };

        tokio::task::spawn_blocking(move || build_customize_state(project_path))
            .await
            .unwrap_or_else(|error| {
                DesktopCustomizeState::empty_with_warning(
                    DEFAULT_PROJECT_PATH.to_string(),
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
                .unwrap_or_else(|| DEFAULT_PROJECT_PATH.to_string())
        };

        tokio::task::spawn_blocking(move || build_settings_state(project_path))
            .await
            .unwrap_or_else(|error| DesktopSettingsState {
                project_path: DEFAULT_PROJECT_PATH.to_string(),
                config_home: ConfigLoader::default_for(DEFAULT_PROJECT_PATH)
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
                    .unwrap_or_else(|| DEFAULT_PROJECT_PATH.to_string()),
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
                    .unwrap_or_else(|| DEFAULT_PROJECT_PATH.to_string()),
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

    pub async fn delete_session(
        &self,
        session_id: &str,
    ) -> Result<bool, DesktopStateError> {
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

    pub async fn cancel_session(
        &self,
        session_id: &str,
    ) -> Result<(), DesktopStateError> {
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

    pub async fn delete_scheduled_task(
        &self,
        task_id: &str,
    ) -> Result<bool, DesktopStateError> {
        let removed = self
            .scheduled_store
            .write()
            .await
            .tasks
            .remove(task_id)
            .is_some();
        if !removed {
            return Err(DesktopStateError::ScheduledTaskNotFound(task_id.to_string()));
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

    pub async fn delete_dispatch_item(
        &self,
        item_id: &str,
    ) -> Result<bool, DesktopStateError> {
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
            .unwrap_or_else(|| DEFAULT_PROJECT_PATH.to_string());

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

    pub async fn append_user_message(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<DesktopSessionDetail, DesktopStateError> {
        let user_message = ConversationMessage::user_text(message.clone());
        let session_id = session_id.to_string();

        let (detail, sender, session, previous_message_count, project_path) = {
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
            if record.metadata.title == "New session" {
                record.metadata.title = session_title_from_message(&message);
            }
            record.session.messages.push(user_message.clone());

            (
                record.detail(),
                record.events.clone(),
                session,
                previous_message_count,
                project_path,
            )
        };

        self.persist().await;
        let _ = sender.send(DesktopSessionEvent::Snapshot {
            session: detail.clone(),
        });

        let state = self.clone();
        let turn_executor = Arc::clone(&self.turn_executor);
        tokio::spawn(async move {
            state
                .run_background_turn(
                    session_id,
                    session,
                    previous_message_count,
                    DesktopTurnRequest {
                        message: message.clone(),
                        project_path,
                    },
                    turn_executor,
                )
                .await;
        });

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
            match self
                .append_user_message(session_id, task.prompt.clone())
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
                .append_user_message(&session.id, task.prompt.clone())
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

        self.append_user_message(&target_session_id, item.body.clone())
            .await?;
        Ok(target_session_id)
    }

    async fn session_context(&self) -> DesktopSessionContext {
        let store = self.store.read().await;
        let mut trusted_project_paths = BTreeSet::from([DEFAULT_PROJECT_PATH.to_string()]);
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
            .unwrap_or_else(|| DEFAULT_PROJECT_PATH.to_string())
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
            session: DesktopSessionData::from(&self.session),
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
        ConfigLoader::default_for(DEFAULT_PROJECT_PATH)
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
        ConfigLoader::default_for(DEFAULT_PROJECT_PATH)
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
        ConfigLoader::default_for(DEFAULT_PROJECT_PATH)
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

    let resolved_model = resolve_model_alias(runtime_config.model().unwrap_or(DEFAULT_MODEL_ID));
    let model_label = humanize_model_label(&resolved_model);
    let system_prompt = match load_system_prompt(
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

    let default_auth = match default_auth_source(&resolved_model, &runtime_config) {
        Ok(auth) => auth,
        Err(error) => {
            return fallback_turn_result(
                session,
                &request.message,
                model_label.clone(),
                format!("failed to resolve model authentication: {error}"),
            )
        }
    };

    let api_client = match DesktopRuntimeClient::new(
        resolved_model.to_string(),
        default_auth,
        tool_registry.clone(),
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
    fn new(
        model: String,
        default_auth: Option<AuthSource>,
        tool_registry: GlobalToolRegistry,
    ) -> Result<Self, String> {
        Ok(Self {
            runtime: tokio::runtime::Runtime::new().map_err(|error| error.to_string())?,
            client: ProviderClient::from_model_with_anthropic_auth(&model, default_auth)
                .map_err(|error| error.to_string())?,
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

    for (index, block) in response.content.into_iter().enumerate() {
        let index = u32::try_from(index).expect("response block index overflow");
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

fn process_workspace_lock() -> &'static StdMutex<()> {
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
            project_path: DEFAULT_PROJECT_PATH.to_string(),
            environment_label: DEFAULT_ENVIRONMENT_LABEL.to_string(),
            model_label: DEFAULT_MODEL_LABEL.to_string(),
            turn_state: DesktopTurnState::Idle,
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
        DesktopDispatchStatus, DesktopPersistence, DesktopScheduledRunStatus,
        DesktopScheduledSchedule, DesktopState, DesktopTurnState,
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
                }
                .message,
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

    fn legacy_sessions_fixture_path() -> PathBuf {
        let suffix = format!(
            "desktop-core-legacy-sessions-{}-{}.json",
            std::process::id(),
            super::unix_timestamp_millis()
        );
        std::env::temp_dir().join(suffix)
    }
}

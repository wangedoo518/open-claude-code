use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::codex_auth::{
    self, DesktopCodexAuthSource, DesktopCodexLoginSessionSnapshot, DesktopCodexLoginSessionStatus,
    DesktopCodexProfileSummary,
};
use crate::oauth_runtime::{self, DesktopProviderModel};

pub const CODEX_OPENAI_AUTH_PROVIDER_ID: &str = "codex-openai";
pub const QWEN_CODE_AUTH_PROVIDER_ID: &str = "qwen-code";

const QWEN_STORE_DIR: &str = ".warwolf";
const QWEN_STORE_FILE: &str = "profiles.json";
const QWEN_PROFILE_KIND_DEVICE_CODE: &str = "device_code";
const QWEN_PROFILE_KIND_IMPORTED: &str = "imported";
const QWEN_OAUTH_BASE_URL: &str = "https://chat.qwen.ai";
const QWEN_RUNTIME_BASE_URL: &str = "https://portal.qwen.ai";
const QWEN_OAUTH_DEVICE_CODE_ENDPOINT: &str = "https://chat.qwen.ai/api/v1/oauth2/device/code";
const QWEN_OAUTH_TOKEN_ENDPOINT: &str = "https://chat.qwen.ai/api/v1/oauth2/token";
const QWEN_OAUTH_CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";
const QWEN_OAUTH_SCOPE: &str = "openid profile email model.completion";
const QWEN_OAUTH_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const QWEN_RUNTIME_DIR: &str = ".qwen";
const QWEN_RUNTIME_CREDS_FILE: &str = "oauth_creds.json";
const QWEN_RUNTIME_SETTINGS_FILE: &str = "settings.json";
const QWEN_DEFAULT_MODEL_ID: &str = "coder-model";
const EXPIRING_BUFFER_SECONDS: i64 = 300;

static QWEN_LOGIN_SESSIONS: LazyLock<Mutex<HashMap<String, Arc<QwenLoginSessionRuntime>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopManagedAuthProviderKind {
    CodexOpenai,
    QwenCode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopManagedAuthSource {
    ImportedAuthJson,
    BrowserLogin,
    DeviceCode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopManagedAuthAccountStatus {
    Ready,
    Expiring,
    Expired,
    NeedsReauth,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopManagedAuthLoginSessionStatus {
    Pending,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthRuntimeBinding {
    pub runtime_name: String,
    pub auth_path: Option<String>,
    pub config_path: Option<String>,
    pub synced: bool,
    pub synced_account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthRuntimeClient {
    pub provider_id: String,
    pub provider_kind: DesktopManagedAuthProviderKind,
    pub base_url: String,
    pub bearer_token: String,
    pub extra_headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCodeToolLaunchProfile {
    pub environment_variables: HashMap<String, String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthProvider {
    pub id: String,
    pub name: String,
    pub kind: DesktopManagedAuthProviderKind,
    pub website_url: Option<String>,
    pub description: Option<String>,
    pub models: Vec<DesktopProviderModel>,
    pub default_model_id: Option<String>,
    pub account_count: usize,
    pub default_account_id: Option<String>,
    pub default_account_label: Option<String>,
    pub runtime: DesktopManagedAuthRuntimeBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthAccount {
    pub id: String,
    pub provider_id: String,
    pub email: Option<String>,
    pub subject: Option<String>,
    pub display_label: String,
    pub plan_label: Option<String>,
    pub auth_source: DesktopManagedAuthSource,
    pub status: DesktopManagedAuthAccountStatus,
    pub is_default: bool,
    pub applied_to_runtime: bool,
    pub created_at_epoch: i64,
    pub updated_at_epoch: i64,
    pub last_refresh_epoch: Option<i64>,
    pub access_token_expires_at_epoch: Option<i64>,
    pub resource_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthLoginSessionSnapshot {
    pub session_id: String,
    pub provider_id: String,
    pub status: DesktopManagedAuthLoginSessionStatus,
    pub authorize_url: Option<String>,
    pub verification_uri: Option<String>,
    pub verification_uri_complete: Option<String>,
    pub user_code: Option<String>,
    pub redirect_uri: Option<String>,
    pub error: Option<String>,
    pub account: Option<DesktopManagedAuthAccount>,
    pub created_at_epoch: i64,
    pub updated_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredQwenProfileStore {
    version: u8,
    active_profile_id: Option<String>,
    #[serde(default)]
    profiles: Vec<StoredQwenProfile>,
    updated_at_epoch: i64,
}

impl Default for StoredQwenProfileStore {
    fn default() -> Self {
        Self {
            version: 1,
            active_profile_id: None,
            profiles: Vec::new(),
            updated_at_epoch: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredQwenProfile {
    id: String,
    display_label: String,
    email: Option<String>,
    subject: Option<String>,
    plan_label: Option<String>,
    auth_kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    access_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_url: Option<String>,
    last_refresh_epoch: Option<i64>,
    access_token_expires_at_epoch: Option<i64>,
    created_at_epoch: i64,
    updated_at_epoch: i64,
}

#[derive(Debug)]
struct QwenLoginSessionRuntime {
    session_id: String,
    device_code: String,
    code_verifier: String,
    verification_uri: String,
    verification_uri_complete: String,
    user_code: String,
    expires_at_epoch: i64,
    state: Mutex<QwenLoginSessionState>,
}

#[derive(Debug, Clone)]
struct QwenLoginSessionState {
    status: DesktopManagedAuthLoginSessionStatus,
    error: Option<String>,
    profile: Option<StoredQwenProfile>,
    created_at_epoch: i64,
    updated_at_epoch: i64,
}

#[derive(Debug, Deserialize)]
struct QwenDeviceAuthorizationSuccess {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct QwenErrorResponse {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QwenTokenSuccess {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    expires_in: i64,
    #[serde(default)]
    resource_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QwenRuntimeCredentials {
    access_token: String,
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
    expiry_date: i64,
    token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QwenRuntimeSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    security: Option<QwenRuntimeSecurity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<QwenRuntimeModelSettings>,
    #[serde(flatten)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QwenRuntimeSecurity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<QwenRuntimeAuthSettings>,
    #[serde(flatten)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QwenRuntimeAuthSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_type: Option<String>,
    #[serde(flatten)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QwenRuntimeModelSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(flatten)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedAuthProviderId {
    CodexOpenai,
    QwenCode,
}

impl ManagedAuthProviderId {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            CODEX_OPENAI_AUTH_PROVIDER_ID => Ok(Self::CodexOpenai),
            QWEN_CODE_AUTH_PROVIDER_ID => Ok(Self::QwenCode),
            _ => Err(format!("managed auth provider not found: {value}")),
        }
    }
}

pub(crate) fn list_providers() -> Result<Vec<DesktopManagedAuthProvider>, String> {
    Ok(vec![codex_provider_state()?, qwen_provider_state()?])
}

pub(crate) fn provider_state(provider_id: &str) -> Result<DesktopManagedAuthProvider, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => codex_provider_state(),
        ManagedAuthProviderId::QwenCode => qwen_provider_state(),
    }
}

pub(crate) fn list_accounts(provider_id: &str) -> Result<Vec<DesktopManagedAuthAccount>, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => codex_accounts(),
        ManagedAuthProviderId::QwenCode => qwen_accounts(),
    }
}

pub(crate) fn import_accounts(provider_id: &str) -> Result<Vec<DesktopManagedAuthAccount>, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => {
            codex_auth::profile_import(None)?;
            codex_accounts()
        }
        ManagedAuthProviderId::QwenCode => {
            Err("Qwen Code does not support importing runtime auth files".to_string())
        }
    }
}

pub(crate) async fn begin_login(
    provider_id: &str,
) -> Result<DesktopManagedAuthLoginSessionSnapshot, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => {
            let session = codex_auth::login_begin(None).await?;
            Ok(adapt_codex_login_session(session))
        }
        ManagedAuthProviderId::QwenCode => begin_qwen_login().await,
    }
}

pub(crate) async fn poll_login(
    provider_id: &str,
    session_id: &str,
) -> Result<DesktopManagedAuthLoginSessionSnapshot, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => {
            let session = codex_auth::login_poll(session_id.to_string()).await?;
            Ok(adapt_codex_login_session(session))
        }
        ManagedAuthProviderId::QwenCode => poll_qwen_login(session_id).await,
    }
}

pub(crate) fn set_default_account(
    provider_id: &str,
    account_id: &str,
) -> Result<Vec<DesktopManagedAuthAccount>, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => {
            codex_auth::profile_set_active(account_id.to_string(), None)?;
            codex_accounts()
        }
        ManagedAuthProviderId::QwenCode => {
            let mut store = load_qwen_store()?;
            let profile = find_qwen_profile(&store, account_id)?;
            for item in &mut store.profiles {
                if item.id == profile.id {
                    item.updated_at_epoch = now_unix_i64();
                }
            }
            store.active_profile_id = Some(profile.id.clone());
            let model_id = current_qwen_runtime_model_id()
                .unwrap_or_else(|| QWEN_DEFAULT_MODEL_ID.to_string());
            let store = save_qwen_store(store)?;
            write_qwen_profile_to_runtime(&profile, &model_id)?;
            qwen_accounts_from_store(&store)
        }
    }
}

pub(crate) async fn refresh_account(
    provider_id: &str,
    account_id: &str,
) -> Result<Vec<DesktopManagedAuthAccount>, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => {
            codex_auth::profile_refresh(account_id.to_string(), None).await?;
            codex_accounts()
        }
        ManagedAuthProviderId::QwenCode => {
            let mut store = load_qwen_store()?;
            let profile = find_qwen_profile(&store, account_id)?;
            let refreshed = refresh_qwen_profile(profile).await?;
            let should_apply = store.active_profile_id.as_deref() == Some(refreshed.id.as_str());
            upsert_qwen_profile(&mut store, refreshed.clone(), should_apply);
            let store = save_qwen_store(store)?;
            if should_apply {
                let model_id = current_qwen_runtime_model_id()
                    .unwrap_or_else(|| QWEN_DEFAULT_MODEL_ID.to_string());
                write_qwen_profile_to_runtime(&refreshed, &model_id)?;
            }
            qwen_accounts_from_store(&store)
        }
    }
}

pub(crate) fn remove_account(
    provider_id: &str,
    account_id: &str,
) -> Result<Vec<DesktopManagedAuthAccount>, String> {
    match ManagedAuthProviderId::parse(provider_id)? {
        ManagedAuthProviderId::CodexOpenai => {
            codex_auth::profile_remove(account_id.to_string(), None)?;
            codex_accounts()
        }
        ManagedAuthProviderId::QwenCode => {
            let mut store = load_qwen_store()?;
            let was_default = store.active_profile_id.as_deref() == Some(account_id);
            store.profiles.retain(|item| item.id != account_id);
            if was_default {
                store.active_profile_id = store.profiles.first().map(|item| item.id.clone());
            }
            let store = save_qwen_store(store)?;
            if let Some(active_id) = &store.active_profile_id {
                let active_profile = find_qwen_profile(&store, active_id)?;
                let model_id = current_qwen_runtime_model_id()
                    .unwrap_or_else(|| QWEN_DEFAULT_MODEL_ID.to_string());
                write_qwen_profile_to_runtime(&active_profile, &model_id)?;
            } else {
                clear_qwen_runtime_creds()?;
            }
            qwen_accounts_from_store(&store)
        }
    }
}

pub(crate) fn build_code_tool_launch_profile(
    cli_tool: &str,
    provider_id: &str,
    model_id: &str,
    desktop_api_base: &str,
) -> Result<DesktopCodeToolLaunchProfile, String> {
    match (cli_tool, provider_id) {
        ("claude-code", CODEX_OPENAI_AUTH_PROVIDER_ID | QWEN_CODE_AUTH_PROVIDER_ID) => {
            let mut environment_variables = HashMap::new();
            let base = desktop_api_base.trim_end_matches('/');
            let bridge_base = format!("{base}/api/desktop/code-tools/claude-bridge/{provider_id}");
            environment_variables.insert("ANTHROPIC_BASE_URL".to_string(), bridge_base);
            environment_variables.insert(
                "ANTHROPIC_AUTH_TOKEN".to_string(),
                "warwolf-managed-auth".to_string(),
            );
            environment_variables.insert(
                "ANTHROPIC_API_KEY".to_string(),
                "warwolf-managed-auth".to_string(),
            );
            Ok(DesktopCodeToolLaunchProfile {
                environment_variables,
                message: None,
            })
        }
        ("openai-codex", CODEX_OPENAI_AUTH_PROVIDER_ID) => {
            let projection_home = codex_auth::prepare_code_tool_home(model_id)?;
            let mut environment_variables = HashMap::new();
            environment_variables.insert(
                "CODEX_HOME".to_string(),
                projection_home.display().to_string(),
            );
            Ok(DesktopCodeToolLaunchProfile {
                environment_variables,
                message: None,
            })
        }
        ("openai-codex", QWEN_CODE_AUTH_PROVIDER_ID) => Err(
            "OpenAI Codex 当前仅支持 Codex OAuth 模型服务，请改用 Codex OAuth 模型。".to_string(),
        ),
        _ => Err(format!(
            "Unsupported code tool launch profile: {cli_tool}/{provider_id}"
        )),
    }
}

pub(crate) fn runtime_client(provider_id: &str) -> Result<DesktopManagedAuthRuntimeClient, String> {
    match provider_id {
        CODEX_OPENAI_AUTH_PROVIDER_ID => {
            let tokens = codex_auth::resolve_default_runtime_tokens()?;
            Ok(DesktopManagedAuthRuntimeClient {
                provider_id: provider_id.to_string(),
                provider_kind: DesktopManagedAuthProviderKind::CodexOpenai,
                base_url: "https://chatgpt.com/backend-api/codex".to_string(),
                bearer_token: tokens.access_token,
                extra_headers: HashMap::new(),
            })
        }
        QWEN_CODE_AUTH_PROVIDER_ID => {
            let store = load_qwen_store()?;
            let profile = default_qwen_profile_with_runtime_fallback(&store)?;
            if profile.access_token.trim().is_empty() {
                return Err("Qwen OAuth 默认账号缺少 access token".to_string());
            }
            let mut extra_headers = HashMap::new();
            let user_agent = qwen_cli_user_agent();
            extra_headers.insert("User-Agent".to_string(), user_agent.clone());
            extra_headers.insert("X-DashScope-CacheControl".to_string(), "enable".to_string());
            extra_headers.insert("X-DashScope-UserAgent".to_string(), user_agent);
            extra_headers.insert("X-DashScope-AuthType".to_string(), "qwen-oauth".to_string());
            Ok(DesktopManagedAuthRuntimeClient {
                provider_id: provider_id.to_string(),
                provider_kind: DesktopManagedAuthProviderKind::QwenCode,
                base_url: normalized_qwen_runtime_base_url(profile.resource_url.as_deref()),
                bearer_token: profile.access_token,
                extra_headers,
            })
        }
        _ => Err(format!("Unsupported managed auth provider: {provider_id}")),
    }
}

fn codex_provider_state() -> Result<DesktopManagedAuthProvider, String> {
    let accounts = codex_accounts()?;
    let runtime = oauth_runtime::codex_runtime_state()?;
    let default_account = accounts.iter().find(|item| item.is_default);
    let synced_account = accounts.iter().find(|item| item.applied_to_runtime);

    Ok(DesktopManagedAuthProvider {
        id: CODEX_OPENAI_AUTH_PROVIDER_ID.to_string(),
        name: "OpenAI".to_string(),
        kind: DesktopManagedAuthProviderKind::CodexOpenai,
        website_url: Some("https://platform.openai.com".to_string()),
        description: Some(
            "OpenAI 官方服务，仅支持通过 Codex OAuth 登录态同步到 ~/.codex 配置。".to_string(),
        ),
        models: oauth_runtime::codex_oauth_models(),
        default_model_id: runtime
            .model
            .or_else(|| default_model_id_from_catalog(&accounts, &[]))
            .or_else(|| Some("gpt-5.4".to_string())),
        account_count: accounts.len(),
        default_account_id: default_account.map(|item| item.id.clone()),
        default_account_label: default_account.map(|item| item.display_label.clone()),
        runtime: DesktopManagedAuthRuntimeBinding {
            runtime_name: "Codex".to_string(),
            auth_path: Some(runtime.auth_path),
            config_path: Some(runtime.config_path),
            synced: synced_account.is_some(),
            synced_account_id: synced_account.map(|item| item.id.clone()),
        },
    })
}

fn codex_accounts() -> Result<Vec<DesktopManagedAuthAccount>, String> {
    let overview = codex_auth::overview_get(None)?;
    let runtime = oauth_runtime::codex_runtime_state()?;
    Ok(overview
        .profiles
        .into_iter()
        .map(|profile| adapt_codex_account(profile, &runtime))
        .collect())
}

fn adapt_codex_account(
    profile: DesktopCodexProfileSummary,
    _runtime: &oauth_runtime::DesktopCodexRuntimeState,
) -> DesktopManagedAuthAccount {
    DesktopManagedAuthAccount {
        id: profile.id,
        provider_id: CODEX_OPENAI_AUTH_PROVIDER_ID.to_string(),
        email: Some(profile.email.clone()),
        subject: profile.chatgpt_user_id.clone(),
        display_label: profile.display_label,
        plan_label: profile.chatgpt_plan_type.clone(),
        auth_source: match profile.auth_source {
            DesktopCodexAuthSource::ImportedAuthJson => DesktopManagedAuthSource::ImportedAuthJson,
            DesktopCodexAuthSource::BrowserLogin => DesktopManagedAuthSource::BrowserLogin,
        },
        status: status_from_expiry(profile.access_token_expires_at_epoch),
        is_default: profile.active,
        applied_to_runtime: profile.applied_to_codex,
        created_at_epoch: profile.updated_at_epoch,
        updated_at_epoch: profile.updated_at_epoch,
        last_refresh_epoch: profile.last_refresh_epoch,
        access_token_expires_at_epoch: profile.access_token_expires_at_epoch,
        resource_url: None,
    }
}

fn adapt_codex_login_session(
    session: DesktopCodexLoginSessionSnapshot,
) -> DesktopManagedAuthLoginSessionSnapshot {
    DesktopManagedAuthLoginSessionSnapshot {
        session_id: session.session_id,
        provider_id: CODEX_OPENAI_AUTH_PROVIDER_ID.to_string(),
        status: match session.status {
            DesktopCodexLoginSessionStatus::Pending => {
                DesktopManagedAuthLoginSessionStatus::Pending
            }
            DesktopCodexLoginSessionStatus::Completed => {
                DesktopManagedAuthLoginSessionStatus::Completed
            }
            DesktopCodexLoginSessionStatus::Failed => DesktopManagedAuthLoginSessionStatus::Failed,
            DesktopCodexLoginSessionStatus::Cancelled => {
                DesktopManagedAuthLoginSessionStatus::Cancelled
            }
        },
        authorize_url: Some(session.authorize_url),
        verification_uri: None,
        verification_uri_complete: None,
        user_code: None,
        redirect_uri: Some(session.redirect_uri),
        error: session.error,
        account: session.profile.map(|profile| DesktopManagedAuthAccount {
            id: profile.id,
            provider_id: CODEX_OPENAI_AUTH_PROVIDER_ID.to_string(),
            email: Some(profile.email.clone()),
            subject: profile.chatgpt_user_id.clone(),
            display_label: profile.display_label,
            plan_label: profile.chatgpt_plan_type,
            auth_source: match profile.auth_source {
                DesktopCodexAuthSource::ImportedAuthJson => {
                    DesktopManagedAuthSource::ImportedAuthJson
                }
                DesktopCodexAuthSource::BrowserLogin => DesktopManagedAuthSource::BrowserLogin,
            },
            status: status_from_expiry(profile.access_token_expires_at_epoch),
            is_default: profile.active,
            applied_to_runtime: profile.applied_to_codex,
            created_at_epoch: profile.updated_at_epoch,
            updated_at_epoch: profile.updated_at_epoch,
            last_refresh_epoch: profile.last_refresh_epoch,
            access_token_expires_at_epoch: profile.access_token_expires_at_epoch,
            resource_url: None,
        }),
        created_at_epoch: session.created_at_epoch,
        updated_at_epoch: session.updated_at_epoch,
    }
}

fn qwen_provider_state() -> Result<DesktopManagedAuthProvider, String> {
    let accounts = qwen_accounts()?;
    let default_account = accounts.iter().find(|item| item.is_default);
    let synced_account = accounts.iter().find(|item| item.applied_to_runtime);

    Ok(DesktopManagedAuthProvider {
        id: QWEN_CODE_AUTH_PROVIDER_ID.to_string(),
        name: "Qwen Code".to_string(),
        kind: DesktopManagedAuthProviderKind::QwenCode,
        website_url: Some(QWEN_OAUTH_BASE_URL.to_string()),
        description: Some(
            "Qwen 官方浏览器 OAuth 登录，可在本机托管多个账号并写入 ~/.qwen。".to_string(),
        ),
        models: qwen_oauth_models(),
        default_model_id: current_qwen_runtime_model_id()
            .or_else(|| Some(QWEN_DEFAULT_MODEL_ID.to_string())),
        account_count: accounts.len(),
        default_account_id: default_account.map(|item| item.id.clone()),
        default_account_label: default_account.map(|item| item.display_label.clone()),
        runtime: DesktopManagedAuthRuntimeBinding {
            runtime_name: "Qwen Code".to_string(),
            auth_path: Some(qwen_runtime_creds_path().display().to_string()),
            config_path: Some(qwen_runtime_settings_path().display().to_string()),
            synced: synced_account.is_some(),
            synced_account_id: synced_account.map(|item| item.id.clone()),
        },
    })
}

fn qwen_accounts() -> Result<Vec<DesktopManagedAuthAccount>, String> {
    let store = load_qwen_store()?;
    qwen_accounts_from_store(&store)
}

fn default_qwen_profile(store: &StoredQwenProfileStore) -> Result<StoredQwenProfile, String> {
    if let Some(account_id) = store.active_profile_id.as_deref() {
        return find_qwen_profile(store, account_id);
    }
    store
        .profiles
        .first()
        .cloned()
        .ok_or_else(|| "Qwen Code 尚未连接任何账号".to_string())
}

fn default_qwen_profile_with_runtime_fallback(
    store: &StoredQwenProfileStore,
) -> Result<StoredQwenProfile, String> {
    let mut profile = default_qwen_profile(store)?;
    let runtime_creds = load_qwen_runtime_credentials()?;
    if let Some(runtime_creds) = runtime_creds {
        let expiry_epoch = runtime_creds.expiry_date / 1000;
        if profile.access_token.trim().is_empty() {
            profile.access_token = runtime_creds.access_token;
        }
        if profile.refresh_token.trim().is_empty() {
            profile.refresh_token = runtime_creds.refresh_token;
        }
        if profile.id_token.is_none() {
            profile.id_token = runtime_creds.id_token;
        }
        if profile.token_type.is_none() {
            profile.token_type = Some(runtime_creds.token_type);
        }
        if profile.resource_url.is_none() {
            profile.resource_url = runtime_creds.resource_url;
        }
        if profile.access_token_expires_at_epoch.is_none() {
            profile.access_token_expires_at_epoch = Some(expiry_epoch);
        }
        profile.updated_at_epoch = now_unix_i64();
    }

    if !profile.access_token.trim().is_empty() && !profile.refresh_token.trim().is_empty() {
        return Ok(profile);
    }

    Err("Qwen OAuth 默认账号缺少 access token".to_string())
}

fn qwen_accounts_from_store(
    store: &StoredQwenProfileStore,
) -> Result<Vec<DesktopManagedAuthAccount>, String> {
    let applied_account_id = current_qwen_runtime_account_id(store)?;
    Ok(store
        .profiles
        .iter()
        .cloned()
        .map(|profile| DesktopManagedAuthAccount {
            id: profile.id.clone(),
            provider_id: QWEN_CODE_AUTH_PROVIDER_ID.to_string(),
            email: profile.email.clone(),
            subject: profile.subject.clone(),
            display_label: profile.display_label.clone(),
            plan_label: profile.plan_label.clone(),
            auth_source: match profile.auth_kind.as_str() {
                QWEN_PROFILE_KIND_IMPORTED => DesktopManagedAuthSource::ImportedAuthJson,
                _ => DesktopManagedAuthSource::DeviceCode,
            },
            status: status_from_expiry(profile.access_token_expires_at_epoch),
            is_default: store.active_profile_id.as_deref() == Some(profile.id.as_str()),
            applied_to_runtime: applied_account_id.as_deref() == Some(profile.id.as_str()),
            created_at_epoch: profile.created_at_epoch,
            updated_at_epoch: profile.updated_at_epoch,
            last_refresh_epoch: profile.last_refresh_epoch,
            access_token_expires_at_epoch: profile.access_token_expires_at_epoch,
            resource_url: profile.resource_url.clone(),
        })
        .collect())
}

async fn begin_qwen_login() -> Result<DesktopManagedAuthLoginSessionSnapshot, String> {
    let code_verifier = generate_qwen_code_verifier();
    let code_challenge = generate_qwen_code_challenge(&code_verifier);
    let client = reqwest::Client::new();
    let response = client
        .post(QWEN_OAUTH_DEVICE_CODE_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .header("x-request-id", Uuid::new_v4().to_string())
        .form(&[
            ("client_id", QWEN_OAUTH_CLIENT_ID),
            ("scope", QWEN_OAUTH_SCOPE),
            ("code_challenge", code_challenge.as_str()),
            ("code_challenge_method", "S256"),
        ])
        .send()
        .await
        .map_err(|error| format!("request Qwen device authorization failed: {error}"))?;

    if response.status() != StatusCode::OK {
        let status = response.status();
        let raw = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown authorization error".to_string());
        return Err(format!(
            "request Qwen device authorization failed with HTTP {}: {}",
            status.as_u16(),
            raw.trim()
        ));
    }

    let payload = response
        .json::<QwenDeviceAuthorizationSuccess>()
        .await
        .map_err(|error| format!("parse Qwen device authorization response failed: {error}"))?;

    let session_id = format!("qwen-{}", Uuid::new_v4());
    let created_at_epoch = now_unix_i64();
    let runtime = Arc::new(QwenLoginSessionRuntime {
        session_id: session_id.clone(),
        device_code: payload.device_code,
        code_verifier,
        verification_uri: payload.verification_uri,
        verification_uri_complete: payload.verification_uri_complete,
        user_code: payload.user_code,
        expires_at_epoch: created_at_epoch + payload.expires_in,
        state: Mutex::new(QwenLoginSessionState {
            status: DesktopManagedAuthLoginSessionStatus::Pending,
            error: None,
            profile: None,
            created_at_epoch,
            updated_at_epoch: created_at_epoch,
        }),
    });

    let mut sessions = QWEN_LOGIN_SESSIONS.lock().await;
    sessions.insert(session_id, Arc::clone(&runtime));
    drop(sessions);

    qwen_login_snapshot(&runtime).await
}

async fn poll_qwen_login(
    session_id: &str,
) -> Result<DesktopManagedAuthLoginSessionSnapshot, String> {
    let session = {
        let sessions = QWEN_LOGIN_SESSIONS.lock().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("Qwen login session not found: {session_id}"))?
    };

    {
        let mut state = session.state.lock().await;
        if state.status != DesktopManagedAuthLoginSessionStatus::Pending {
            return qwen_login_snapshot(&session).await;
        }
        if now_unix_i64() >= session.expires_at_epoch {
            state.status = DesktopManagedAuthLoginSessionStatus::Failed;
            state.error = Some(
                "Qwen device authorization expired, please restart the login flow.".to_string(),
            );
            state.updated_at_epoch = now_unix_i64();
            return qwen_login_snapshot(&session).await;
        }
    }

    let client = reqwest::Client::new();
    let response = client
        .post(QWEN_OAUTH_TOKEN_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", QWEN_OAUTH_GRANT_TYPE),
            ("client_id", QWEN_OAUTH_CLIENT_ID),
            ("device_code", session.device_code.as_str()),
            ("code_verifier", session.code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|error| format!("poll Qwen device token failed: {error}"))?;

    if response.status() == StatusCode::OK {
        let token = response
            .json::<QwenTokenSuccess>()
            .await
            .map_err(|error| format!("parse Qwen token response failed: {error}"))?;
        let mut store = load_qwen_store()?;
        let profile = build_qwen_profile_from_token(token, QWEN_PROFILE_KIND_DEVICE_CODE)?;
        let should_apply = true;
        let merged = upsert_qwen_profile(&mut store, profile.clone(), should_apply);
        let store = save_qwen_store(store)?;
        let model_id =
            current_qwen_runtime_model_id().unwrap_or_else(|| QWEN_DEFAULT_MODEL_ID.to_string());
        write_qwen_profile_to_runtime(&merged, &model_id)?;

        let mut state = session.state.lock().await;
        state.status = DesktopManagedAuthLoginSessionStatus::Completed;
        state.error = None;
        state.profile = Some(find_qwen_profile(&store, &merged.id)?);
        state.updated_at_epoch = now_unix_i64();
        drop(state);
        return qwen_login_snapshot(&session).await;
    }

    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "unknown Qwen token error".to_string());

    if let Ok(error) = serde_json::from_str::<QwenErrorResponse>(&body) {
        if status == StatusCode::BAD_REQUEST && error.error == "authorization_pending" {
            return qwen_login_snapshot(&session).await;
        }
        if status == StatusCode::TOO_MANY_REQUESTS && error.error == "slow_down" {
            return qwen_login_snapshot(&session).await;
        }
        let mut state = session.state.lock().await;
        state.status = DesktopManagedAuthLoginSessionStatus::Failed;
        state.error = Some(format_qwen_error(&error));
        state.updated_at_epoch = now_unix_i64();
        drop(state);
        return qwen_login_snapshot(&session).await;
    }

    let mut state = session.state.lock().await;
    state.status = DesktopManagedAuthLoginSessionStatus::Failed;
    state.error = Some(format!(
        "Qwen token exchange failed with HTTP {}: {}",
        status.as_u16(),
        body.trim()
    ));
    state.updated_at_epoch = now_unix_i64();
    drop(state);
    qwen_login_snapshot(&session).await
}

async fn qwen_login_snapshot(
    session: &QwenLoginSessionRuntime,
) -> Result<DesktopManagedAuthLoginSessionSnapshot, String> {
    let state = session.state.lock().await.clone();
    Ok(DesktopManagedAuthLoginSessionSnapshot {
        session_id: session.session_id.clone(),
        provider_id: QWEN_CODE_AUTH_PROVIDER_ID.to_string(),
        status: state.status,
        authorize_url: Some(session.verification_uri_complete.clone()),
        verification_uri: Some(session.verification_uri.clone()),
        verification_uri_complete: Some(session.verification_uri_complete.clone()),
        user_code: Some(session.user_code.clone()),
        redirect_uri: None,
        error: state.error.clone(),
        account: state.profile.map(|profile| DesktopManagedAuthAccount {
            id: profile.id.clone(),
            provider_id: QWEN_CODE_AUTH_PROVIDER_ID.to_string(),
            email: profile.email.clone(),
            subject: profile.subject.clone(),
            display_label: profile.display_label.clone(),
            plan_label: profile.plan_label.clone(),
            auth_source: DesktopManagedAuthSource::DeviceCode,
            status: status_from_expiry(profile.access_token_expires_at_epoch),
            is_default: true,
            applied_to_runtime: true,
            created_at_epoch: profile.created_at_epoch,
            updated_at_epoch: profile.updated_at_epoch,
            last_refresh_epoch: profile.last_refresh_epoch,
            access_token_expires_at_epoch: profile.access_token_expires_at_epoch,
            resource_url: profile.resource_url.clone(),
        }),
        created_at_epoch: state.created_at_epoch,
        updated_at_epoch: state.updated_at_epoch,
    })
}

async fn refresh_qwen_profile(profile: StoredQwenProfile) -> Result<StoredQwenProfile, String> {
    if profile.refresh_token.trim().is_empty() {
        return Err(format!(
            "Qwen account {} does not contain a refresh token",
            profile.display_label
        ));
    }

    let client = reqwest::Client::new();
    let response = client
        .post(QWEN_OAUTH_TOKEN_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", profile.refresh_token.as_str()),
            ("client_id", QWEN_OAUTH_CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|error| format!("refresh Qwen account failed: {error}"))?;

    if response.status() != StatusCode::OK {
        let status = response.status();
        let raw = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown refresh error".to_string());
        return Err(format!(
            "refresh Qwen account failed with HTTP {}: {}",
            status.as_u16(),
            raw.trim()
        ));
    }

    let token = response
        .json::<QwenTokenSuccess>()
        .await
        .map_err(|error| format!("parse Qwen refresh response failed: {error}"))?;

    let mut updated = profile;
    updated.access_token = token.access_token;
    if let Some(refresh_token) = token.refresh_token {
        updated.refresh_token = refresh_token;
    }
    updated.id_token = token.id_token;
    updated.token_type = token.token_type.or_else(|| updated.token_type.clone());
    if token.resource_url.is_some() {
        updated.resource_url = token.resource_url;
    }
    let now = now_unix_i64();
    updated.last_refresh_epoch = Some(now);
    updated.access_token_expires_at_epoch = Some(now + token.expires_in);
    updated.updated_at_epoch = now;
    Ok(updated)
}

fn build_qwen_profile_from_token(
    token: QwenTokenSuccess,
    auth_kind: &str,
) -> Result<StoredQwenProfile, String> {
    let now = now_unix_i64();
    let claims = token
        .id_token
        .as_deref()
        .and_then(|raw| parse_jwt_claims(raw).ok());
    let subject = claims
        .as_ref()
        .and_then(|value| value.get("sub"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let email = claims
        .as_ref()
        .and_then(|value| value.get("email"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let label = email
        .clone()
        .or_else(|| qwen_label_from_resource_url(token.resource_url.as_deref()))
        .unwrap_or_else(|| format!("Qwen OAuth {}", short_id_suffix()));

    let refresh_token = token
        .refresh_token
        .ok_or_else(|| "Qwen token response did not return a refresh token".to_string())?;

    Ok(StoredQwenProfile {
        id: Uuid::new_v4().to_string(),
        display_label: label,
        email,
        subject,
        plan_label: Some("OAuth".to_string()),
        auth_kind: auth_kind.to_string(),
        access_token: token.access_token,
        refresh_token,
        id_token: token.id_token,
        token_type: token.token_type,
        resource_url: token.resource_url,
        last_refresh_epoch: Some(now),
        access_token_expires_at_epoch: Some(now + token.expires_in),
        created_at_epoch: now,
        updated_at_epoch: now,
    })
}

fn upsert_qwen_profile(
    store: &mut StoredQwenProfileStore,
    mut incoming: StoredQwenProfile,
    mark_active: bool,
) -> StoredQwenProfile {
    if let Some(existing_index) = store.profiles.iter().position(|item| {
        (!item.refresh_token.is_empty() && item.refresh_token == incoming.refresh_token)
            || (item.subject.is_some() && item.subject == incoming.subject)
    }) {
        let existing = &store.profiles[existing_index];
        incoming.id = existing.id.clone();
        incoming.created_at_epoch = existing.created_at_epoch;
        if incoming.email.is_none() {
            incoming.email = existing.email.clone();
        }
        if incoming.subject.is_none() {
            incoming.subject = existing.subject.clone();
        }
        if incoming.plan_label.is_none() {
            incoming.plan_label = existing.plan_label.clone();
        }
        if incoming.resource_url.is_none() {
            incoming.resource_url = existing.resource_url.clone();
        }
        store.profiles[existing_index] = incoming.clone();
    } else {
        store.profiles.push(incoming.clone());
    }
    if mark_active {
        store.active_profile_id = Some(incoming.id.clone());
    }
    incoming
}

fn load_qwen_store() -> Result<StoredQwenProfileStore, String> {
    let path = qwen_store_path();
    if !path.exists() {
        return Ok(StoredQwenProfileStore::default());
    }

    // Try encrypted read first. If the file has the WWE1 magic bytes,
    // it was written by secure_storage::write_encrypted. Otherwise we
    // fall back to plaintext for backward compat with pre-AES installs
    // (and migrate the file to encrypted on the next save).
    let payload_string = if crate::secure_storage::is_encrypted(&path) {
        let bytes = crate::secure_storage::read_encrypted(&path).map_err(|e| {
            format!(
                "decrypt Qwen account store failed ({}): {e}",
                path.display()
            )
        })?;
        String::from_utf8(bytes).map_err(|e| {
            format!("Qwen account store contained invalid UTF-8 after decrypt: {e}")
        })?
    } else {
        // Legacy plaintext file. Read as-is; it will be re-saved encrypted
        // on the next save_qwen_store call (transparent migration).
        fs::read_to_string(&path).map_err(|error| {
            format!(
                "read Qwen account store failed ({}): {error}",
                path.display()
            )
        })?
    };

    if payload_string.trim().is_empty() {
        return Ok(StoredQwenProfileStore::default());
    }
    let mut store = serde_json::from_str::<StoredQwenProfileStore>(&payload_string)
        .map_err(|error| format!("parse Qwen account store failed: {error}"))?;
    store
        .profiles
        .sort_by(|left, right| right.updated_at_epoch.cmp(&left.updated_at_epoch));
    Ok(store)
}

fn save_qwen_store(mut store: StoredQwenProfileStore) -> Result<StoredQwenProfileStore, String> {
    store.updated_at_epoch = now_unix_i64();
    store
        .profiles
        .sort_by(|left, right| right.updated_at_epoch.cmp(&left.updated_at_epoch));
    let payload = serde_json::to_string_pretty(&store)
        .map_err(|error| format!("serialize Qwen account store failed: {error}"))?;
    let path = qwen_store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create Qwen account store dir failed: {error}"))?;
    }

    // Always write encrypted. The file format (WWE1 magic + AES-256-GCM)
    // is detected on read, so legacy plaintext files are silently
    // migrated to encrypted on the next save.
    crate::secure_storage::write_encrypted(&path, payload.as_bytes()).map_err(|error| {
        format!(
            "write Qwen account store failed ({}): {error}",
            path.display()
        )
    })?;
    Ok(store)
}

fn find_qwen_profile(
    store: &StoredQwenProfileStore,
    account_id: &str,
) -> Result<StoredQwenProfile, String> {
    store
        .profiles
        .iter()
        .find(|item| item.id == account_id)
        .cloned()
        .ok_or_else(|| format!("Qwen account not found: {account_id}"))
}

fn current_qwen_runtime_account_id(
    store: &StoredQwenProfileStore,
) -> Result<Option<String>, String> {
    let Some(creds) = load_qwen_runtime_credentials()? else {
        return Ok(None);
    };
    Ok(store
        .profiles
        .iter()
        .find(|item| {
            (!item.refresh_token.is_empty() && item.refresh_token == creds.refresh_token)
                || (!item.access_token.is_empty() && item.access_token == creds.access_token)
        })
        .map(|item| item.id.clone()))
}

fn load_qwen_runtime_credentials() -> Result<Option<QwenRuntimeCredentials>, String> {
    let creds_path = qwen_runtime_creds_path();
    if !creds_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&creds_path).map_err(|error| {
        format!(
            "read Qwen runtime credentials failed ({}): {error}",
            creds_path.display()
        )
    })?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let creds = serde_json::from_str::<QwenRuntimeCredentials>(&raw)
        .map_err(|error| format!("parse Qwen runtime credentials failed: {error}"))?;
    Ok(Some(creds))
}

fn current_qwen_runtime_model_id() -> Option<String> {
    let settings_path = qwen_runtime_settings_path();
    let raw = fs::read_to_string(settings_path).ok()?;
    let payload = serde_json::from_str::<Value>(&raw).ok()?;
    payload
        .get("model")
        .and_then(Value::as_object)
        .and_then(|model| model.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn write_qwen_profile_to_runtime(
    profile: &StoredQwenProfile,
    model_id: &str,
) -> Result<(), String> {
    let creds_path = qwen_runtime_creds_path();
    if let Some(parent) = creds_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create Qwen runtime dir failed: {error}"))?;
    }
    let creds = QwenRuntimeCredentials {
        access_token: profile.access_token.clone(),
        refresh_token: profile.refresh_token.clone(),
        id_token: profile.id_token.clone(),
        expiry_date: profile
            .access_token_expires_at_epoch
            .unwrap_or_else(now_unix_i64)
            * 1000,
        token_type: profile
            .token_type
            .clone()
            .unwrap_or_else(|| "Bearer".to_string()),
        resource_url: profile.resource_url.clone(),
    };
    let payload = serde_json::to_string_pretty(&creds)
        .map_err(|error| format!("serialize Qwen runtime credentials failed: {error}"))?;
    fs::write(&creds_path, payload).map_err(|error| {
        format!(
            "write Qwen runtime credentials failed ({}): {error}",
            creds_path.display()
        )
    })?;

    write_qwen_runtime_settings(model_id)
}

fn write_qwen_runtime_settings(model_id: &str) -> Result<(), String> {
    let settings_path = qwen_runtime_settings_path();
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create Qwen runtime settings dir failed: {error}"))?;
    }

    let mut settings = if settings_path.exists() {
        let raw = fs::read_to_string(&settings_path).map_err(|error| {
            format!(
                "read Qwen runtime settings failed ({}): {error}",
                settings_path.display()
            )
        })?;
        if raw.trim().is_empty() {
            QwenRuntimeSettings {
                security: None,
                model: None,
                extra: serde_json::Map::new(),
            }
        } else {
            serde_json::from_str::<QwenRuntimeSettings>(&raw).unwrap_or(QwenRuntimeSettings {
                security: None,
                model: None,
                extra: serde_json::Map::new(),
            })
        }
    } else {
        QwenRuntimeSettings {
            security: None,
            model: None,
            extra: serde_json::Map::new(),
        }
    };

    let mut security = settings.security.unwrap_or(QwenRuntimeSecurity {
        auth: None,
        extra: serde_json::Map::new(),
    });
    let mut auth = security.auth.unwrap_or(QwenRuntimeAuthSettings {
        selected_type: None,
        extra: serde_json::Map::new(),
    });
    auth.selected_type = Some("qwen-oauth".to_string());
    security.auth = Some(auth);
    settings.security = Some(security);

    let mut model = settings.model.unwrap_or(QwenRuntimeModelSettings {
        name: None,
        extra: serde_json::Map::new(),
    });
    model.name = Some(model_id.to_string());
    settings.model = Some(model);

    let payload = serde_json::to_string_pretty(&settings)
        .map_err(|error| format!("serialize Qwen runtime settings failed: {error}"))?;
    fs::write(&settings_path, payload).map_err(|error| {
        format!(
            "write Qwen runtime settings failed ({}): {error}",
            settings_path.display()
        )
    })
}

fn clear_qwen_runtime_creds() -> Result<(), String> {
    let creds_path = qwen_runtime_creds_path();
    if creds_path.exists() {
        fs::remove_file(&creds_path).map_err(|error| {
            format!(
                "remove Qwen runtime credentials failed ({}): {error}",
                creds_path.display()
            )
        })?;
    }
    Ok(())
}

fn qwen_store_path() -> PathBuf {
    current_home_dir()
        .join(QWEN_STORE_DIR)
        .join("qwen")
        .join(QWEN_STORE_FILE)
}

fn qwen_runtime_dir() -> PathBuf {
    current_home_dir().join(QWEN_RUNTIME_DIR)
}

fn qwen_runtime_creds_path() -> PathBuf {
    qwen_runtime_dir().join(QWEN_RUNTIME_CREDS_FILE)
}

fn qwen_runtime_settings_path() -> PathBuf {
    qwen_runtime_dir().join(QWEN_RUNTIME_SETTINGS_FILE)
}

fn normalized_qwen_runtime_base_url(resource_url: Option<&str>) -> String {
    let base_endpoint = resource_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(QWEN_RUNTIME_BASE_URL);
    let normalized = if base_endpoint.starts_with("http") {
        base_endpoint.to_string()
    } else {
        format!("https://{base_endpoint}")
    };
    if normalized.ends_with("/v1") {
        normalized
    } else {
        format!("{normalized}/v1")
    }
}

fn qwen_cli_user_agent() -> String {
    let platform = match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    format!("QwenCode/1.0.0 ({platform}; {arch})")
}

fn qwen_oauth_models() -> Vec<DesktopProviderModel> {
    vec![DesktopProviderModel {
        model_id: QWEN_DEFAULT_MODEL_ID.to_string(),
        display_name: "coder-model".to_string(),
        context_window: None,
        max_output_tokens: None,
        billing_kind: Some("included".to_string()),
        capability_tags: vec!["coding".to_string(), "vision".to_string()],
    }]
}

fn status_from_expiry(expires_at_epoch: Option<i64>) -> DesktopManagedAuthAccountStatus {
    let Some(expires_at_epoch) = expires_at_epoch else {
        return DesktopManagedAuthAccountStatus::NeedsReauth;
    };
    let now = now_unix_i64();
    if expires_at_epoch <= now {
        return DesktopManagedAuthAccountStatus::Expired;
    }
    if expires_at_epoch <= now + EXPIRING_BUFFER_SECONDS {
        return DesktopManagedAuthAccountStatus::Expiring;
    }
    DesktopManagedAuthAccountStatus::Ready
}

fn parse_jwt_claims(jwt: &str) -> Result<Value, String> {
    let mut parts = jwt.split('.');
    let (Some(_header), Some(payload_b64), Some(_signature)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err("invalid JWT format".to_string());
    };
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|error| format!("decode JWT payload failed: {error}"))?;
    serde_json::from_slice::<Value>(&payload_bytes)
        .map_err(|error| format!("parse JWT payload failed: {error}"))
}

fn generate_qwen_code_verifier() -> String {
    let mut seed = String::new();
    for _ in 0..4 {
        seed.push_str(&Uuid::new_v4().simple().to_string());
    }
    seed
}

fn generate_qwen_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn qwen_label_from_resource_url(resource_url: Option<&str>) -> Option<String> {
    let url = resource_url?;
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .or_else(|| Some(url.to_string()))?;
    Some(format!("Qwen OAuth ({host})"))
}

fn default_model_id_from_catalog(
    _accounts: &[DesktopManagedAuthAccount],
    models: &[DesktopProviderModel],
) -> Option<String> {
    models.first().map(|item| item.model_id.clone())
}

fn format_qwen_error(error: &QwenErrorResponse) -> String {
    match &error.error_description {
        Some(description) if !description.trim().is_empty() => {
            format!("{} - {}", error.error, description.trim())
        }
        _ => error.error.clone(),
    }
}

fn now_unix_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn short_id_suffix() -> String {
    Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect()
}

fn current_home_dir() -> PathBuf {
    env::var("HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

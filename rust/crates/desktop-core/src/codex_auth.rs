use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use base64::Engine;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

const PROFILES_STORE_DIR: &str = ".warwolf";
const PROFILES_STORE_FILE: &str = "profiles.json";
const DEFAULT_REFRESH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_REFRESH_URL: &str = "https://auth.openai.com/oauth/token";
const DEFAULT_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_OAUTH_ISSUER: &str = "https://auth.openai.com";
const DEFAULT_ORIGINATOR: &str = "codex_cli_rs";
const MACOS_CODEX_APP_PATH: &str = "/Applications/Codex.app";

static LOGIN_SESSIONS: LazyLock<Mutex<HashMap<String, Arc<LoginSessionRuntime>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopCodexAuthSource {
    ImportedAuthJson,
    BrowserLogin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexProfileSummary {
    pub id: String,
    pub email: String,
    pub display_label: String,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub chatgpt_plan_type: Option<String>,
    pub auth_source: DesktopCodexAuthSource,
    pub active: bool,
    pub applied_to_codex: bool,
    pub last_refresh_epoch: Option<i64>,
    pub access_token_expires_at_epoch: Option<i64>,
    pub updated_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexInstallationRecord {
    pub target_id: String,
    pub target_label: String,
    pub installed: bool,
    pub path: Option<String>,
    pub auth_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexAuthOverview {
    pub profiles: Vec<DesktopCodexProfileSummary>,
    pub installations: Vec<DesktopCodexInstallationRecord>,
    pub active_profile_id: Option<String>,
    pub auth_path: String,
    pub auth_mode: Option<String>,
    pub has_chatgpt_tokens: bool,
    pub updated_at_epoch: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopCodexLoginSessionStatus {
    Pending,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexLoginSessionSnapshot {
    pub session_id: String,
    pub status: DesktopCodexLoginSessionStatus,
    pub authorize_url: String,
    pub redirect_uri: String,
    pub error: Option<String>,
    pub profile: Option<DesktopCodexProfileSummary>,
    pub created_at_epoch: i64,
    pub updated_at_epoch: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChatgptJwtClaims {
    pub email: Option<String>,
    pub chatgpt_plan_type: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub exp: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexAuthJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_mode: Option<String>,
    #[serde(
        rename = "OPENAI_API_KEY",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    openai_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tokens: Option<CodexAuthTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_refresh: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexAuthTokens {
    id_token: String,
    access_token: String,
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCodexProfileStore {
    version: u8,
    active_profile_id: Option<String>,
    #[serde(default)]
    profiles: Vec<StoredCodexProfile>,
    updated_at_epoch: i64,
}

impl Default for StoredCodexProfileStore {
    fn default() -> Self {
        Self {
            version: 1,
            active_profile_id: None,
            profiles: Vec::new(),
            updated_at_epoch: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCodexProfile {
    id: String,
    email: String,
    display_label: String,
    chatgpt_account_id: Option<String>,
    chatgpt_user_id: Option<String>,
    chatgpt_plan_type: Option<String>,
    auth_source: DesktopCodexAuthSource,
    id_token_raw: String,
    access_token: String,
    refresh_token: String,
    account_id: Option<String>,
    imported_from_auth_path: Option<String>,
    last_refresh_epoch: Option<i64>,
    access_token_expires_at_epoch: Option<i64>,
    created_at_epoch: i64,
    updated_at_epoch: i64,
}

#[derive(Debug, Deserialize)]
struct IdClaims {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "https://api.openai.com/profile", default)]
    profile: Option<ProfileClaims>,
    #[serde(rename = "https://api.openai.com/auth", default)]
    auth: Option<AuthClaims>,
    #[serde(default)]
    exp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ProfileClaims {
    #[serde(default)]
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthClaims {
    #[serde(default)]
    chatgpt_plan_type: Option<String>,
    #[serde(default)]
    chatgpt_user_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    chatgpt_account_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct BeginLoginOptions {
    pub codex_home_override: Option<String>,
    pub issuer: String,
    pub client_id: String,
}

impl Default for BeginLoginOptions {
    fn default() -> Self {
        Self {
            codex_home_override: None,
            issuer: DEFAULT_OAUTH_ISSUER.to_string(),
            client_id: DEFAULT_OAUTH_CLIENT_ID.to_string(),
        }
    }
}

#[derive(Debug)]
struct LoginSessionRuntime {
    session_id: String,
    expected_state: String,
    pkce_verifier: String,
    authorize_url: String,
    redirect_uri: String,
    issuer: String,
    client_id: String,
    codex_home_override: Option<String>,
    state: Mutex<LoginSessionState>,
    shutdown_sender: Mutex<Option<oneshot::Sender<()>>>,
}

#[derive(Debug, Clone)]
struct LoginSessionState {
    status: DesktopCodexLoginSessionStatus,
    error: Option<String>,
    profile: Option<StoredCodexProfile>,
    created_at_epoch: i64,
    updated_at_epoch: i64,
}

#[derive(Debug, Deserialize)]
struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug)]
struct ExchangedTokens {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

pub(crate) fn overview_get(
    codex_home_override: Option<&str>,
) -> Result<DesktopCodexAuthOverview, String> {
    let store = load_store()?;
    build_overview(&store, codex_home_override)
}

pub(crate) fn profile_import(
    codex_home_override: Option<&str>,
) -> Result<DesktopCodexAuthOverview, String> {
    let auth_path = resolve_default_auth_path(codex_home_override);
    let mut store = load_store()?;
    let imported =
        import_profile_from_auth_path(&auth_path, DesktopCodexAuthSource::ImportedAuthJson)?;
    upsert_profile(&mut store, imported, true);
    let store = save_store(store)?;
    build_overview(&store, codex_home_override)
}

pub(crate) fn profile_set_active(
    profile_id: String,
    codex_home_override: Option<&str>,
) -> Result<DesktopCodexAuthOverview, String> {
    let mut store = load_store()?;
    let profile = find_profile(&store, &profile_id)?;
    write_profile_to_auth_paths(&profile, codex_home_override)?;
    for item in &mut store.profiles {
        if item.id == profile.id {
            item.updated_at_epoch = now_unix_i64();
        }
    }
    store.active_profile_id = Some(profile.id);
    let store = save_store(store)?;
    build_overview(&store, codex_home_override)
}

pub(crate) fn profile_remove(
    profile_id: String,
    codex_home_override: Option<&str>,
) -> Result<DesktopCodexAuthOverview, String> {
    let mut store = load_store()?;
    store.profiles.retain(|item| item.id != profile_id);
    if store.active_profile_id.as_deref() == Some(profile_id.as_str()) {
        store.active_profile_id = None;
    }
    let store = save_store(store)?;
    build_overview(&store, codex_home_override)
}

pub(crate) async fn profile_refresh(
    profile_id: String,
    codex_home_override: Option<&str>,
) -> Result<DesktopCodexAuthOverview, String> {
    let mut store = load_store()?;
    let profile = find_profile(&store, &profile_id)?;
    let refreshed = refresh_profile_tokens(profile).await?;
    let applied = store.active_profile_id.as_deref() == Some(refreshed.id.as_str());
    upsert_profile(&mut store, refreshed.clone(), applied);
    if applied {
        write_profile_to_auth_paths(&refreshed, codex_home_override)?;
    }
    let store = save_store(store)?;
    build_overview(&store, codex_home_override)
}

pub(crate) async fn login_begin(
    codex_home_override: Option<String>,
) -> Result<DesktopCodexLoginSessionSnapshot, String> {
    begin_login(BeginLoginOptions {
        codex_home_override,
        ..BeginLoginOptions::default()
    })
    .await
}

pub(crate) async fn login_poll(
    session_id: String,
) -> Result<DesktopCodexLoginSessionSnapshot, String> {
    poll_login(&session_id).await
}

pub(crate) fn parse_chatgpt_jwt_claims(jwt: &str) -> Result<ChatgptJwtClaims, String> {
    let mut parts = jwt.split('.');
    let (Some(_header), Some(payload_b64), Some(_signature)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err("invalid JWT format".to_string());
    };
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|err| format!("decode JWT payload failed: {err}"))?;
    let claims = serde_json::from_slice::<IdClaims>(&payload_bytes)
        .map_err(|err| format!("parse JWT payload failed: {err}"))?;
    Ok(ChatgptJwtClaims {
        email: claims
            .email
            .or_else(|| claims.profile.and_then(|profile| profile.email)),
        chatgpt_plan_type: claims
            .auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_plan_type.clone()),
        chatgpt_user_id: claims
            .auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_user_id.clone().or(auth.user_id.clone())),
        chatgpt_account_id: claims.auth.and_then(|auth| auth.chatgpt_account_id),
        exp: claims.exp,
    })
}

pub(crate) fn resolve_default_auth_path(codex_home_override: Option<&str>) -> PathBuf {
    default_codex_home(codex_home_override).join("auth.json")
}

pub(crate) fn read_auth_payload(codex_home_override: Option<&str>) -> Result<Value, String> {
    let auth_path = resolve_default_auth_path(codex_home_override);
    read_auth_json(&auth_path)
}

pub(crate) fn has_chatgpt_tokens(payload: &Value) -> bool {
    payload
        .get("tokens")
        .and_then(Value::as_object)
        .map(|tokens| {
            ["id_token", "access_token", "refresh_token"]
                .iter()
                .all(|key| {
                    tokens
                        .get(*key)
                        .and_then(Value::as_str)
                        .map(|value| !value.trim().is_empty())
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false)
}

pub(crate) fn default_codex_home(codex_home_override: Option<&str>) -> PathBuf {
    if let Some(override_path) = codex_home_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return PathBuf::from(override_path);
    }
    if let Ok(env_path) = env::var("CODEX_HOME") {
        let trimmed = env_path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    current_home_dir().join(".codex")
}

fn profiles_store_path() -> PathBuf {
    current_home_dir()
        .join(PROFILES_STORE_DIR)
        .join("codex")
        .join(PROFILES_STORE_FILE)
}

fn load_store() -> Result<StoredCodexProfileStore, String> {
    let path = profiles_store_path();
    if !path.exists() {
        return Ok(StoredCodexProfileStore::default());
    }
    let payload = fs::read_to_string(&path).map_err(|err| {
        format!(
            "read Codex profiles store failed ({}): {err}",
            path.display()
        )
    })?;
    if payload.trim().is_empty() {
        return Ok(StoredCodexProfileStore::default());
    }
    let mut store = serde_json::from_str::<StoredCodexProfileStore>(&payload)
        .map_err(|err| format!("parse Codex profiles store failed: {err}"))?;
    store
        .profiles
        .sort_by(|left, right| right.updated_at_epoch.cmp(&left.updated_at_epoch));
    Ok(store)
}

fn save_store(mut store: StoredCodexProfileStore) -> Result<StoredCodexProfileStore, String> {
    store.updated_at_epoch = now_unix_i64();
    store
        .profiles
        .sort_by(|left, right| right.updated_at_epoch.cmp(&left.updated_at_epoch));
    let payload = serde_json::to_string_pretty(&store)
        .map_err(|err| format!("serialize Codex profiles store failed: {err}"))?;
    let path = profiles_store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create Codex profiles store dir failed: {err}"))?;
    }
    fs::write(&path, payload).map_err(|err| {
        format!(
            "write Codex profiles store failed ({}): {err}",
            path.display()
        )
    })?;
    Ok(store)
}

fn import_profile_from_auth_path(
    auth_path: &Path,
    auth_source: DesktopCodexAuthSource,
) -> Result<StoredCodexProfile, String> {
    let auth_raw = fs::read_to_string(auth_path)
        .map_err(|err| format!("read Codex auth failed ({}): {err}", auth_path.display()))?;
    let auth = serde_json::from_str::<CodexAuthJson>(&auth_raw)
        .map_err(|err| format!("parse Codex auth failed ({}): {err}", auth_path.display()))?;
    let tokens = auth.tokens.ok_or_else(|| {
        format!(
            "Codex auth at {} does not contain ChatGPT tokens",
            auth_path.display()
        )
    })?;
    build_profile_from_tokens(
        tokens,
        Some(now_unix_i64()),
        auth_source,
        Some(auth_path.to_string_lossy().to_string()),
    )
}

fn build_profile_from_tokens(
    tokens: CodexAuthTokens,
    last_refresh_epoch: Option<i64>,
    auth_source: DesktopCodexAuthSource,
    imported_from_auth_path: Option<String>,
) -> Result<StoredCodexProfile, String> {
    let claims = parse_chatgpt_jwt_claims(&tokens.id_token)?;
    let now = now_unix_i64();
    Ok(StoredCodexProfile {
        id: Uuid::new_v4().to_string(),
        email: claims
            .email
            .clone()
            .unwrap_or_else(|| "unknown@chatgpt.local".to_string()),
        display_label: claims
            .email
            .clone()
            .or_else(|| claims.chatgpt_account_id.clone())
            .unwrap_or_else(|| "Codex ChatGPT Account".to_string()),
        chatgpt_account_id: claims.chatgpt_account_id.clone(),
        chatgpt_user_id: claims.chatgpt_user_id.clone(),
        chatgpt_plan_type: claims.chatgpt_plan_type.clone(),
        auth_source,
        id_token_raw: tokens.id_token,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        account_id: tokens.account_id.or(claims.chatgpt_account_id),
        imported_from_auth_path,
        last_refresh_epoch,
        access_token_expires_at_epoch: claims.exp,
        created_at_epoch: now,
        updated_at_epoch: now,
    })
}

fn upsert_profile(
    store: &mut StoredCodexProfileStore,
    mut incoming: StoredCodexProfile,
    mark_active: bool,
) -> StoredCodexProfile {
    if let Some(existing_index) = store.profiles.iter().position(|item| {
        (!item.refresh_token.is_empty() && item.refresh_token == incoming.refresh_token)
            || (item.email.eq_ignore_ascii_case(&incoming.email)
                && item.chatgpt_account_id == incoming.chatgpt_account_id)
    }) {
        let existing = &store.profiles[existing_index];
        incoming.id = existing.id.clone();
        incoming.created_at_epoch = existing.created_at_epoch;
        if incoming.imported_from_auth_path.is_none() {
            incoming.imported_from_auth_path = existing.imported_from_auth_path.clone();
        }
        if incoming.last_refresh_epoch.is_none() {
            incoming.last_refresh_epoch = existing.last_refresh_epoch;
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

fn find_profile(
    store: &StoredCodexProfileStore,
    profile_id: &str,
) -> Result<StoredCodexProfile, String> {
    store
        .profiles
        .iter()
        .find(|item| item.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("Codex profile not found: {profile_id}"))
}

fn write_profile_to_auth_paths(
    profile: &StoredCodexProfile,
    codex_home_override: Option<&str>,
) -> Result<Vec<String>, String> {
    let auth = CodexAuthJson {
        auth_mode: Some("chatgpt".to_string()),
        openai_api_key: None,
        tokens: Some(CodexAuthTokens {
            id_token: profile.id_token_raw.clone(),
            access_token: profile.access_token.clone(),
            refresh_token: profile.refresh_token.clone(),
            account_id: profile.account_id.clone(),
        }),
        last_refresh: Some(now_rfc3339()?),
    };
    let payload = serde_json::to_string_pretty(&auth)
        .map_err(|err| format!("serialize Codex auth payload failed: {err}"))?;
    let mut written = Vec::new();
    let auth_paths = resolve_auth_paths(codex_home_override);
    for (index, auth_path) in auth_paths.into_iter().enumerate() {
        if index > 0 && !auth_path.exists() {
            continue;
        }
        if let Some(parent) = auth_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("create Codex auth dir failed ({}): {err}", parent.display())
            })?;
        }
        fs::write(&auth_path, &payload)
            .map_err(|err| format!("write Codex auth failed ({}): {err}", auth_path.display()))?;
        written.push(auth_path.to_string_lossy().to_string());
    }
    Ok(written)
}

fn build_overview(
    store: &StoredCodexProfileStore,
    codex_home_override: Option<&str>,
) -> Result<DesktopCodexAuthOverview, String> {
    let installations = detect_codex_installations(codex_home_override);
    let current_profile_id = current_applied_profile_id(store, codex_home_override)?;
    let current_auth = read_auth_json(&resolve_default_auth_path(codex_home_override))?;
    let profiles = store
        .profiles
        .iter()
        .map(|profile| DesktopCodexProfileSummary {
            id: profile.id.clone(),
            email: profile.email.clone(),
            display_label: profile.display_label.clone(),
            chatgpt_account_id: profile.chatgpt_account_id.clone(),
            chatgpt_user_id: profile.chatgpt_user_id.clone(),
            chatgpt_plan_type: profile.chatgpt_plan_type.clone(),
            auth_source: profile.auth_source,
            active: store.active_profile_id.as_deref() == Some(profile.id.as_str()),
            applied_to_codex: current_profile_id.as_deref() == Some(profile.id.as_str()),
            last_refresh_epoch: profile.last_refresh_epoch,
            access_token_expires_at_epoch: profile.access_token_expires_at_epoch,
            updated_at_epoch: profile.updated_at_epoch,
        })
        .collect();
    Ok(DesktopCodexAuthOverview {
        profiles,
        installations,
        active_profile_id: store.active_profile_id.clone(),
        auth_path: resolve_default_auth_path(codex_home_override)
            .to_string_lossy()
            .to_string(),
        auth_mode: current_auth
            .get("auth_mode")
            .and_then(Value::as_str)
            .map(str::to_string),
        has_chatgpt_tokens: has_chatgpt_tokens(&current_auth),
        updated_at_epoch: now_unix_i64(),
    })
}

fn current_applied_profile_id(
    store: &StoredCodexProfileStore,
    codex_home_override: Option<&str>,
) -> Result<Option<String>, String> {
    let current_auth_path = resolve_auth_paths(codex_home_override)
        .into_iter()
        .find(|path| path.exists());
    let Some(auth_path) = current_auth_path else {
        return Ok(None);
    };
    let current =
        match import_profile_from_auth_path(&auth_path, DesktopCodexAuthSource::ImportedAuthJson) {
            Ok(profile) => profile,
            Err(_) => return Ok(None),
        };
    Ok(store
        .profiles
        .iter()
        .find(|item| {
            (!item.refresh_token.is_empty() && item.refresh_token == current.refresh_token)
                || (item.email.eq_ignore_ascii_case(&current.email)
                    && item.chatgpt_account_id == current.chatgpt_account_id)
        })
        .map(|item| item.id.clone()))
}

fn detect_codex_installations(
    codex_home_override: Option<&str>,
) -> Vec<DesktopCodexInstallationRecord> {
    let auth_path = resolve_default_auth_path(codex_home_override)
        .to_string_lossy()
        .to_string();
    let cli_path = find_executable_in_path("codex");
    let mut out = vec![DesktopCodexInstallationRecord {
        target_id: "cli".to_string(),
        target_label: "Codex CLI".to_string(),
        installed: cli_path.is_some(),
        path: cli_path.map(|path| path.to_string_lossy().to_string()),
        auth_path: auth_path.clone(),
    }];
    #[cfg(target_os = "macos")]
    {
        let desktop_path = Path::new(MACOS_CODEX_APP_PATH);
        out.push(DesktopCodexInstallationRecord {
            target_id: "desktop".to_string(),
            target_label: "Codex Desktop".to_string(),
            installed: desktop_path.exists(),
            path: desktop_path
                .exists()
                .then(|| MACOS_CODEX_APP_PATH.to_string()),
            auth_path,
        });
    }
    out
}

fn resolve_auth_paths(codex_home_override: Option<&str>) -> Vec<PathBuf> {
    let default_auth_path = resolve_default_auth_path(codex_home_override);
    let mut candidates = vec![default_auth_path];
    #[cfg(target_os = "macos")]
    {
        let app_support = current_home_dir()
            .join("Library")
            .join("Application Support")
            .join("Codex");
        candidates.push(app_support.join("auth.json"));
        candidates.push(app_support.join("codex").join("auth.json"));
    }
    dedup_paths(candidates)
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for path in paths {
        let identity = path.to_string_lossy().to_string();
        if seen.insert(identity) {
            unique.push(path);
        }
    }
    unique
}

fn read_auth_json(auth_path: &Path) -> Result<Value, String> {
    if !auth_path.exists() {
        return Ok(json!({}));
    }
    let auth_text = fs::read_to_string(auth_path)
        .map_err(|err| format!("read Codex auth failed ({}): {err}", auth_path.display()))?;
    if auth_text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(&auth_text)
        .map_err(|err| format!("parse Codex auth failed ({}): {err}", auth_path.display()))
}

async fn refresh_profile_tokens(profile: StoredCodexProfile) -> Result<StoredCodexProfile, String> {
    #[derive(Debug, Deserialize)]
    struct RefreshResponse {
        id_token: Option<String>,
        access_token: Option<String>,
        refresh_token: Option<String>,
    }

    if profile.refresh_token.trim().is_empty() {
        return Err(format!(
            "Codex profile {} does not contain a refresh token",
            profile.display_label
        ));
    }

    let client = reqwest::Client::new();
    let response = client
        .post(DEFAULT_REFRESH_URL)
        .form(&[
            ("client_id", DEFAULT_REFRESH_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", profile.refresh_token.as_str()),
        ])
        .send()
        .await
        .map_err(|err| format!("refresh Codex profile failed: {err}"))?;
    let status = response.status();
    if status != StatusCode::OK {
        let raw = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown refresh error".to_string());
        return Err(format!(
            "refresh Codex profile failed with HTTP {}: {}",
            status.as_u16(),
            raw.trim()
        ));
    }
    let refreshed = response
        .json::<RefreshResponse>()
        .await
        .map_err(|err| format!("parse refresh token response failed: {err}"))?;

    let mut updated = profile.clone();
    if let Some(id_token) = refreshed.id_token {
        let claims = parse_chatgpt_jwt_claims(&id_token)?;
        updated.id_token_raw = id_token;
        updated.email = claims
            .email
            .clone()
            .unwrap_or_else(|| updated.email.clone());
        updated.display_label = claims
            .email
            .clone()
            .or_else(|| claims.chatgpt_account_id.clone())
            .unwrap_or_else(|| updated.display_label.clone());
        updated.chatgpt_account_id = claims
            .chatgpt_account_id
            .clone()
            .or(updated.chatgpt_account_id.clone());
        updated.chatgpt_user_id = claims
            .chatgpt_user_id
            .clone()
            .or(updated.chatgpt_user_id.clone());
        updated.chatgpt_plan_type = claims
            .chatgpt_plan_type
            .clone()
            .or(updated.chatgpt_plan_type.clone());
        updated.access_token_expires_at_epoch =
            claims.exp.or(updated.access_token_expires_at_epoch);
        updated.account_id = updated
            .account_id
            .clone()
            .or(updated.chatgpt_account_id.clone());
    }
    if let Some(access_token) = refreshed.access_token {
        updated.access_token = access_token;
    }
    if let Some(refresh_token) = refreshed.refresh_token {
        updated.refresh_token = refresh_token;
    }
    updated.last_refresh_epoch = Some(now_unix_i64());
    updated.updated_at_epoch = now_unix_i64();
    Ok(updated)
}

async fn begin_login(
    options: BeginLoginOptions,
) -> Result<DesktopCodexLoginSessionSnapshot, String> {
    let session_id = Uuid::new_v4().to_string();
    let state_token = Uuid::new_v4().to_string();
    let pkce_verifier = build_pkce_verifier();
    let pkce_challenge = pkce_code_challenge(&pkce_verifier);
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:1455").await {
        Ok(listener) => listener,
        Err(_) => tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|err| format!("bind Codex login callback server failed: {err}"))?,
    };
    let port = listener
        .local_addr()
        .map_err(|err| format!("read Codex login callback port failed: {err}"))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/auth/callback");
    let authorize_url = build_authorize_url(
        &options.issuer,
        &options.client_id,
        &redirect_uri,
        &pkce_challenge,
        &state_token,
    );
    let created_at_epoch = now_unix_i64();
    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let runtime = Arc::new(LoginSessionRuntime {
        session_id: session_id.clone(),
        expected_state: state_token,
        pkce_verifier,
        authorize_url: authorize_url.clone(),
        redirect_uri: redirect_uri.clone(),
        issuer: options.issuer,
        client_id: options.client_id,
        codex_home_override: options.codex_home_override,
        state: Mutex::new(LoginSessionState {
            status: DesktopCodexLoginSessionStatus::Pending,
            error: None,
            profile: None,
            created_at_epoch,
            updated_at_epoch: created_at_epoch,
        }),
        shutdown_sender: Mutex::new(Some(shutdown_sender)),
    });

    LOGIN_SESSIONS
        .lock()
        .await
        .insert(session_id.clone(), runtime.clone());

    let app = Router::new()
        .route("/auth/callback", get(handle_auth_callback))
        .with_state(runtime.clone());
    tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = shutdown_receiver.await;
        });
        if let Err(err) = server.await {
            let mut guard = runtime.state.lock().await;
            if guard.status == DesktopCodexLoginSessionStatus::Pending {
                guard.status = DesktopCodexLoginSessionStatus::Failed;
                guard.error = Some(format!("Codex login callback server failed: {err}"));
                guard.updated_at_epoch = now_unix_i64();
            }
        }
    });

    poll_login(&session_id).await
}

async fn poll_login(session_id: &str) -> Result<DesktopCodexLoginSessionSnapshot, String> {
    let runtime = LOGIN_SESSIONS
        .lock()
        .await
        .get(session_id)
        .cloned()
        .ok_or_else(|| format!("Codex login session not found: {session_id}"))?;
    let guard = runtime.state.lock().await;
    Ok(DesktopCodexLoginSessionSnapshot {
        session_id: runtime.session_id.clone(),
        status: guard.status,
        authorize_url: runtime.authorize_url.clone(),
        redirect_uri: runtime.redirect_uri.clone(),
        error: guard.error.clone(),
        profile: guard.profile.as_ref().map(build_profile_summary),
        created_at_epoch: guard.created_at_epoch,
        updated_at_epoch: guard.updated_at_epoch,
    })
}

async fn handle_auth_callback(
    State(runtime): State<Arc<LoginSessionRuntime>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
    let html = match complete_login(runtime.clone(), query).await {
        Ok(message) => success_html(&message),
        Err(message) => {
            let mut guard = runtime.state.lock().await;
            if guard.status == DesktopCodexLoginSessionStatus::Pending {
                guard.status = DesktopCodexLoginSessionStatus::Failed;
                guard.error = Some(message.clone());
                guard.updated_at_epoch = now_unix_i64();
            }
            error_html(&message)
        }
    };
    if let Some(sender) = runtime.shutdown_sender.lock().await.take() {
        let _ = sender.send(());
    }
    Html(html)
}

async fn complete_login(
    runtime: Arc<LoginSessionRuntime>,
    query: OAuthCallbackQuery,
) -> Result<String, String> {
    if let Some(error_code) = query.error.as_deref() {
        let message = query
            .error_description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("ChatGPT authorization failed: {value}"))
            .unwrap_or_else(|| format!("ChatGPT authorization failed: {error_code}"));
        let mut guard = runtime.state.lock().await;
        guard.status = DesktopCodexLoginSessionStatus::Failed;
        guard.error = Some(message.clone());
        guard.updated_at_epoch = now_unix_i64();
        return Err(message);
    }

    let Some(code) = query
        .code
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        let message = "ChatGPT authorization callback is missing the code parameter".to_string();
        let mut guard = runtime.state.lock().await;
        guard.status = DesktopCodexLoginSessionStatus::Failed;
        guard.error = Some(message.clone());
        guard.updated_at_epoch = now_unix_i64();
        return Err(message);
    };

    if query.state.as_deref() != Some(runtime.expected_state.as_str()) {
        let message = "ChatGPT authorization callback state validation failed".to_string();
        let mut guard = runtime.state.lock().await;
        guard.status = DesktopCodexLoginSessionStatus::Failed;
        guard.error = Some(message.clone());
        guard.updated_at_epoch = now_unix_i64();
        return Err(message);
    }

    let exchanged = exchange_code_for_tokens(
        &runtime.issuer,
        &runtime.client_id,
        &runtime.redirect_uri,
        &runtime.pkce_verifier,
        code,
    )
    .await?;
    let claims = parse_chatgpt_jwt_claims(&exchanged.id_token)?;
    let mut profile = StoredCodexProfile {
        id: Uuid::new_v4().to_string(),
        email: claims
            .email
            .clone()
            .unwrap_or_else(|| "unknown@chatgpt.local".to_string()),
        display_label: claims
            .email
            .clone()
            .or_else(|| claims.chatgpt_account_id.clone())
            .unwrap_or_else(|| "Codex ChatGPT Account".to_string()),
        chatgpt_account_id: claims.chatgpt_account_id.clone(),
        chatgpt_user_id: claims.chatgpt_user_id.clone(),
        chatgpt_plan_type: claims.chatgpt_plan_type.clone(),
        auth_source: DesktopCodexAuthSource::BrowserLogin,
        id_token_raw: exchanged.id_token,
        access_token: exchanged.access_token,
        refresh_token: exchanged.refresh_token,
        account_id: claims.chatgpt_account_id.clone(),
        imported_from_auth_path: Some(
            default_codex_home(runtime.codex_home_override.as_deref())
                .join("auth.json")
                .to_string_lossy()
                .to_string(),
        ),
        last_refresh_epoch: Some(now_unix_i64()),
        access_token_expires_at_epoch: claims.exp,
        created_at_epoch: now_unix_i64(),
        updated_at_epoch: now_unix_i64(),
    };

    let mut store = load_store()?;
    profile = upsert_profile(&mut store, profile, true);
    let store = save_store(store)?;
    write_profile_to_auth_paths(&profile, runtime.codex_home_override.as_deref())?;

    let mut guard = runtime.state.lock().await;
    guard.status = DesktopCodexLoginSessionStatus::Completed;
    guard.error = None;
    guard.profile = store
        .profiles
        .iter()
        .find(|item| item.id == profile.id)
        .cloned();
    guard.updated_at_epoch = now_unix_i64();
    Ok(
        "Authorization completed. Warwolf saved the account profile and updated local Codex credentials."
            .to_string(),
    )
}

fn build_authorize_url(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
) -> String {
    let params = [
        ("response_type", "code".to_string()),
        ("client_id", client_id.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        (
            "scope",
            "openid profile email offline_access api.connectors.read api.connectors.invoke"
                .to_string(),
        ),
        ("code_challenge", code_challenge.to_string()),
        ("code_challenge_method", "S256".to_string()),
        ("id_token_add_organizations", "true".to_string()),
        ("codex_cli_simplified_flow", "true".to_string()),
        ("state", state.to_string()),
        ("originator", DEFAULT_ORIGINATOR.to_string()),
    ];
    let query = params
        .into_iter()
        .fold(
            url::form_urlencoded::Serializer::new(String::new()),
            |mut serializer, (key, value)| {
                serializer.append_pair(key, value.as_str());
                serializer
            },
        )
        .finish();
    format!("{issuer}/oauth/authorize?{query}")
}

async fn exchange_code_for_tokens(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce_verifier: &str,
    code: &str,
) -> Result<ExchangedTokens, String> {
    #[derive(Debug, Deserialize)]
    struct TokenResponse {
        id_token: String,
        access_token: String,
        refresh_token: String,
    }

    let body = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", pkce_verifier),
    ];
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{issuer}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&body)
        .send()
        .await
        .map_err(|err| format!("exchange ChatGPT auth code failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let raw = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown token endpoint error".to_string());
        return Err(format!(
            "exchange ChatGPT auth code failed with HTTP {}: {}",
            status.as_u16(),
            raw.trim()
        ));
    }
    let tokens = response
        .json::<TokenResponse>()
        .await
        .map_err(|err| format!("parse ChatGPT token response failed: {err}"))?;
    Ok(ExchangedTokens {
        id_token: tokens.id_token,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    })
}

fn build_pkce_verifier() -> String {
    let seed = format!("{}{}", Uuid::new_v4(), Uuid::new_v4());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(seed.as_bytes())
}

fn pkce_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn build_profile_summary(profile: &StoredCodexProfile) -> DesktopCodexProfileSummary {
    DesktopCodexProfileSummary {
        id: profile.id.clone(),
        email: profile.email.clone(),
        display_label: profile.display_label.clone(),
        chatgpt_account_id: profile.chatgpt_account_id.clone(),
        chatgpt_user_id: profile.chatgpt_user_id.clone(),
        chatgpt_plan_type: profile.chatgpt_plan_type.clone(),
        auth_source: profile.auth_source,
        active: false,
        applied_to_codex: false,
        last_refresh_epoch: profile.last_refresh_epoch,
        access_token_expires_at_epoch: profile.access_token_expires_at_epoch,
        updated_at_epoch: profile.updated_at_epoch,
    }
}

fn success_html(message: &str) -> String {
    format!(
        "<html><body style=\"font-family:-apple-system,system-ui;padding:32px;\"><h1>Codex authorization completed</h1><p>{message}</p><p>You can now close this page and return to Warwolf.</p></body></html>"
    )
}

fn error_html(message: &str) -> String {
    format!(
        "<html><body style=\"font-family:-apple-system,system-ui;padding:32px;\"><h1>Codex authorization failed</h1><p>{message}</p><p>Please close this page and try again.</p></body></html>"
    )
}

fn find_executable_in_path(cmd: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(cmd);
        if is_valid_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_valid_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn now_rfc3339() -> Result<String, String> {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| format!("format timestamp failed: {err}"))
}

fn now_unix_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn current_home_dir() -> PathBuf {
    env::var("HOME")
        .ok()
        .map(|value| PathBuf::from(value.trim()))
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

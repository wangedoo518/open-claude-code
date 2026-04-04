use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::codex_auth::{has_chatgpt_tokens, parse_chatgpt_jwt_claims, read_auth_payload};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::StatusCode;
use runtime::ConfigLoader;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use toml_edit::{value as toml_value, DocumentMut, InlineTable, Item};

const PROVIDER_HUB_FILE: &str = "warwolf-provider-hub.json";
const DEFAULT_OPENCLAW_CONFIG_FILE: &str = "openclaw.json";
const ALTERNATE_OPENCLAW_CONFIG_FILE: &str = "config.json";
const DEFAULT_CODEX_CONFIG_FILE: &str = "config.toml";
const DEFAULT_CODEX_AUTH_FILE: &str = "auth.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProviderRuntimeTarget {
    OpenClaw,
    Codex,
}

impl Default for DesktopProviderRuntimeTarget {
    fn default() -> Self {
        Self::OpenClaw
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderModel {
    pub model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub billing_kind: Option<String>,
    pub capability_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderPreset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub runtime_target: DesktopProviderRuntimeTarget,
    pub category: String,
    pub provider_type: String,
    pub billing_category: String,
    pub protocol: String,
    pub base_url: String,
    pub official_verified: bool,
    pub website_url: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub models: Vec<DesktopProviderModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedProvider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub runtime_target: DesktopProviderRuntimeTarget,
    pub category: String,
    pub provider_type: String,
    pub billing_category: String,
    pub protocol: String,
    pub base_url: String,
    pub api_key_masked: Option<String>,
    pub has_api_key: bool,
    pub enabled: bool,
    pub official_verified: bool,
    pub preset_id: Option<String>,
    pub website_url: Option<String>,
    pub description: Option<String>,
    pub models: Vec<DesktopProviderModel>,
    pub created_at_epoch: i64,
    pub updated_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedProviderUpsertInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub runtime_target: DesktopProviderRuntimeTarget,
    pub category: String,
    pub provider_type: String,
    pub billing_category: String,
    pub protocol: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub enabled: bool,
    pub official_verified: bool,
    pub preset_id: Option<String>,
    pub website_url: Option<String>,
    pub description: Option<String>,
    pub models: Vec<DesktopProviderModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderDeleteResult {
    pub deleted: bool,
    pub provider_id: String,
    #[serde(default)]
    pub runtime_target: DesktopProviderRuntimeTarget,
    pub live_config_removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderSyncResult {
    pub provider_id: String,
    #[serde(default)]
    pub runtime_target: DesktopProviderRuntimeTarget,
    pub config_path: String,
    pub auth_path: Option<String>,
    pub model_count: usize,
    pub primary_applied: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopOpenClawDefaultModel {
    pub primary: Option<String>,
    pub fallbacks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopOpenClawLiveProvider {
    pub id: String,
    pub base_url: String,
    pub protocol: String,
    pub model_count: usize,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopOpenClawRuntimeState {
    pub config_path: String,
    pub live_provider_ids: Vec<String>,
    pub live_providers: Vec<DesktopOpenClawLiveProvider>,
    pub default_model: DesktopOpenClawDefaultModel,
    pub model_catalog_count: usize,
    pub env: BTreeMap<String, String>,
    pub env_keys: Vec<String>,
    pub tools: Value,
    pub tool_keys: Vec<String>,
    pub health_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopOpenClawConfigWriteResult {
    pub config_path: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexLiveProvider {
    pub id: String,
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub wire_api: Option<String>,
    pub requires_openai_auth: bool,
    pub model: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexRuntimeState {
    pub config_dir: String,
    pub auth_path: String,
    pub config_path: String,
    pub active_provider_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub provider_count: usize,
    pub has_api_key: bool,
    pub has_chatgpt_tokens: bool,
    pub auth_mode: Option<String>,
    pub auth_profile_label: Option<String>,
    pub auth_plan_type: Option<String>,
    pub live_providers: Vec<DesktopCodexLiveProvider>,
    pub health_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConnectionTestInput {
    pub id: Option<String>,
    pub protocol: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProviderConnectionStatus {
    Success,
    Warning,
    AuthError,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConnectionTestResult {
    pub status: DesktopProviderConnectionStatus,
    pub checked_url: String,
    pub http_status: Option<u16>,
    pub message: String,
    pub response_excerpt: Option<String>,
    pub used_stored_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProviderHubStore {
    version: u32,
    providers: Vec<StoredManagedProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredManagedProvider {
    id: String,
    name: String,
    #[serde(default)]
    runtime_target: DesktopProviderRuntimeTarget,
    category: String,
    provider_type: String,
    billing_category: String,
    protocol: String,
    base_url: String,
    api_key: Option<String>,
    enabled: bool,
    official_verified: bool,
    preset_id: Option<String>,
    website_url: Option<String>,
    description: Option<String>,
    models: Vec<DesktopProviderModel>,
    created_at_epoch: i64,
    updated_at_epoch: i64,
}

#[derive(Debug, Clone)]
struct CodexConfigSnapshot {
    active_provider_key: Option<String>,
    model: Option<String>,
    providers: Vec<CodexLiveProviderEntry>,
}

#[derive(Debug, Clone)]
struct CodexLiveProviderEntry {
    key: String,
    name: Option<String>,
    base_url: Option<String>,
    wire_api: Option<String>,
    requires_openai_auth: bool,
    model: Option<String>,
    is_active: bool,
}

impl StoredManagedProvider {
    fn into_public(self) -> DesktopManagedProvider {
        let api_key_masked = self.api_key.as_deref().map(mask_api_key);
        let has_api_key = self
            .api_key
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        DesktopManagedProvider {
            id: self.id,
            name: self.name,
            runtime_target: self.runtime_target,
            category: self.category,
            provider_type: self.provider_type,
            billing_category: self.billing_category,
            protocol: self.protocol,
            base_url: self.base_url,
            api_key_masked,
            has_api_key,
            enabled: self.enabled,
            official_verified: self.official_verified,
            preset_id: self.preset_id,
            website_url: self.website_url,
            description: self.description,
            models: self.models,
            created_at_epoch: self.created_at_epoch,
            updated_at_epoch: self.updated_at_epoch,
        }
    }
}

pub fn provider_presets() -> Vec<DesktopProviderPreset> {
    vec![
        preset(
            "deepseek",
            "DeepSeek",
            DesktopProviderRuntimeTarget::OpenClaw,
            "cn_official",
            "deepseek",
            "official",
            "openai-completions",
            "https://api.deepseek.com/v1",
            true,
            Some("https://platform.deepseek.com"),
            Some("DeepSeek 官方兼容接口，适合通用与推理模型接入。"),
            Some("deepseek"),
            Some("#1E88E5"),
            vec![
                model(
                    "deepseek-chat",
                    "DeepSeek V3.2",
                    Some(64000),
                    Some(8192),
                    Some("paid"),
                    &["general", "coding"],
                ),
                model(
                    "deepseek-reasoner",
                    "DeepSeek R1",
                    Some(64000),
                    Some(8192),
                    Some("paid"),
                    &["reasoning"],
                ),
            ],
        ),
        preset(
            "zhipu-glm",
            "Zhipu GLM",
            DesktopProviderRuntimeTarget::OpenClaw,
            "cn_official",
            "zhipu",
            "official",
            "openai-completions",
            "https://open.bigmodel.cn/api/paas/v4",
            true,
            Some("https://open.bigmodel.cn"),
            Some("智谱开放平台官方兼容接口。"),
            Some("zhipu"),
            Some("#0F62FE"),
            vec![model(
                "glm-5",
                "GLM-5",
                Some(128000),
                Some(8192),
                Some("paid"),
                &["general", "coding"],
            )],
        ),
        preset(
            "qwen-coder",
            "Qwen Coder",
            DesktopProviderRuntimeTarget::OpenClaw,
            "cn_official",
            "qwen",
            "official",
            "openai-completions",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            true,
            Some("https://bailian.console.aliyun.com"),
            Some("阿里云百炼兼容接口，适合 Qwen 系列编码模型。"),
            Some("qwen"),
            Some("#FF6A00"),
            vec![model(
                "qwen3.5-plus",
                "Qwen3.5 Plus",
                Some(32000),
                Some(8192),
                Some("paid"),
                &["coding", "general"],
            )],
        ),
        preset(
            "kimi-k2-5",
            "Kimi K2.5",
            DesktopProviderRuntimeTarget::OpenClaw,
            "cn_official",
            "kimi",
            "official",
            "openai-completions",
            "https://api.moonshot.cn/v1",
            true,
            Some("https://platform.moonshot.cn/console"),
            Some("Moonshot 官方兼容接口。"),
            Some("kimi"),
            Some("#6366F1"),
            vec![model(
                "kimi-k2.5",
                "Kimi K2.5",
                Some(131072),
                Some(8192),
                Some("paid"),
                &["general", "coding"],
            )],
        ),
        preset(
            "stepfun",
            "StepFun",
            DesktopProviderRuntimeTarget::OpenClaw,
            "cn_official",
            "stepfun",
            "official",
            "openai-completions",
            "https://api.stepfun.ai/v1",
            true,
            Some("https://platform.stepfun.ai"),
            Some("阶跃星辰兼容接口。"),
            Some("stepfun"),
            Some("#005AFF"),
            vec![model(
                "step-3.5-flash",
                "Step 3.5 Flash",
                Some(128000),
                Some(8192),
                Some("paid"),
                &["general", "fast"],
            )],
        ),
        preset(
            "minimax",
            "MiniMax",
            DesktopProviderRuntimeTarget::OpenClaw,
            "cn_official",
            "minimax",
            "official",
            "openai-completions",
            "https://api.minimax.chat/v1",
            true,
            Some("https://platform.minimaxi.com"),
            Some("MiniMax 官方兼容接口。"),
            Some("minimax"),
            Some("#22C55E"),
            vec![model(
                "minimax-text-01",
                "MiniMax Text 01",
                Some(200000),
                Some(8192),
                Some("paid"),
                &["general"],
            )],
        ),
        preset(
            "openrouter-official",
            "OpenRouter Official API",
            DesktopProviderRuntimeTarget::OpenClaw,
            "aggregator",
            "openrouter",
            "mixed",
            "openai-completions",
            "https://openrouter.ai/api/v1",
            true,
            Some("https://openrouter.ai"),
            Some("OpenRouter 官方聚合入口，适合快速接入多模型目录。"),
            Some("openrouter"),
            Some("#8B5CF6"),
            vec![model(
                "openai/gpt-oss-20b:free",
                "GPT-OSS 20B (free)",
                Some(262144),
                Some(262144),
                Some("free"),
                &["general", "reasoning"],
            )],
        ),
        preset(
            "codex-openai",
            "OpenAI",
            DesktopProviderRuntimeTarget::Codex,
            "official",
            "codex_openai",
            "official",
            "openai-responses",
            "https://api.openai.com/v1",
            true,
            Some("https://platform.openai.com"),
            Some("OpenAI 官方服务，仅支持通过 Codex 登录态同步到 ~/.codex 配置。"),
            Some("openai"),
            Some("#00A67E"),
            vec![
                model(
                    "gpt-5",
                    "GPT 5",
                    Some(200000),
                    Some(16384),
                    Some("paid"),
                    &["general", "coding", "reasoning"],
                ),
                model(
                    "gpt-5-mini",
                    "GPT 5 Mini",
                    Some(200000),
                    Some(16384),
                    Some("paid"),
                    &["general", "coding"],
                ),
                model(
                    "gpt-5-nano",
                    "GPT 5 Nano",
                    Some(200000),
                    Some(16384),
                    Some("paid"),
                    &["general"],
                ),
                model(
                    "gpt-5-pro",
                    "GPT 5 Pro",
                    Some(200000),
                    Some(16384),
                    Some("paid"),
                    &["reasoning", "coding"],
                ),
                model(
                    "gpt-5-chat",
                    "GPT 5 Chat",
                    Some(200000),
                    Some(16384),
                    Some("paid"),
                    &["general"],
                ),
                model(
                    "gpt-5.1",
                    "GPT 5.1",
                    Some(200000),
                    Some(16384),
                    Some("paid"),
                    &["general", "coding", "reasoning"],
                ),
                model(
                    "gpt-image-1",
                    "GPT Image",
                    Some(32000),
                    Some(8192),
                    Some("paid"),
                    &["image"],
                ),
            ],
        ),
        preset(
            "codex-azure-openai",
            "Codex Azure OpenAI",
            DesktopProviderRuntimeTarget::Codex,
            "official",
            "azure_openai_codex",
            "official",
            "openai-responses",
            "https://YOUR_RESOURCE_NAME.openai.azure.com/openai",
            true,
            Some("https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/codex"),
            Some("Azure OpenAI Codex 兼容配置，会保留并写入 Codex 所需的 query params。"),
            Some("azure"),
            Some("#0078D4"),
            vec![model(
                "gpt-5.4",
                "GPT-5.4",
                Some(200000),
                Some(16384),
                Some("paid"),
                &["coding", "reasoning"],
            )],
        ),
        preset(
            "codex-aihubmix",
            "Codex AiHubMix",
            DesktopProviderRuntimeTarget::Codex,
            "aggregator",
            "codex_aihubmix",
            "mixed",
            "openai-responses",
            "https://aihubmix.com/v1",
            false,
            Some("https://aihubmix.com"),
            Some("从 clawhub123 迁移的 Codex 聚合通道预设。"),
            Some("generic"),
            Some("#2563EB"),
            vec![model(
                "gpt-5.4",
                "GPT-5.4",
                Some(200000),
                Some(16384),
                Some("paid"),
                &["coding", "reasoning"],
            )],
        ),
        preset(
            "codex-dmxapi",
            "Codex DMXAPI",
            DesktopProviderRuntimeTarget::Codex,
            "third_party",
            "codex_dmxapi",
            "mixed",
            "openai-responses",
            "https://www.dmxapi.cn/v1",
            false,
            Some("https://www.dmxapi.cn"),
            Some("从 clawhub123 迁移的 DMXAPI Codex 预设。"),
            Some("generic"),
            Some("#7C3AED"),
            vec![model(
                "gpt-5.4",
                "GPT-5.4",
                Some(200000),
                Some(16384),
                Some("paid"),
                &["coding", "reasoning"],
            )],
        ),
        preset(
            "codex-openrouter",
            "Codex OpenRouter",
            DesktopProviderRuntimeTarget::Codex,
            "aggregator",
            "codex_openrouter",
            "mixed",
            "openai-responses",
            "https://openrouter.ai/api/v1",
            true,
            Some("https://openrouter.ai"),
            Some("OpenRouter 的 Codex Responses 兼容入口。"),
            Some("openrouter"),
            Some("#6566F1"),
            vec![model(
                "openai/gpt-5.4",
                "GPT-5.4",
                Some(200000),
                Some(16384),
                Some("paid"),
                &["coding", "reasoning"],
            )],
        ),
        preset(
            "custom-openai",
            "Custom OpenAI Compatible",
            DesktopProviderRuntimeTarget::OpenClaw,
            "custom",
            "custom_gateway",
            "custom",
            "openai-completions",
            "https://api.example.com/v1",
            false,
            None,
            Some("接入任意 OpenAI-compatible 服务。"),
            Some("generic"),
            Some("#6B7280"),
            vec![model(
                "custom-model",
                "Custom Model",
                Some(128000),
                Some(8192),
                None,
                &["general"],
            )],
        ),
        preset(
            "custom-responses",
            "Custom Responses API",
            DesktopProviderRuntimeTarget::OpenClaw,
            "custom",
            "custom_responses",
            "custom",
            "openai-responses",
            "https://api.example.com/v1",
            false,
            None,
            Some("接入兼容 OpenAI Responses API 的推理与编码服务。"),
            Some("generic"),
            Some("#64748B"),
            vec![model(
                "gpt-5.2-codex",
                "GPT-5.2 Codex",
                Some(128000),
                Some(8192),
                None,
                &["coding", "reasoning"],
            )],
        ),
        preset(
            "custom-codex",
            "Custom Codex Provider",
            DesktopProviderRuntimeTarget::Codex,
            "custom",
            "codex_custom_gateway",
            "custom",
            "openai-responses",
            "https://api.example.com/v1",
            false,
            None,
            Some("接入任意兼容 OpenAI Responses 的 Codex provider，并同步到 ~/.codex。"),
            Some("codex"),
            Some("#111827"),
            vec![model(
                "gpt-5.4",
                "GPT-5.4",
                Some(200000),
                Some(16384),
                None,
                &["coding", "reasoning"],
            )],
        ),
    ]
}

pub fn list_managed_providers(project_path: &str) -> Result<Vec<DesktopManagedProvider>, String> {
    let mut providers = read_provider_store(project_path)?
        .providers
        .into_iter()
        .map(StoredManagedProvider::into_public)
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| {
        right
            .updated_at_epoch
            .cmp(&left.updated_at_epoch)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(providers)
}

pub fn upsert_managed_provider(
    project_path: &str,
    input: DesktopManagedProviderUpsertInput,
) -> Result<DesktopManagedProvider, String> {
    let mut store = read_provider_store(project_path)?;
    let now = now_unix_i64();
    let provider_id = allocate_provider_id(&store.providers, input.id.as_deref(), &input.name);
    let normalized_name = normalize_required_text(&input.name, "provider name")?;
    let normalized_category = normalize_required_text(&input.category, "provider category")?;
    let normalized_provider_type = normalize_required_text(&input.provider_type, "provider type")?;
    let normalized_billing_category =
        normalize_required_text(&input.billing_category, "billing category")?;
    let normalized_protocol = normalize_required_text(&input.protocol, "protocol")?;
    let normalized_base_url = normalize_base_url(&input.base_url)?;
    let next_models = normalize_models(input.models)?;
    let index = store
        .providers
        .iter()
        .position(|provider| provider.id == provider_id);
    let existing = index.and_then(|one| store.providers.get(one).cloned());

    let next_api_key = match (
        input.api_key,
        existing
            .as_ref()
            .and_then(|provider| provider.api_key.clone()),
    ) {
        (Some(value), _) if value.trim().is_empty() => None,
        (Some(value), _) => Some(value.trim().to_string()),
        (None, existing_api_key) => existing_api_key,
    };

    let next = StoredManagedProvider {
        id: provider_id.clone(),
        name: normalized_name,
        runtime_target: input.runtime_target,
        category: normalized_category,
        provider_type: normalized_provider_type,
        billing_category: normalized_billing_category,
        protocol: normalized_protocol,
        base_url: normalized_base_url,
        api_key: next_api_key,
        enabled: input.enabled,
        official_verified: input.official_verified,
        preset_id: normalize_optional_text(input.preset_id.as_deref()),
        website_url: normalize_optional_text(input.website_url.as_deref()),
        description: normalize_optional_text(input.description.as_deref()),
        models: next_models,
        created_at_epoch: existing
            .as_ref()
            .map(|provider| provider.created_at_epoch)
            .unwrap_or(now),
        updated_at_epoch: now,
    };

    if let Some(index) = index {
        store.providers[index] = next.clone();
    } else {
        store.providers.push(next.clone());
    }
    write_provider_store(project_path, &store)?;
    Ok(next.into_public())
}

pub fn delete_managed_provider(
    project_path: &str,
    provider_id: &str,
) -> Result<DesktopProviderDeleteResult, String> {
    let normalized = normalize_required_text(provider_id, "provider id")?;
    let mut store = read_provider_store(project_path)?;
    let removed_provider = store
        .providers
        .iter()
        .find(|provider| provider.id == normalized)
        .cloned()
        .ok_or_else(|| format!("managed provider not found: {normalized}"))?;
    let before_len = store.providers.len();
    store.providers.retain(|provider| provider.id != normalized);
    if store.providers.len() == before_len {
        return Err(format!("managed provider not found: {normalized}"));
    }
    write_provider_store(project_path, &store)?;
    let live_config_removed = match removed_provider.runtime_target {
        DesktopProviderRuntimeTarget::OpenClaw => {
            remove_provider_from_openclaw_config(normalized.as_str())?;
            true
        }
        DesktopProviderRuntimeTarget::Codex => false,
    };
    Ok(DesktopProviderDeleteResult {
        deleted: true,
        provider_id: normalized,
        runtime_target: removed_provider.runtime_target,
        live_config_removed,
    })
}

pub fn sync_provider_to_runtime(
    project_path: &str,
    provider_id: &str,
    set_primary: bool,
) -> Result<DesktopProviderSyncResult, String> {
    let normalized = normalize_required_text(provider_id, "provider id")?;
    let store = read_provider_store(project_path)?;
    let provider = store
        .providers
        .into_iter()
        .find(|one| one.id == normalized)
        .ok_or_else(|| format!("managed provider not found: {normalized}"))?;

    if provider.models.is_empty() {
        return Err("provider has no models configured".to_string());
    }

    match provider.runtime_target {
        DesktopProviderRuntimeTarget::OpenClaw => sync_provider_to_openclaw(provider, set_primary),
        DesktopProviderRuntimeTarget::Codex => sync_provider_to_codex(provider),
    }
}

fn sync_provider_to_openclaw(
    provider: StoredManagedProvider,
    set_primary: bool,
) -> Result<DesktopProviderSyncResult, String> {
    let (path, mut config) = read_openclaw_config(None)?;
    let providers = ensure_providers_mut(&mut config)?;
    let model_values = provider
        .models
        .iter()
        .map(|model| {
            let mut payload = json!({
                "id": model.model_id,
                "name": model.display_name,
                "api": provider.protocol,
                "input": ["text", "image"],
            });
            if let Some(context_window) = model.context_window {
                payload["contextWindow"] = Value::Number(context_window.into());
            }
            if let Some(max_output_tokens) = model.max_output_tokens {
                payload["maxTokens"] = Value::Number(max_output_tokens.into());
            }
            payload
        })
        .collect::<Vec<_>>();
    providers.insert(
        provider.id.clone(),
        json!({
            "baseUrl": provider.base_url,
            "apiKey": provider.api_key,
            "api": provider.protocol,
            "auth": if provider.api_key.as_ref().map(|value| !value.trim().is_empty()).unwrap_or(false) {
                Value::String("api-key".to_string())
            } else {
                Value::Null
            },
            "authHeader": provider.api_key.as_ref().map(|value| !value.trim().is_empty()).unwrap_or(false),
            "models": model_values,
        }),
    );

    let agents_defaults = ensure_agents_defaults_mut(&mut config);
    let model_catalog = ensure_child_object_mut(agents_defaults, "models");
    for model in &provider.models {
        model_catalog.insert(format!("{}/{}", provider.id, model.model_id), json!({}));
    }

    let mut primary_applied = None;
    if set_primary {
        let model_config = ensure_child_object_mut(agents_defaults, "model");
        let primary = format!("{}/{}", provider.id, provider.models[0].model_id);
        model_config.insert("primary".to_string(), Value::String(primary.clone()));
        primary_applied = Some(primary);
    }

    write_openclaw_config(&path, &config)?;
    Ok(DesktopProviderSyncResult {
        provider_id: provider.id,
        runtime_target: DesktopProviderRuntimeTarget::OpenClaw,
        config_path: path.display().to_string(),
        auth_path: None,
        model_count: provider.models.len(),
        primary_applied,
    })
}

pub fn import_providers_from_openclaw_live(
    project_path: &str,
    selected_provider_ids: Option<Vec<String>>,
) -> Result<Vec<DesktopManagedProvider>, String> {
    let (_, config) = read_openclaw_config(None)?;
    let live_providers = read_live_provider_values(&config);
    let selected = selected_provider_ids.map(|ids| {
        ids.into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
    });

    let mut store = read_provider_store(project_path)?;
    let now = now_unix_i64();
    let mut imported = Vec::new();
    for (provider_id, value) in live_providers {
        if let Some(selected) = selected.as_ref() {
            if !selected.iter().any(|one| one == &provider_id) {
                continue;
            }
        }
        let normalized_id = normalize_required_text(&provider_id, "live provider id")?;
        let object = value
            .as_object()
            .ok_or_else(|| format!("openclaw live provider `{normalized_id}` is not an object"))?;
        let base_url = object
            .get("baseUrl")
            .and_then(Value::as_str)
            .unwrap_or("https://api.example.com/v1");
        let protocol = object
            .get("api")
            .and_then(Value::as_str)
            .unwrap_or("openai-completions");
        let api_key = object
            .get("apiKey")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let models = object
            .get("models")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(parse_live_model)
                    .collect::<Vec<DesktopProviderModel>>()
            })
            .unwrap_or_default();

        let index = store
            .providers
            .iter()
            .position(|provider| provider.id == normalized_id);
        let existing = index.and_then(|one| store.providers.get(one).cloned());
        let next = StoredManagedProvider {
            id: normalized_id.clone(),
            name: title_case_provider_name(normalized_id.as_str()),
            runtime_target: existing
                .as_ref()
                .map(|provider| provider.runtime_target)
                .unwrap_or(DesktopProviderRuntimeTarget::OpenClaw),
            category: existing
                .as_ref()
                .map(|provider| provider.category.clone())
                .unwrap_or_else(|| "custom".to_string()),
            provider_type: existing
                .as_ref()
                .map(|provider| provider.provider_type.clone())
                .unwrap_or_else(|| "openclaw_live".to_string()),
            billing_category: existing
                .as_ref()
                .map(|provider| provider.billing_category.clone())
                .unwrap_or_else(|| "imported".to_string()),
            protocol: protocol.trim().to_string(),
            base_url: normalize_base_url(base_url)?,
            api_key: api_key.or_else(|| {
                existing
                    .as_ref()
                    .and_then(|provider| provider.api_key.clone())
            }),
            enabled: existing
                .as_ref()
                .map(|provider| provider.enabled)
                .unwrap_or(true),
            official_verified: existing
                .as_ref()
                .map(|provider| provider.official_verified)
                .unwrap_or(false),
            preset_id: existing
                .as_ref()
                .and_then(|provider| provider.preset_id.clone()),
            website_url: existing
                .as_ref()
                .and_then(|provider| provider.website_url.clone()),
            description: Some("Imported from OpenClaw live config.".to_string()),
            models: normalize_models(models)?,
            created_at_epoch: existing
                .as_ref()
                .map(|provider| provider.created_at_epoch)
                .unwrap_or(now),
            updated_at_epoch: now,
        };

        if let Some(index) = index {
            store.providers[index] = next.clone();
        } else {
            store.providers.push(next.clone());
        }
        imported.push(next.into_public());
    }
    write_provider_store(project_path, &store)?;
    imported.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(imported)
}

pub fn import_providers_from_codex_live(
    project_path: &str,
    selected_provider_ids: Option<Vec<String>>,
) -> Result<Vec<DesktopManagedProvider>, String> {
    let config_path = resolve_codex_config_dir().join(DEFAULT_CODEX_CONFIG_FILE);
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let snapshot = load_codex_config_snapshot(&config_path)?;
    let selected = selected_provider_ids.map(|ids| {
        ids.into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
    });

    let mut store = read_provider_store(project_path)?;
    let now = now_unix_i64();
    let mut imported = Vec::new();
    for provider in snapshot.providers {
        if let Some(selected) = selected.as_ref() {
            if !selected.iter().any(|one| one == &provider.key) {
                continue;
            }
        }
        let normalized_id = normalize_required_text(&provider.key, "codex provider id")?;
        let index = store
            .providers
            .iter()
            .position(|existing| existing.id == normalized_id);
        let existing = index.and_then(|one| store.providers.get(one).cloned());
        if existing
            .as_ref()
            .map(|item| item.runtime_target != DesktopProviderRuntimeTarget::Codex)
            .unwrap_or(false)
        {
            return Err(format!(
                "managed provider id `{normalized_id}` already exists for another runtime"
            ));
        }

        let next = StoredManagedProvider {
            id: normalized_id.clone(),
            name: provider
                .name
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| title_case_provider_name(normalized_id.as_str())),
            runtime_target: DesktopProviderRuntimeTarget::Codex,
            category: existing
                .as_ref()
                .map(|item| item.category.clone())
                .unwrap_or_else(|| "custom".to_string()),
            provider_type: existing
                .as_ref()
                .map(|item| item.provider_type.clone())
                .unwrap_or_else(|| normalized_id.clone()),
            billing_category: existing
                .as_ref()
                .map(|item| item.billing_category.clone())
                .unwrap_or_else(|| "imported".to_string()),
            protocol: provider
                .wire_api
                .as_deref()
                .map(codex_wire_api_to_protocol)
                .unwrap_or("openai-responses")
                .to_string(),
            base_url: provider
                .base_url
                .as_deref()
                .map(normalize_base_url)
                .transpose()?
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            api_key: existing.as_ref().and_then(|item| item.api_key.clone()),
            enabled: existing.as_ref().map(|item| item.enabled).unwrap_or(true),
            official_verified: existing
                .as_ref()
                .map(|item| item.official_verified)
                .unwrap_or(false),
            preset_id: existing.as_ref().and_then(|item| item.preset_id.clone()),
            website_url: existing.as_ref().and_then(|item| item.website_url.clone()),
            description: existing
                .as_ref()
                .and_then(|item| item.description.clone())
                .or_else(|| {
                    Some(
                        "Imported from ~/.codex/config.toml. Save and refine models before syncing back if needed."
                            .to_string(),
                    )
                }),
            models: provider
                .model
                .as_deref()
                .map(|model_id| {
                    vec![DesktopProviderModel {
                        model_id: model_id.to_string(),
                        display_name: model_id.to_string(),
                        context_window: None,
                        max_output_tokens: None,
                        billing_kind: None,
                        capability_tags: vec!["general".to_string()],
                    }]
                })
                .unwrap_or_default(),
            created_at_epoch: existing
                .as_ref()
                .map(|item| item.created_at_epoch)
                .unwrap_or(now),
            updated_at_epoch: now,
        };

        if let Some(index) = index {
            store.providers[index] = next.clone();
        } else {
            store.providers.push(next.clone());
        }
        imported.push(next.into_public());
    }
    write_provider_store(project_path, &store)?;
    imported.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(imported)
}

pub fn openclaw_runtime_state() -> Result<DesktopOpenClawRuntimeState, String> {
    let (config_path, config) = read_openclaw_config(None)?;
    let live_providers_map = read_live_provider_values(&config);
    let mut live_providers = live_providers_map
        .iter()
        .map(|(id, value)| {
            let object = value.as_object();
            let model_count = object
                .and_then(|item| item.get("models"))
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let has_api_key = object
                .and_then(|item| item.get("apiKey"))
                .and_then(Value::as_str)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            DesktopOpenClawLiveProvider {
                id: id.clone(),
                base_url: object
                    .and_then(|item| item.get("baseUrl"))
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string(),
                protocol: object
                    .and_then(|item| item.get("api"))
                    .and_then(Value::as_str)
                    .unwrap_or("openai-completions")
                    .to_string(),
                model_count,
                has_api_key,
            }
        })
        .collect::<Vec<_>>();
    live_providers.sort_by(|left, right| left.id.cmp(&right.id));
    let live_provider_ids = live_providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect::<Vec<_>>();
    let default_model = read_default_model(&config);
    let model_catalog_count = config
        .get("agents")
        .and_then(|value| value.get("defaults"))
        .and_then(|value| value.get("models"))
        .and_then(Value::as_object)
        .map_or(0, Map::len);
    let env = read_env_map(&config);
    let env_keys = sorted_object_keys(config.get("env"));
    let tools = config.get("tools").cloned().unwrap_or_else(|| json!({}));
    let tool_keys = sorted_object_keys(config.get("tools"));
    let health_warnings = scan_openclaw_health(&config, &live_provider_ids, &default_model);

    Ok(DesktopOpenClawRuntimeState {
        config_path: config_path.display().to_string(),
        live_provider_ids,
        live_providers,
        default_model,
        model_catalog_count,
        env,
        env_keys,
        tools,
        tool_keys,
        health_warnings,
    })
}

pub fn codex_runtime_state() -> Result<DesktopCodexRuntimeState, String> {
    let config_dir = resolve_codex_config_dir();
    let auth_path = config_dir.join(DEFAULT_CODEX_AUTH_FILE);
    let config_path = config_dir.join(DEFAULT_CODEX_CONFIG_FILE);
    let auth = read_auth_payload(None)?;
    let has_api_key = auth
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_chatgpt_tokens = has_chatgpt_tokens(&auth);
    let auth_mode = auth
        .get("auth_mode")
        .and_then(Value::as_str)
        .map(str::to_string);
    let auth_claims = auth
        .pointer("/tokens/id_token")
        .and_then(Value::as_str)
        .and_then(|jwt| parse_chatgpt_jwt_claims(jwt).ok());

    let mut active_provider_key = None;
    let mut model = None;
    let mut base_url = None;
    let mut live_providers = Vec::new();
    let mut warnings = Vec::new();

    if !config_path.exists() {
        warnings.push("Codex config.toml does not exist yet.".to_string());
    } else {
        let snapshot = load_codex_config_snapshot(&config_path)?;
        if snapshot.providers.is_empty()
            && snapshot.active_provider_key.is_none()
            && snapshot.model.is_none()
        {
            warnings.push("Codex config.toml is empty.".to_string());
        } else {
            active_provider_key = snapshot.active_provider_key.clone();
            model = snapshot.model.clone();
            base_url = snapshot
                .providers
                .iter()
                .find(|provider| provider.is_active)
                .and_then(|provider| provider.base_url.clone());
            live_providers = snapshot
                .providers
                .into_iter()
                .map(|provider| DesktopCodexLiveProvider {
                    id: provider.key,
                    name: provider.name,
                    base_url: provider.base_url,
                    wire_api: provider.wire_api,
                    requires_openai_auth: provider.requires_openai_auth,
                    model: provider.model,
                    is_active: provider.is_active,
                })
                .collect();
        }
    }

    if !auth_path.exists() {
        warnings.push("Codex auth.json does not exist yet.".to_string());
    }
    if active_provider_key.is_none() {
        warnings.push("Codex model_provider is not configured.".to_string());
    }
    if model.is_none() {
        warnings.push("Codex model is not configured.".to_string());
    }
    if !has_api_key && !has_chatgpt_tokens {
        warnings.push(
            "Codex credentials are missing. Add an API key or sign in with ChatGPT.".to_string(),
        );
    }
    if auth_mode.as_deref() == Some("chatgpt") && !has_chatgpt_tokens {
        warnings
            .push("Codex auth_mode is chatgpt but auth.json does not contain tokens.".to_string());
    }

    Ok(DesktopCodexRuntimeState {
        config_dir: config_dir.display().to_string(),
        auth_path: auth_path.display().to_string(),
        config_path: config_path.display().to_string(),
        active_provider_key,
        model,
        base_url,
        provider_count: live_providers.len(),
        has_api_key,
        has_chatgpt_tokens,
        auth_mode,
        auth_profile_label: auth_claims
            .as_ref()
            .and_then(|claims| claims.email.clone().or(claims.chatgpt_account_id.clone())),
        auth_plan_type: auth_claims
            .as_ref()
            .and_then(|claims| claims.chatgpt_plan_type.clone()),
        live_providers,
        health_warnings: warnings,
    })
}

fn load_codex_config_snapshot(config_path: &PathBuf) -> Result<CodexConfigSnapshot, String> {
    let config_text = fs::read_to_string(config_path).map_err(|error| {
        format!(
            "read codex config failed ({}): {error}",
            config_path.display()
        )
    })?;
    if config_text.trim().is_empty() {
        return Ok(CodexConfigSnapshot {
            active_provider_key: None,
            model: None,
            providers: Vec::new(),
        });
    }

    let document = config_text.parse::<DocumentMut>().map_err(|error| {
        format!(
            "parse codex config failed ({}): {error}",
            config_path.display()
        )
    })?;
    let active_provider_key = toml_string(document.get("model_provider"));
    let model = toml_string(document.get("model"));
    let providers = document
        .get("model_providers")
        .and_then(Item::as_table_like)
        .map(|table| {
            table
                .iter()
                .filter_map(|(key, item)| {
                    let table = item.as_table_like()?;
                    let base_url = table
                        .get("base_url")
                        .and_then(Item::as_value)
                        .and_then(toml_edit::Value::as_str)
                        .map(str::to_string);
                    let name = table
                        .get("name")
                        .and_then(Item::as_value)
                        .and_then(toml_edit::Value::as_str)
                        .map(str::to_string);
                    let wire_api = table
                        .get("wire_api")
                        .and_then(Item::as_value)
                        .and_then(toml_edit::Value::as_str)
                        .map(str::to_string);
                    let requires_openai_auth = table
                        .get("requires_openai_auth")
                        .and_then(Item::as_value)
                        .and_then(toml_edit::Value::as_bool)
                        .unwrap_or(false);
                    let is_active = active_provider_key.as_deref() == Some(key);
                    Some(CodexLiveProviderEntry {
                        key: key.to_string(),
                        name,
                        base_url,
                        wire_api,
                        requires_openai_auth,
                        model: is_active.then(|| model.clone()).flatten(),
                        is_active,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(CodexConfigSnapshot {
        active_provider_key,
        model,
        providers,
    })
}

fn codex_wire_api_to_protocol(wire_api: &str) -> &'static str {
    match wire_api.trim() {
        "responses" => "openai-responses",
        _ => "openai-completions",
    }
}

pub fn test_provider_connection(
    project_path: &str,
    input: DesktopProviderConnectionTestInput,
) -> Result<DesktopProviderConnectionTestResult, String> {
    let base_url = normalize_base_url(&input.base_url)?;
    let protocol = normalize_required_text(&input.protocol, "protocol")?;
    let provider_id = normalize_optional_text(input.id.as_deref());
    let (api_key, used_stored_api_key) =
        resolve_connection_test_api_key(project_path, &provider_id, input.api_key)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(4))
        .build()
        .map_err(|error| format!("create provider test client failed: {error}"))?;

    let mut last_result = None;
    for candidate_url in connection_probe_candidates(&base_url, &protocol) {
        let result = execute_connection_probe(
            &client,
            &candidate_url,
            &protocol,
            api_key.as_deref(),
            used_stored_api_key,
        );
        if matches!(
            result.status,
            DesktopProviderConnectionStatus::Success | DesktopProviderConnectionStatus::AuthError
        ) {
            return Ok(result);
        }
        last_result = Some(result);
    }

    Ok(last_result.unwrap_or(DesktopProviderConnectionTestResult {
        status: DesktopProviderConnectionStatus::Error,
        checked_url: base_url,
        http_status: None,
        message: "No probe candidates were generated for this provider.".to_string(),
        response_excerpt: None,
        used_stored_api_key,
    }))
}

pub fn set_openclaw_env(
    env: BTreeMap<String, String>,
) -> Result<DesktopOpenClawConfigWriteResult, String> {
    let (path, mut config) = read_openclaw_config(None)?;
    let current = read_env_map(&config);
    let changed = current != env;
    let env_object = env
        .into_iter()
        .filter_map(|(key, value)| {
            let normalized_key = key.trim().to_string();
            if normalized_key.is_empty() {
                return None;
            }
            Some((normalized_key, Value::String(value)))
        })
        .collect::<Map<String, Value>>();
    let root = ensure_object_mut(&mut config);
    root.insert("env".to_string(), Value::Object(env_object));
    if changed {
        write_openclaw_config(&path, &config)?;
    }
    Ok(DesktopOpenClawConfigWriteResult {
        config_path: path.display().to_string(),
        changed,
    })
}

pub fn set_openclaw_tools(tools: Value) -> Result<DesktopOpenClawConfigWriteResult, String> {
    if !tools.is_object() {
        return Err("tools payload must be a JSON object".to_string());
    }
    let (path, mut config) = read_openclaw_config(None)?;
    let current = config.get("tools").cloned().unwrap_or_else(|| json!({}));
    let changed = current != tools;
    let root = ensure_object_mut(&mut config);
    root.insert("tools".to_string(), tools);
    if changed {
        write_openclaw_config(&path, &config)?;
    }
    Ok(DesktopOpenClawConfigWriteResult {
        config_path: path.display().to_string(),
        changed,
    })
}

fn sync_provider_to_codex(
    provider: StoredManagedProvider,
) -> Result<DesktopProviderSyncResult, String> {
    let api_key = provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let config_dir = resolve_codex_config_dir();
    let auth_path = config_dir.join(DEFAULT_CODEX_AUTH_FILE);
    let config_path = config_dir.join(DEFAULT_CODEX_CONFIG_FILE);
    let existing_auth = read_auth_payload(None)?;
    let auth_json = if let Some(api_key) = api_key {
        build_codex_auth_json(&auth_path, api_key)?
    } else if has_chatgpt_tokens(&existing_auth) {
        existing_auth
    } else {
        return Err(
            "codex provider requires an API key or ChatGPT login before syncing".to_string(),
        );
    };
    let config_text = build_codex_config_text(&config_path, &provider)?;
    write_codex_live_atomic(&auth_json, &config_text, &auth_path, &config_path)?;

    Ok(DesktopProviderSyncResult {
        provider_id: provider.id,
        runtime_target: DesktopProviderRuntimeTarget::Codex,
        config_path: config_path.display().to_string(),
        auth_path: Some(auth_path.display().to_string()),
        model_count: provider.models.len(),
        primary_applied: None,
    })
}

fn build_codex_auth_json(auth_path: &PathBuf, api_key: &str) -> Result<Value, String> {
    let mut auth = read_codex_auth_json(auth_path)?;
    let object = ensure_object_mut(&mut auth);
    object.insert(
        "OPENAI_API_KEY".to_string(),
        Value::String(api_key.trim().to_string()),
    );
    Ok(auth)
}

fn build_codex_config_text(
    config_path: &PathBuf,
    provider: &StoredManagedProvider,
) -> Result<String, String> {
    let existing_text = if config_path.exists() {
        fs::read_to_string(config_path).map_err(|error| {
            format!(
                "read codex config failed ({}): {error}",
                config_path.display()
            )
        })?
    } else {
        String::new()
    };
    let mut document = if existing_text.trim().is_empty() {
        DocumentMut::new()
    } else {
        existing_text.parse::<DocumentMut>().map_err(|error| {
            format!(
                "parse codex config failed ({}): {error}",
                config_path.display()
            )
        })?
    };

    let provider_key = provider.id.as_str();
    let model_id = provider
        .models
        .first()
        .map(|model| model.model_id.as_str())
        .ok_or_else(|| "codex provider must define at least one model".to_string())?;
    document["model_provider"] = toml_value(provider_key);
    document["model"] = toml_value(model_id);
    document["model_reasoning_effort"] = toml_value("high");
    document["disable_response_storage"] = toml_value(true);

    if document.get("model_providers").is_none() {
        document["model_providers"] = toml_edit::table();
    }
    let model_providers = document["model_providers"]
        .as_table_mut()
        .ok_or_else(|| "codex model_providers must be a table".to_string())?;
    if !model_providers.contains_key(provider_key) {
        model_providers[provider_key] = toml_edit::table();
    }
    let provider_table = model_providers[provider_key]
        .as_table_mut()
        .ok_or_else(|| "codex provider entry must be a table".to_string())?;
    let codex_base_url = normalize_codex_base_url(&provider.base_url);
    provider_table["name"] = toml_value(provider.name.as_str());
    provider_table["base_url"] = toml_value(codex_base_url.as_str());
    provider_table["wire_api"] = toml_value("responses");
    provider_table["requires_openai_auth"] = toml_value(true);

    if is_azure_codex_provider(provider) {
        provider_table["env_key"] = toml_value("OPENAI_API_KEY");
        let mut inline_table = InlineTable::default();
        inline_table.insert("api-version", toml_edit::Value::from("2025-04-01-preview"));
        provider_table["query_params"] = toml_value(inline_table);
    } else {
        provider_table.remove("env_key");
        provider_table.remove("query_params");
    }

    Ok(document.to_string())
}

fn write_codex_live_atomic(
    auth: &Value,
    config_text: &str,
    auth_path: &PathBuf,
    config_path: &PathBuf,
) -> Result<(), String> {
    if let Some(parent) = auth_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create codex config directory failed ({}): {error}",
                parent.display()
            )
        })?;
    }

    let old_auth = if auth_path.exists() {
        Some(fs::read(auth_path).map_err(|error| {
            format!("read codex auth failed ({}): {error}", auth_path.display())
        })?)
    } else {
        None
    };
    let old_config = if config_path.exists() {
        Some(fs::read(config_path).map_err(|error| {
            format!(
                "read codex config failed ({}): {error}",
                config_path.display()
            )
        })?)
    } else {
        None
    };

    if !config_text.trim().is_empty() {
        config_text.parse::<DocumentMut>().map_err(|error| {
            format!(
                "validate codex config failed ({}): {error}",
                config_path.display()
            )
        })?;
    }
    let auth_payload = serde_json::to_string_pretty(auth)
        .map_err(|error| format!("serialize codex auth failed: {error}"))?;

    fs::write(auth_path, auth_payload)
        .map_err(|error| format!("write codex auth failed ({}): {error}", auth_path.display()))?;

    if let Err(error) = fs::write(config_path, config_text) {
        if let Some(bytes) = old_auth {
            let _ = fs::write(auth_path, bytes);
        } else {
            let _ = fs::remove_file(auth_path);
        }
        return Err(format!(
            "write codex config failed ({}): {error}",
            config_path.display()
        ));
    }

    if old_config.is_none() && config_text.is_empty() {
        let _ = fs::remove_file(config_path);
    }
    Ok(())
}

fn read_provider_store(project_path: &str) -> Result<ProviderHubStore, String> {
    let path = provider_hub_path(project_path);
    if !path.exists() {
        return Ok(ProviderHubStore {
            version: 1,
            providers: Vec::new(),
        });
    }
    let raw = fs::read_to_string(&path).map_err(|error| {
        format!(
            "read provider hub store failed ({}): {error}",
            path.display()
        )
    })?;
    let store = serde_json::from_str::<ProviderHubStore>(&raw).map_err(|error| {
        format!(
            "parse provider hub store failed ({}): {error}",
            path.display()
        )
    })?;
    Ok(store)
}

fn write_provider_store(project_path: &str, store: &ProviderHubStore) -> Result<(), String> {
    let path = provider_hub_path(project_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create provider hub directory failed ({}): {error}",
                parent.display()
            )
        })?;
    }
    let payload = serde_json::to_string_pretty(store)
        .map_err(|error| format!("serialize provider hub store failed: {error}"))?;
    fs::write(&path, payload).map_err(|error| {
        format!(
            "write provider hub store failed ({}): {error}",
            path.display()
        )
    })
}

fn provider_hub_path(project_path: &str) -> PathBuf {
    let cwd = Path::new(project_path);
    let loader = ConfigLoader::default_for(cwd);
    loader.config_home().join(PROVIDER_HUB_FILE)
}

fn preset(
    id: &str,
    name: &str,
    runtime_target: DesktopProviderRuntimeTarget,
    category: &str,
    provider_type: &str,
    billing_category: &str,
    protocol: &str,
    base_url: &str,
    official_verified: bool,
    website_url: Option<&str>,
    description: Option<&str>,
    icon: Option<&str>,
    icon_color: Option<&str>,
    models: Vec<DesktopProviderModel>,
) -> DesktopProviderPreset {
    DesktopProviderPreset {
        id: id.to_string(),
        name: name.to_string(),
        runtime_target,
        category: category.to_string(),
        provider_type: provider_type.to_string(),
        billing_category: billing_category.to_string(),
        protocol: protocol.to_string(),
        base_url: base_url.to_string(),
        official_verified,
        website_url: website_url.map(str::to_string),
        description: description.map(str::to_string),
        icon: icon.map(str::to_string),
        icon_color: icon_color.map(str::to_string),
        models,
    }
}

fn model(
    model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
    max_output_tokens: Option<i64>,
    billing_kind: Option<&str>,
    capability_tags: &[&str],
) -> DesktopProviderModel {
    DesktopProviderModel {
        model_id: model_id.to_string(),
        display_name: display_name.to_string(),
        context_window,
        max_output_tokens,
        billing_kind: billing_kind.map(str::to_string),
        capability_tags: capability_tags
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    }
}

fn normalize_models(
    models: Vec<DesktopProviderModel>,
) -> Result<Vec<DesktopProviderModel>, String> {
    models
        .into_iter()
        .map(|model| {
            Ok(DesktopProviderModel {
                model_id: normalize_required_text(&model.model_id, "model id")?,
                display_name: normalize_required_text(&model.display_name, "model display name")?,
                context_window: model.context_window.filter(|value| *value > 0),
                max_output_tokens: model.max_output_tokens.filter(|value| *value > 0),
                billing_kind: normalize_optional_text(model.billing_kind.as_deref()),
                capability_tags: model
                    .capability_tags
                    .into_iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect(),
            })
        })
        .collect()
}

fn normalize_required_text(input: &str, field_name: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(format!("{field_name} is required"));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_text(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn toml_string(item: Option<&Item>) -> Option<String> {
    item.and_then(Item::as_value)
        .and_then(toml_edit::Value::as_str)
        .map(str::to_string)
}

fn resolve_connection_test_api_key(
    project_path: &str,
    provider_id: &Option<String>,
    api_key: Option<String>,
) -> Result<(Option<String>, bool), String> {
    if let Some(api_key) = normalize_optional_text(api_key.as_deref()) {
        return Ok((Some(api_key), false));
    }
    let Some(provider_id) = provider_id else {
        return Ok((None, false));
    };
    let store = read_provider_store(project_path)?;
    let normalized_provider_id = normalize_required_text(provider_id, "provider id")?;
    let stored_provider = store
        .providers
        .into_iter()
        .find(|provider| provider.id == normalized_provider_id)
        .ok_or_else(|| format!("managed provider not found: {normalized_provider_id}"))?;
    let stored_api_key = normalize_optional_text(stored_provider.api_key.as_deref());
    Ok((stored_api_key.clone(), stored_api_key.is_some()))
}

fn normalize_base_url(input: &str) -> Result<String, String> {
    let normalized = normalize_required_text(input, "base url")?;
    let value = normalized.trim_end_matches('/').to_string();
    if value.starts_with("http://") || value.starts_with("https://") {
        Ok(value)
    } else {
        Err("base url must start with http:// or https://".to_string())
    }
}

fn normalize_codex_base_url(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        return trimmed.to_string();
    }

    match trimmed.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((_host, path)) if !path.trim_matches('/').is_empty() => trimmed.to_string(),
            _ => format!("{scheme}://{rest}/v1"),
        },
        None => trimmed.to_string(),
    }
}

fn is_azure_codex_provider(provider: &StoredManagedProvider) -> bool {
    provider.provider_type == "azure_openai_codex"
        || provider.preset_id.as_deref() == Some("codex-azure-openai")
}

fn connection_probe_candidates(base_url: &str, protocol: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let models_url = format!("{base_url}/models");
    candidates.push(models_url);
    if protocol == "google-generative-ai" {
        candidates.push(format!("{base_url}/models?alt=json"));
    }
    candidates.push(base_url.to_string());
    candidates.dedup();
    candidates
}

fn execute_connection_probe(
    client: &Client,
    candidate_url: &str,
    protocol: &str,
    api_key: Option<&str>,
    used_stored_api_key: bool,
) -> DesktopProviderConnectionTestResult {
    match build_probe_request(client, candidate_url, protocol, api_key)
        .and_then(|request| request.send().map_err(|error| error.to_string()))
    {
        Ok(response) => classify_probe_response(response, used_stored_api_key),
        Err(error) => DesktopProviderConnectionTestResult {
            status: DesktopProviderConnectionStatus::Error,
            checked_url: candidate_url.to_string(),
            http_status: None,
            message: classify_probe_error(&error),
            response_excerpt: None,
            used_stored_api_key,
        },
    }
}

fn build_probe_request(
    client: &Client,
    candidate_url: &str,
    protocol: &str,
    api_key: Option<&str>,
) -> Result<reqwest::blocking::RequestBuilder, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("warwolf-desktop-provider-test/1.0"),
    );
    if let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        let auth_value = HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|error| format!("invalid authorization header: {error}"))?;
        headers.insert(AUTHORIZATION, auth_value);
        insert_optional_header(&mut headers, "x-api-key", api_key)?;
        insert_optional_header(&mut headers, "api-key", api_key)?;
        insert_optional_header(&mut headers, "x-goog-api-key", api_key)?;
    }
    if protocol == "anthropic-messages" {
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
    }

    Ok(client.get(candidate_url).headers(headers))
}

fn insert_optional_header(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), String> {
    let header_name = HeaderName::from_static(name);
    let header_value =
        HeaderValue::from_str(value).map_err(|error| format!("invalid {name} header: {error}"))?;
    headers.insert(header_name, header_value);
    Ok(())
}

fn classify_probe_response(
    response: Response,
    used_stored_api_key: bool,
) -> DesktopProviderConnectionTestResult {
    let status = response.status();
    let checked_url = response.url().to_string();
    let body_excerpt = read_response_excerpt(response);
    let (probe_status, message) = if status.is_success() {
        (
            DesktopProviderConnectionStatus::Success,
            format!("Connection succeeded with HTTP {}.", status.as_u16()),
        )
    } else if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        (
            DesktopProviderConnectionStatus::AuthError,
            format!(
                "Endpoint is reachable, but authentication failed with HTTP {}.",
                status.as_u16()
            ),
        )
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        (
            DesktopProviderConnectionStatus::Warning,
            "Endpoint is reachable, but the provider is rate limiting this probe.".to_string(),
        )
    } else {
        (
            DesktopProviderConnectionStatus::Warning,
            format!(
                "Endpoint responded with HTTP {}. Review the base URL, protocol, or provider-specific requirements.",
                status.as_u16()
            ),
        )
    };

    DesktopProviderConnectionTestResult {
        status: probe_status,
        checked_url,
        http_status: Some(status.as_u16()),
        message,
        response_excerpt: body_excerpt,
        used_stored_api_key,
    }
}

fn read_response_excerpt(response: Response) -> Option<String> {
    response
        .text()
        .ok()
        .and_then(|body| normalize_response_excerpt(&body))
}

fn normalize_response_excerpt(body: &str) -> Option<String> {
    let condensed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = condensed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut excerpt = trimmed.chars().take(240).collect::<String>();
    if trimmed.chars().count() > 240 {
        excerpt.push_str("...");
    }
    Some(excerpt)
}

fn classify_probe_error(error: &str) -> String {
    let lowercase = error.to_ascii_lowercase();
    if lowercase.contains("timed out") {
        "Connection timed out while probing the provider endpoint.".to_string()
    } else if lowercase.contains("dns")
        || lowercase.contains("failed to lookup address")
        || lowercase.contains("name or service not known")
    {
        "DNS lookup failed. Check the provider base URL hostname.".to_string()
    } else if lowercase.contains("certificate") || lowercase.contains("tls") {
        "TLS handshake failed. Check the provider certificate or HTTPS interception.".to_string()
    } else {
        format!("Connection probe failed: {error}")
    }
}

fn allocate_provider_id(
    providers: &[StoredManagedProvider],
    explicit_id: Option<&str>,
    name: &str,
) -> String {
    if let Some(explicit_id) = explicit_id.map(str::trim).filter(|value| !value.is_empty()) {
        return normalize_provider_id(explicit_id);
    }

    let base_id = normalize_provider_id(name);
    if !providers.iter().any(|provider| provider.id == base_id) {
        return base_id;
    }
    let mut suffix = 2_u32;
    loop {
        let candidate = format!("{base_id}-{suffix}");
        if !providers.iter().any(|provider| provider.id == candidate) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

fn normalize_provider_id(input: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;
    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            output.push('-');
            previous_dash = true;
        }
    }
    let trimmed = output.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "provider".to_string()
    } else {
        trimmed
    }
}

fn now_unix_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn mask_api_key(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 8 {
        return "********".to_string();
    }
    let prefix = &trimmed[..4];
    let suffix = &trimmed[trimmed.len() - 4..];
    format!("{prefix}********{suffix}")
}

fn resolve_codex_config_dir() -> PathBuf {
    if let Ok(value) = std::env::var("CODEX_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    current_home_dir().join(".codex")
}

fn read_codex_auth_json(auth_path: &PathBuf) -> Result<Value, String> {
    if !auth_path.exists() {
        return Ok(json!({}));
    }
    let raw = fs::read_to_string(auth_path)
        .map_err(|error| format!("read codex auth failed ({}): {error}", auth_path.display()))?;
    let parsed = serde_json::from_str::<Value>(&raw)
        .map_err(|error| format!("parse codex auth failed ({}): {error}", auth_path.display()))?;
    if parsed.is_object() {
        Ok(parsed)
    } else {
        Err(format!(
            "codex auth root must be a JSON object ({})",
            auth_path.display()
        ))
    }
}

fn current_home_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .or_else(|_| std::env::var("LOCALAPPDATA"))
            .unwrap_or_else(|_| ".".to_string()),
    )
}

fn resolve_openclaw_config_target(explicit_path: Option<&str>) -> (PathBuf, bool) {
    if let Some(path) = explicit_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return (PathBuf::from(path), false);
    }
    for env_key in ["CLAWHUB_OPENCLAW_CONFIG_FILE", "OPENCLAW_CONFIG_FILE"] {
        if let Ok(value) = std::env::var(env_key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return (PathBuf::from(trimmed), false);
            }
        }
    }
    if let Ok(value) = std::env::var("OPENCLAW_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return (implicit_openclaw_config_path(&PathBuf::from(trimmed)), true);
        }
    }
    let config_dir = current_home_dir().join(".openclaw");
    (implicit_openclaw_config_path(&config_dir), true)
}

fn implicit_openclaw_config_path(config_dir: &PathBuf) -> PathBuf {
    let primary = config_dir.join(DEFAULT_OPENCLAW_CONFIG_FILE);
    if primary.exists() {
        return primary;
    }
    let alternate = config_dir.join(ALTERNATE_OPENCLAW_CONFIG_FILE);
    if alternate.exists() {
        return alternate;
    }
    primary
}

fn read_openclaw_config(explicit_path: Option<&str>) -> Result<(PathBuf, Value), String> {
    let (path, allow_legacy_fallback) = resolve_openclaw_config_target(explicit_path);
    if path.exists() {
        return Ok((path.clone(), parse_openclaw_config(&path)?));
    }
    if allow_legacy_fallback
        && path.file_name().and_then(|value| value.to_str()) == Some(DEFAULT_OPENCLAW_CONFIG_FILE)
    {
        let alternate = path.with_file_name(ALTERNATE_OPENCLAW_CONFIG_FILE);
        if alternate.exists() {
            return Ok((path, parse_openclaw_config(&alternate)?));
        }
    }
    Ok((
        path,
        json!({ "models": { "mode": "merge", "providers": {} } }),
    ))
}

fn parse_openclaw_config(path: &PathBuf) -> Result<Value, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("read openclaw config failed ({}): {error}", path.display()))?;
    let parsed = json5::from_str::<Value>(&raw)
        .map_err(|error| format!("parse openclaw config failed ({}): {error}", path.display()))?;
    if !parsed.is_object() {
        return Err(format!(
            "openclaw config root must be an object ({})",
            path.display()
        ));
    }
    Ok(parsed)
}

fn write_openclaw_config(path: &PathBuf, config: &Value) -> Result<(), String> {
    if !config.is_object() {
        return Err("openclaw config root must be an object".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create openclaw config directory failed ({}): {error}",
                parent.display()
            )
        })?;
    }
    let payload = serde_json::to_string_pretty(config)
        .map_err(|error| format!("serialize openclaw config failed: {error}"))?;
    fs::write(path, payload)
        .map_err(|error| format!("write openclaw config failed ({}): {error}", path.display()))
}

fn ensure_object_mut(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value.as_object_mut().expect("object after ensure")
}

fn ensure_child_object_mut<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    if parent.get(key).and_then(Value::as_object).is_none() {
        parent.insert(key.to_string(), json!({}));
    }
    parent
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("object child after ensure")
}

fn ensure_providers_mut(config: &mut Value) -> Result<&mut Map<String, Value>, String> {
    let root = ensure_object_mut(config);
    let models = ensure_child_object_mut(root, "models");
    if !models.contains_key("mode") {
        models.insert("mode".to_string(), Value::String("merge".to_string()));
    }
    let providers = ensure_child_object_mut(models, "providers");
    Ok(providers)
}

fn ensure_agents_defaults_mut(config: &mut Value) -> &mut Map<String, Value> {
    let root = ensure_object_mut(config);
    let agents = ensure_child_object_mut(root, "agents");
    ensure_child_object_mut(agents, "defaults")
}

fn read_live_provider_values(config: &Value) -> Vec<(String, Value)> {
    config
        .get("models")
        .and_then(|value| value.get("providers"))
        .and_then(Value::as_object)
        .map(|providers| {
            providers
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn read_default_model(config: &Value) -> DesktopOpenClawDefaultModel {
    let fallbacks = config
        .get("agents")
        .and_then(|value| value.get("defaults"))
        .and_then(|value| value.get("model"))
        .and_then(|value| value.get("fallbacks"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    DesktopOpenClawDefaultModel {
        primary: config
            .get("agents")
            .and_then(|value| value.get("defaults"))
            .and_then(|value| value.get("model"))
            .and_then(|value| value.get("primary"))
            .and_then(Value::as_str)
            .map(str::to_string),
        fallbacks,
    }
}

fn sorted_object_keys(value: Option<&Value>) -> Vec<String> {
    let mut keys = value
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn read_env_map(config: &Value) -> BTreeMap<String, String> {
    config
        .get("env")
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| value.as_str().map(|raw| (key.clone(), raw.to_string())))
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default()
}

fn scan_openclaw_health(
    config: &Value,
    live_provider_ids: &[String],
    default_model: &DesktopOpenClawDefaultModel,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if config
        .get("models")
        .and_then(|value| value.get("providers"))
        .and_then(Value::as_object)
        .is_none()
    {
        warnings.push("models.providers is missing from openclaw.json".to_string());
    }
    if live_provider_ids.is_empty() {
        warnings.push("No live providers were found in OpenClaw config.".to_string());
    }
    match default_model.primary.as_deref() {
        Some(primary) => {
            let provider_id = primary.split('/').next().unwrap_or_default();
            if provider_id.is_empty() || !live_provider_ids.iter().any(|value| value == provider_id)
            {
                warnings.push(format!(
                    "Default model `{primary}` does not point to an existing live provider."
                ));
            }
        }
        None => warnings.push("OpenClaw default model is not configured.".to_string()),
    }
    warnings
}

fn parse_live_model(value: &Value) -> Option<DesktopProviderModel> {
    let object = value.as_object()?;
    let model_id = object.get("id").and_then(Value::as_str)?.trim().to_string();
    if model_id.is_empty() {
        return None;
    }
    let display_name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(model_id.as_str())
        .to_string();
    Some(DesktopProviderModel {
        model_id,
        display_name,
        context_window: object.get("contextWindow").and_then(Value::as_i64),
        max_output_tokens: object.get("maxTokens").and_then(Value::as_i64),
        billing_kind: None,
        capability_tags: Vec::new(),
    })
}

fn remove_provider_from_openclaw_config(provider_id: &str) -> Result<(), String> {
    let (path, mut config) = read_openclaw_config(None)?;
    let mut changed = false;
    if let Some(providers) = config
        .get_mut("models")
        .and_then(|value| value.get_mut("providers"))
        .and_then(Value::as_object_mut)
    {
        changed = providers.remove(provider_id).is_some() || changed;
    }

    if let Some(model_catalog) = config
        .get_mut("agents")
        .and_then(|value| value.get_mut("defaults"))
        .and_then(|value| value.get_mut("models"))
        .and_then(Value::as_object_mut)
    {
        let existing_keys = model_catalog.keys().cloned().collect::<Vec<_>>();
        for key in existing_keys {
            if key.starts_with(&format!("{provider_id}/")) {
                model_catalog.remove(&key);
                changed = true;
            }
        }
    }

    if let Some(model_config) = config
        .get_mut("agents")
        .and_then(|value| value.get_mut("defaults"))
        .and_then(|value| value.get_mut("model"))
        .and_then(Value::as_object_mut)
    {
        let should_clear_primary = model_config
            .get("primary")
            .and_then(Value::as_str)
            .map(|value| value.starts_with(&format!("{provider_id}/")))
            .unwrap_or(false);
        if should_clear_primary {
            model_config.remove("primary");
            changed = true;
        }
        if let Some(fallbacks) = model_config
            .get_mut("fallbacks")
            .and_then(Value::as_array_mut)
        {
            let before = fallbacks.len();
            fallbacks.retain(|value| {
                !value
                    .as_str()
                    .map(|raw| raw.starts_with(&format!("{provider_id}/")))
                    .unwrap_or(false)
            });
            if before != fallbacks.len() {
                changed = true;
            }
        }
    }

    if changed {
        write_openclaw_config(&path, &config)?;
    }
    Ok(())
}

fn title_case_provider_name(value: &str) -> String {
    value
        .split(['-', '_', '/', '.'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut output = String::new();
                    output.push(first.to_ascii_uppercase());
                    output.extend(chars);
                    output
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

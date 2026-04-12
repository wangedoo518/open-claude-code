//! Multi-provider LLM registry for open-claude-code.
//!
//! This module owns the on-disk schema and CRUD operations for the file
//! `.claw/providers.json` (inside each project root). The file lets a
//! user register multiple LLM providers — Anthropic, OpenAI, DeepSeek,
//! Qwen (DashScope), Kimi (Moonshot), GLM (Zhipu), or any other
//! OpenAI-compatible endpoint — and pick one as *active*. The agentic
//! loop reads the active provider on every turn so switches take effect
//! without restarting the server.
//!
//! ## File layout
//!
//! ```jsonc
//! {
//!   "version": 1,
//!   "active": "deepseek-prod",
//!   "providers": {
//!     "deepseek-prod": {
//!       "kind": "openai_compat",
//!       "display_name": "DeepSeek Chat",
//!       "base_url": "https://api.deepseek.com/v1",
//!       "api_key": "sk-....",
//!       "model": "deepseek-chat",
//!       "max_tokens": 8192
//!     },
//!     "qwen-plus": {
//!       "kind": "openai_compat",
//!       "base_url": "https://dashscope.aliyuncs.com/compatible-mode/v1",
//!       "api_key": "sk-....",
//!       "model": "qwen-plus"
//!     },
//!     "claude-native": {
//!       "kind": "anthropic",
//!       "api_key": "sk-ant-....",
//!       "model": "claude-opus-4-6"
//!     }
//!   }
//! }
//! ```
//!
//! ## Security
//!
//! * `api_key` values are sensitive and MUST NOT be logged in full. This
//!   module's `Debug` impl redacts them. Tests verify the redaction.
//! * The file is written with mode `0o600` on Unix (best effort on
//!   Windows).
//! * Phase 3.6 will add AES-256-GCM at-rest encryption using the
//!   existing `secure_storage::read_encrypted` / `write_encrypted`
//!   helpers.
//! * `.claw/providers.json` is in the project root `.gitignore` so it
//!   cannot be accidentally committed.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Schema version of the on-disk file. Bumped whenever we make an
/// incompatible change to [`ProvidersConfig`]. Callers should tolerate
/// reading an older version and migrate in-place on next save.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// The canonical filename for the per-project multi-provider config.
pub const PROVIDERS_CONFIG_FILENAME: &str = "providers.json";

/// The directory (relative to project root) that holds the file.
pub const PROVIDERS_CONFIG_DIR: &str = ".claw";

/// Errors raised by provider-config I/O and validation.
#[derive(Debug, thiserror::Error)]
pub enum ProvidersConfigError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON decode error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported schema version: {found} (expected {expected})")]
    UnsupportedVersion { found: u32, expected: u32 },
    #[error("provider id contains illegal characters: {0}")]
    InvalidId(String),
    #[error("provider id not found: {0}")]
    NotFound(String),
    #[error("provider validation failed: {0}")]
    Invalid(String),
}

// ── Provider kinds ─────────────────────────────────────────────────

/// Protocol a provider speaks on the wire.
///
/// `Anthropic` → sends the Anthropic Messages API schema, endpoint is
/// `{base_url}/v1/messages` (base_url defaults to `https://api.anthropic.com`).
///
/// `OpenAiCompat` → sends the OpenAI ChatCompletions schema, endpoint is
/// `{base_url}/chat/completions`. Used by DeepSeek, Moonshot/Kimi, Zhipu
/// GLM, DashScope/Qwen (compatible mode), xAI/Grok, and OpenAI itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    // Explicit renames because serde's `snake_case` converts the Rust
    // variant `OpenAiCompat` to `open_ai_compat` (extra underscore
    // between `Ai` and `Compat`) which doesn't match the natural name
    // users expect to write by hand in `.claw/providers.json`.
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai_compat")]
    OpenAiCompat,
}

impl ProviderKind {
    /// Default `max_tokens` for this protocol when the user doesn't
    /// explicitly set one in the config. Matches the per-family caps
    /// used in our vendored `api` crate patch.
    #[must_use]
    pub const fn default_max_tokens(self) -> u32 {
        match self {
            Self::Anthropic => 32_000,
            Self::OpenAiCompat => 4_096,
        }
    }
}

// ── Per-provider record ────────────────────────────────────────────

/// A single provider entry in the registry. All fields except `kind`,
/// `api_key`, and `model` are optional; sensible defaults apply when
/// they're missing.
///
/// Field order inside the serialized object is stable so diffs stay
/// readable when the user edits the file by hand.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderEntry {
    /// Protocol this provider speaks.
    pub kind: ProviderKind,

    /// Human-readable name shown in the UI (e.g. "DeepSeek Chat").
    /// Defaults to the id if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// API base URL. Required for `openai_compat`, optional for
    /// `anthropic` (defaults to `https://api.anthropic.com`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// API key (bearer token). Sensitive — redacted by Debug impl.
    pub api_key: String,

    /// Model identifier to request (e.g. `deepseek-chat`, `qwen-plus`).
    pub model: String,

    /// Override default `max_tokens` for this provider. If absent the
    /// provider's `ProviderKind::default_max_tokens()` applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Free-form extra HTTP headers to attach to every request. Used
    /// for providers that require custom auth schemes beyond Bearer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_headers: BTreeMap<String, String>,
}

impl ProviderEntry {
    /// Resolve the effective `max_tokens`, honoring the override.
    #[must_use]
    pub fn effective_max_tokens(&self) -> u32 {
        self.max_tokens
            .unwrap_or_else(|| self.kind.default_max_tokens())
    }

    /// Resolve the effective `base_url`. For `Anthropic` this is the
    /// canonical `https://api.anthropic.com` if not overridden; for
    /// `OpenAiCompat` the user MUST supply one (validated at save time).
    #[must_use]
    pub fn effective_base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| match self.kind {
                ProviderKind::Anthropic => "https://api.anthropic.com".to_string(),
                ProviderKind::OpenAiCompat => String::new(),
            })
    }
}

impl std::fmt::Debug for ProviderEntry {
    /// Redacted `Debug` — never leaks `api_key` to logs. Shows length
    /// + a 4-char prefix so you can tell keys apart without exposing
    /// the full value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderEntry")
            .field("kind", &self.kind)
            .field("display_name", &self.display_name)
            .field("base_url", &self.base_url)
            .field(
                "api_key",
                &format_args!(
                    "[redacted: {} chars, prefix={:?}]",
                    self.api_key.len(),
                    &self.api_key.chars().take(4).collect::<String>(),
                ),
            )
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("extra_headers", &self.extra_headers)
            .finish()
    }
}

// ── Top-level config ───────────────────────────────────────────────

/// Root of the `.claw/providers.json` file.
///
/// `providers` is a `BTreeMap` (not `HashMap`) so keys are ordered
/// lexicographically on serialization, keeping diffs readable when the
/// user hand-edits the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    /// The id of the currently-active provider. Must exist in
    /// `providers`. When empty (or pointing at a deleted id), the
    /// agentic loop falls back to the legacy env-var / codex-auth
    /// credential chain.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active: String,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderEntry>,
}

fn default_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_SCHEMA_VERSION,
            active: String::new(),
            providers: BTreeMap::new(),
        }
    }
}

impl ProvidersConfig {
    /// Return the active provider entry, if any. `None` when `active`
    /// is empty or points at a deleted id.
    #[must_use]
    pub fn active_entry(&self) -> Option<&ProviderEntry> {
        if self.active.is_empty() {
            return None;
        }
        self.providers.get(&self.active)
    }

    /// Insert or replace a provider entry. Validates the id and entry
    /// first; on validation failure the config is left unchanged.
    pub fn upsert(
        &mut self,
        id: &str,
        entry: ProviderEntry,
    ) -> Result<(), ProvidersConfigError> {
        validate_id(id)?;
        validate_entry(&entry)?;
        self.providers.insert(id.to_string(), entry);
        // Auto-activate if this is the only provider.
        if self.active.is_empty() && self.providers.len() == 1 {
            self.active = id.to_string();
        }
        Ok(())
    }

    /// Remove a provider. Clears `active` if it pointed at the deleted id.
    pub fn remove(&mut self, id: &str) -> Result<(), ProvidersConfigError> {
        if self.providers.remove(id).is_none() {
            return Err(ProvidersConfigError::NotFound(id.to_string()));
        }
        if self.active == id {
            self.active.clear();
        }
        Ok(())
    }

    /// Set the active provider. Returns `NotFound` if the id doesn't
    /// exist in `providers`.
    pub fn activate(&mut self, id: &str) -> Result<(), ProvidersConfigError> {
        if !self.providers.contains_key(id) {
            return Err(ProvidersConfigError::NotFound(id.to_string()));
        }
        self.active = id.to_string();
        Ok(())
    }
}

// ── Validation ─────────────────────────────────────────────────────

/// Validate a provider id. Only ASCII alphanumerics, dashes, underscores,
/// and dots are allowed. Length 1..64 characters.
fn validate_id(id: &str) -> Result<(), ProvidersConfigError> {
    if id.is_empty() || id.len() > 64 {
        return Err(ProvidersConfigError::InvalidId(id.to_string()));
    }
    let ok = id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !ok {
        return Err(ProvidersConfigError::InvalidId(id.to_string()));
    }
    Ok(())
}

/// Upper bounds on the `extra_headers` map so a bad config can't create
/// a request with hundreds of headers or a multi-megabyte header value.
/// These caps are generous — real providers use at most a couple of extra
/// headers (e.g. `anthropic-beta`, `x-api-version`).
const MAX_EXTRA_HEADERS: usize = 32;
const MAX_HEADER_NAME_LEN: usize = 256;
const MAX_HEADER_VALUE_LEN: usize = 2048;

/// Headers the user is NOT allowed to set via `extra_headers` because
/// they are already managed by the HTTP client or would clobber auth.
/// Match is case-insensitive.
const RESERVED_HEADER_NAMES: &[&str] = &[
    "authorization",
    "x-api-key",
    "content-type",
    "content-length",
    "host",
    "cookie",
    "set-cookie",
];

/// Validate a provider entry. Catches the most common misconfigurations
/// early so errors surface on save rather than on first turn execution.
fn validate_entry(entry: &ProviderEntry) -> Result<(), ProvidersConfigError> {
    if entry.api_key.trim().is_empty() {
        return Err(ProvidersConfigError::Invalid(
            "api_key is empty".to_string(),
        ));
    }
    if entry.model.trim().is_empty() {
        return Err(ProvidersConfigError::Invalid(
            "model is empty".to_string(),
        ));
    }
    // base_url rules:
    //   - OpenAiCompat: REQUIRED, must be http/https (no universal default).
    //   - Anthropic:    OPTIONAL, but if provided must still be http/https
    //                   so we can't silently accept a typo or a bare host.
    match entry.kind {
        ProviderKind::OpenAiCompat => match entry.base_url.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => validate_http_url(s)?,
            _ => {
                return Err(ProvidersConfigError::Invalid(
                    "openai_compat providers require an explicit base_url"
                        .to_string(),
                ));
            }
        },
        ProviderKind::Anthropic => {
            if let Some(s) = entry.base_url.as_deref().map(str::trim) {
                if !s.is_empty() {
                    validate_http_url(s)?;
                }
            }
        }
    }
    if let Some(mt) = entry.max_tokens {
        if mt == 0 || mt > 1_000_000 {
            return Err(ProvidersConfigError::Invalid(format!(
                "max_tokens out of range: {mt}"
            )));
        }
    }
    // extra_headers bounds: cap the count, per-header name/value length,
    // and refuse to let the user override auth-bearing headers that we
    // already manage elsewhere. A bad config shouldn't be able to send
    // a request with 10 000 headers or silently replace `Authorization`.
    if entry.extra_headers.len() > MAX_EXTRA_HEADERS {
        return Err(ProvidersConfigError::Invalid(format!(
            "extra_headers has too many entries ({}); max {}",
            entry.extra_headers.len(),
            MAX_EXTRA_HEADERS
        )));
    }
    for (name, value) in &entry.extra_headers {
        if name.is_empty() || name.len() > MAX_HEADER_NAME_LEN {
            return Err(ProvidersConfigError::Invalid(format!(
                "extra_headers: header name length out of range (1..={MAX_HEADER_NAME_LEN})"
            )));
        }
        if value.len() > MAX_HEADER_VALUE_LEN {
            return Err(ProvidersConfigError::Invalid(format!(
                "extra_headers: value for `{name}` exceeds {MAX_HEADER_VALUE_LEN} chars"
            )));
        }
        // RFC 7230 token characters only — no CR/LF/NUL/spaces which
        // would enable header injection.
        if !name
            .chars()
            .all(|c| c.is_ascii_graphic() && c != ':' && c != '(' && c != ')' && c != ',')
        {
            return Err(ProvidersConfigError::Invalid(format!(
                "extra_headers: header name `{name}` contains invalid characters"
            )));
        }
        if value.chars().any(|c| c == '\r' || c == '\n' || c == '\0') {
            return Err(ProvidersConfigError::Invalid(format!(
                "extra_headers: value for `{name}` contains control characters"
            )));
        }
        let lower = name.to_ascii_lowercase();
        if RESERVED_HEADER_NAMES.contains(&lower.as_str()) {
            return Err(ProvidersConfigError::Invalid(format!(
                "extra_headers: header `{name}` is reserved and cannot be overridden"
            )));
        }
    }
    Ok(())
}

/// Shared helper: a base_url must parse as an http/https URL. We keep
/// the check intentionally shallow (no full URL parser) so the error
/// message stays actionable ("must be http/https: ...").
fn validate_http_url(s: &str) -> Result<(), ProvidersConfigError> {
    if !s.starts_with("http://") && !s.starts_with("https://") {
        return Err(ProvidersConfigError::Invalid(format!(
            "base_url must be http/https: {s}"
        )));
    }
    Ok(())
}

// ── On-disk I/O ────────────────────────────────────────────────────

/// Resolve the absolute path to `.claw/providers.json` inside a project.
pub fn config_path(project_root: &Path) -> PathBuf {
    project_root
        .join(PROVIDERS_CONFIG_DIR)
        .join(PROVIDERS_CONFIG_FILENAME)
}

/// Load the providers config from a project root. Returns
/// `ProvidersConfig::default()` if the file doesn't exist (so first-run
/// callers don't need to check for `NotFound`). Returns an error for
/// other I/O or parse failures.
pub fn load(project_root: &Path) -> Result<ProvidersConfig, ProvidersConfigError> {
    let path = config_path(project_root);
    if !path.exists() {
        return Ok(ProvidersConfig::default());
    }
    let raw = fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        return Ok(ProvidersConfig::default());
    }
    let parsed: ProvidersConfig = serde_json::from_str(&raw)?;
    if parsed.version > CURRENT_SCHEMA_VERSION {
        return Err(ProvidersConfigError::UnsupportedVersion {
            found: parsed.version,
            expected: CURRENT_SCHEMA_VERSION,
        });
    }
    Ok(parsed)
}

/// Persist the providers config to disk. Creates `.claw/` if it
/// doesn't exist. Best-effort `chmod 0o600` on Unix so API keys
/// aren't world-readable.
pub fn save(
    project_root: &Path,
    config: &ProvidersConfig,
) -> Result<(), ProvidersConfigError> {
    let path = config_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&path, json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

// ── Built-in provider templates ────────────────────────────────────

/// A pre-filled provider entry the user can tweak with their own API
/// key. Returned by `builtin_templates()` for use in onboarding flows
/// and the "add provider" UI.
#[derive(Debug, Clone)]
pub struct ProviderTemplate {
    /// Suggested id (e.g. `"deepseek"`).
    pub id: &'static str,
    /// Human-readable name (e.g. `"DeepSeek"`).
    pub display_name: &'static str,
    pub kind: ProviderKind,
    /// Pre-filled base URL (empty for Anthropic which uses the default).
    pub base_url: &'static str,
    /// Recommended model id.
    pub default_model: &'static str,
    /// Provider max_tokens cap (our vendored patch uses these too).
    pub max_tokens: u32,
    /// Short description shown next to the template in the UI.
    pub description: &'static str,
    /// URL where the user can create an API key for this provider.
    pub api_key_url: &'static str,
}

/// Hard-coded list of common LLM providers the user can pick from
/// when adding a new entry. Sorted roughly by global popularity; the
/// UI can reorder freely.
#[must_use]
pub fn builtin_templates() -> &'static [ProviderTemplate] {
    &[
        ProviderTemplate {
            id: "anthropic",
            display_name: "Anthropic (Claude)",
            kind: ProviderKind::Anthropic,
            base_url: "",
            default_model: "claude-opus-4-6",
            max_tokens: 32_000,
            description: "Official Anthropic Claude API (native protocol).",
            api_key_url: "https://console.anthropic.com/settings/keys",
        },
        ProviderTemplate {
            id: "openai",
            display_name: "OpenAI",
            kind: ProviderKind::OpenAiCompat,
            base_url: "https://api.openai.com/v1",
            default_model: "gpt-4o",
            max_tokens: 16_384,
            description: "Official OpenAI ChatCompletions API.",
            api_key_url: "https://platform.openai.com/api-keys",
        },
        ProviderTemplate {
            id: "deepseek",
            display_name: "DeepSeek",
            kind: ProviderKind::OpenAiCompat,
            base_url: "https://api.deepseek.com/v1",
            default_model: "deepseek-chat",
            max_tokens: 8_192,
            description: "DeepSeek V3 / Reasoner via OpenAI-compat API.",
            api_key_url: "https://platform.deepseek.com/api_keys",
        },
        ProviderTemplate {
            id: "qwen-dashscope",
            display_name: "Qwen (Aliyun DashScope)",
            kind: ProviderKind::OpenAiCompat,
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            default_model: "qwen-plus",
            max_tokens: 8_192,
            description: "Alibaba Cloud Bailian DashScope compat mode.",
            api_key_url: "https://bailian.console.aliyun.com/?apiKey=1",
        },
        ProviderTemplate {
            id: "moonshot-kimi",
            display_name: "Moonshot (Kimi)",
            kind: ProviderKind::OpenAiCompat,
            base_url: "https://api.moonshot.cn/v1",
            default_model: "moonshot-v1-128k",
            max_tokens: 8_192,
            description: "Moonshot AI / Kimi via OpenAI-compat API.",
            api_key_url: "https://platform.moonshot.cn/console/api-keys",
        },
        ProviderTemplate {
            id: "glm-zhipu",
            display_name: "GLM (Zhipu)",
            kind: ProviderKind::OpenAiCompat,
            base_url: "https://open.bigmodel.cn/api/paas/v4",
            default_model: "glm-4-plus",
            max_tokens: 4_096,
            description: "Zhipu AI GLM-4 via OpenAI-compat API.",
            api_key_url: "https://bigmodel.cn/usercenter/apikeys",
        },
        ProviderTemplate {
            id: "xai-grok",
            display_name: "xAI Grok",
            kind: ProviderKind::OpenAiCompat,
            base_url: "https://api.x.ai/v1",
            default_model: "grok-3",
            max_tokens: 64_000,
            description: "xAI Grok models via OpenAI-compat API.",
            api_key_url: "https://console.x.ai/team/default/api-keys",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_openai_compat() -> ProviderEntry {
        ProviderEntry {
            kind: ProviderKind::OpenAiCompat,
            display_name: Some("DeepSeek".to_string()),
            base_url: Some("https://api.deepseek.com/v1".to_string()),
            api_key: "example-openai-key-abcdef1234567890".to_string(),
            model: "deepseek-chat".to_string(),
            max_tokens: None,
            extra_headers: BTreeMap::new(),
        }
    }

    fn sample_anthropic() -> ProviderEntry {
        ProviderEntry {
            kind: ProviderKind::Anthropic,
            display_name: Some("Claude".to_string()),
            base_url: None,
            api_key: "example-anthropic-key-aaaaaaaaaaaaaaaa".to_string(),
            model: "claude-opus-4-6".to_string(),
            max_tokens: None,
            extra_headers: BTreeMap::new(),
        }
    }

    #[test]
    fn debug_redacts_api_key() {
        let entry = sample_openai_compat();
        let debug_str = format!("{entry:?}");
        assert!(
            !debug_str.contains("abcdef"),
            "api_key value leaked into Debug output: {debug_str}"
        );
        assert!(debug_str.contains("redacted"));
        assert!(debug_str.contains("prefix=\"exam\""));
    }

    #[test]
    fn effective_max_tokens_defaults_by_kind() {
        let mut anthropic = sample_anthropic();
        assert_eq!(anthropic.effective_max_tokens(), 32_000);
        anthropic.max_tokens = Some(16_000);
        assert_eq!(anthropic.effective_max_tokens(), 16_000);

        let oac = sample_openai_compat();
        assert_eq!(oac.effective_max_tokens(), 4_096);
    }

    #[test]
    fn effective_base_url_anthropic_default() {
        let e = sample_anthropic();
        assert_eq!(e.effective_base_url(), "https://api.anthropic.com");
    }

    #[test]
    fn validate_id_accepts_reasonable_ids() {
        assert!(validate_id("deepseek").is_ok());
        assert!(validate_id("deepseek-prod").is_ok());
        assert!(validate_id("work_env.1").is_ok());
        assert!(validate_id("abc").is_ok());
    }

    #[test]
    fn validate_id_rejects_bad_ids() {
        assert!(validate_id("").is_err());
        assert!(validate_id("has space").is_err());
        assert!(validate_id("has/slash").is_err());
        assert!(validate_id("has\\backslash").is_err());
        assert!(validate_id(&"a".repeat(65)).is_err());
        assert!(validate_id("中文").is_err());
    }

    #[test]
    fn validate_entry_rejects_empty_fields() {
        let mut e = sample_openai_compat();
        e.api_key = String::new();
        assert!(validate_entry(&e).is_err());

        e.api_key = "sk-ok".to_string();
        e.model = String::new();
        assert!(validate_entry(&e).is_err());
    }

    #[test]
    fn validate_entry_requires_openai_base_url() {
        let mut e = sample_openai_compat();
        e.base_url = None;
        assert!(validate_entry(&e).is_err());

        e.base_url = Some("ftp://nope.example".to_string());
        assert!(validate_entry(&e).is_err());
    }

    #[test]
    fn anthropic_entry_can_omit_base_url() {
        let e = sample_anthropic();
        assert!(validate_entry(&e).is_ok());
    }

    #[test]
    fn upsert_validates_id_and_entry() {
        let mut cfg = ProvidersConfig::default();
        assert!(cfg.upsert("good-id", sample_openai_compat()).is_ok());
        // Auto-activated because it's the only entry.
        assert_eq!(cfg.active, "good-id");

        assert!(cfg.upsert("bad id with space", sample_openai_compat()).is_err());
    }

    #[test]
    fn remove_clears_active_if_target_was_active() {
        let mut cfg = ProvidersConfig::default();
        cfg.upsert("a", sample_openai_compat()).unwrap();
        cfg.upsert("b", sample_anthropic()).unwrap();
        assert_eq!(cfg.active, "a"); // auto-activated first insert

        cfg.remove("a").unwrap();
        assert_eq!(cfg.active, "", "active should be cleared after removing it");
        assert!(cfg.providers.contains_key("b"));
    }

    #[test]
    fn activate_checks_existence() {
        let mut cfg = ProvidersConfig::default();
        cfg.upsert("a", sample_openai_compat()).unwrap();
        assert!(cfg.activate("a").is_ok());
        assert!(matches!(
            cfg.activate("nonexistent"),
            Err(ProvidersConfigError::NotFound(_))
        ));
    }

    #[test]
    fn load_returns_default_on_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = load(tmp.path()).unwrap();
        assert_eq!(cfg.version, CURRENT_SCHEMA_VERSION);
        assert!(cfg.providers.is_empty());
        assert!(cfg.active.is_empty());
    }

    #[test]
    fn save_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = ProvidersConfig::default();
        cfg.upsert("deepseek", sample_openai_compat()).unwrap();
        cfg.upsert("claude", sample_anthropic()).unwrap();
        cfg.active = "claude".to_string();

        save(tmp.path(), &cfg).unwrap();

        let loaded = load(tmp.path()).unwrap();
        assert_eq!(loaded.version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.active, "claude");
        assert_eq!(loaded.providers.len(), 2);
        assert!(loaded.providers.contains_key("deepseek"));
        assert!(loaded.providers.contains_key("claude"));
        // api_key must survive the round-trip (since the file isn't
        // encrypted yet; Phase 3.6 changes this).
        assert_eq!(
            loaded.providers["deepseek"].api_key,
            "example-openai-key-abcdef1234567890"
        );
    }

    #[test]
    fn load_rejects_future_version() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".claw");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("providers.json"),
            r#"{"version":9999,"active":"","providers":{}}"#,
        )
        .unwrap();

        let err = load(tmp.path()).unwrap_err();
        match err {
            ProvidersConfigError::UnsupportedVersion { found, expected } => {
                assert_eq!(found, 9999);
                assert_eq!(expected, CURRENT_SCHEMA_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn active_entry_returns_none_when_empty() {
        let cfg = ProvidersConfig::default();
        assert!(cfg.active_entry().is_none());
    }

    #[test]
    fn active_entry_returns_none_when_dangling() {
        let mut cfg = ProvidersConfig::default();
        cfg.active = "nonexistent".to_string();
        assert!(cfg.active_entry().is_none());
    }

    #[test]
    fn active_entry_returns_the_active_provider() {
        let mut cfg = ProvidersConfig::default();
        cfg.upsert("deepseek", sample_openai_compat()).unwrap();
        let entry = cfg.active_entry().unwrap();
        assert_eq!(entry.model, "deepseek-chat");
    }

    #[test]
    fn builtin_templates_are_non_empty_and_unique() {
        let templates = builtin_templates();
        assert!(templates.len() >= 6);
        let ids: std::collections::HashSet<_> = templates.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), templates.len(), "template ids must be unique");
        // All templates must have non-empty required fields.
        for t in templates {
            assert!(!t.id.is_empty());
            assert!(!t.display_name.is_empty());
            assert!(!t.default_model.is_empty());
            if t.kind == ProviderKind::OpenAiCompat {
                assert!(!t.base_url.is_empty(), "{} missing base_url", t.id);
            }
        }
    }

    #[test]
    fn serializes_without_base_url_for_anthropic() {
        // Anthropic entries can omit base_url; the JSON should not
        // contain the key at all (skip_serializing_if).
        let e = sample_anthropic();
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("base_url"));
    }

    #[test]
    fn serializes_without_display_name_when_missing() {
        let mut e = sample_openai_compat();
        e.display_name = None;
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("display_name"));
    }

    #[test]
    fn anthropic_base_url_is_validated_when_provided() {
        // Regression guard for C-05: an Anthropic entry with a garbage
        // base_url used to silently pass because only OpenAiCompat hit
        // the http/https check. Now both kinds validate.
        let mut e = sample_anthropic();
        e.base_url = Some("ftp://no.example.com".to_string());
        let err = validate_entry(&e).unwrap_err();
        assert!(
            matches!(err, ProvidersConfigError::Invalid(ref msg) if msg.contains("http/https")),
            "expected http/https error, got {err:?}"
        );

        // But a legitimate http/https override (e.g. corporate proxy)
        // must still be accepted.
        e.base_url = Some("https://proxy.internal/anthropic".to_string());
        assert!(validate_entry(&e).is_ok());

        // And the empty-string / None cases both pass (use default).
        e.base_url = Some(String::new());
        assert!(validate_entry(&e).is_ok());
        e.base_url = None;
        assert!(validate_entry(&e).is_ok());
    }

    #[test]
    fn extra_headers_bounds_enforced() {
        // C-01 regression guard: unbounded extra_headers would let a
        // bad config construct multi-megabyte requests or override the
        // auth header we manage ourselves.
        let mut e = sample_openai_compat();

        // Too many headers
        for i in 0..(MAX_EXTRA_HEADERS + 1) {
            e.extra_headers
                .insert(format!("X-Header-{i}"), "v".to_string());
        }
        assert!(validate_entry(&e).is_err());

        // Reserved name (case-insensitive)
        e.extra_headers.clear();
        e.extra_headers
            .insert("Authorization".to_string(), "Bearer hax".to_string());
        assert!(validate_entry(&e).is_err());
        e.extra_headers.clear();
        e.extra_headers
            .insert("X-API-KEY".to_string(), "hax".to_string());
        assert!(validate_entry(&e).is_err());

        // Header value with CRLF (injection attempt)
        e.extra_headers.clear();
        e.extra_headers
            .insert("X-Custom".to_string(), "line1\r\nInjected: yes".to_string());
        assert!(validate_entry(&e).is_err());

        // Overlong name
        e.extra_headers.clear();
        e.extra_headers
            .insert("X".repeat(MAX_HEADER_NAME_LEN + 1), "v".to_string());
        assert!(validate_entry(&e).is_err());

        // Overlong value
        e.extra_headers.clear();
        e.extra_headers
            .insert("X-Ok".to_string(), "v".repeat(MAX_HEADER_VALUE_LEN + 1));
        assert!(validate_entry(&e).is_err());

        // Good headers pass
        e.extra_headers.clear();
        e.extra_headers
            .insert("anthropic-beta".to_string(), "tools-2024-05-16".to_string());
        e.extra_headers
            .insert("X-Api-Version".to_string(), "2024-10-01".to_string());
        assert!(validate_entry(&e).is_ok());
    }

    #[test]
    fn provider_kind_wire_format_is_snake_case_no_extra_underscore() {
        // Regression guard: serde's default snake_case would convert
        // OpenAiCompat to "open_ai_compat" (with an extra underscore
        // between Ai and Compat). Users expect "openai_compat", so
        // we rename explicitly. This test fails loudly if someone
        // removes the rename annotation.
        let json = serde_json::to_string(&ProviderKind::OpenAiCompat).unwrap();
        assert_eq!(json, "\"openai_compat\"");
        let json = serde_json::to_string(&ProviderKind::Anthropic).unwrap();
        assert_eq!(json, "\"anthropic\"");

        // Round-trip from user-expected form
        let back: ProviderKind = serde_json::from_str("\"openai_compat\"").unwrap();
        assert_eq!(back, ProviderKind::OpenAiCompat);
        let back: ProviderKind = serde_json::from_str("\"anthropic\"").unwrap();
        assert_eq!(back, ProviderKind::Anthropic);
    }
}

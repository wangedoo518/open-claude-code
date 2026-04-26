//! Adapter bridging `desktop-core` runtime auth sources to
//! `wiki_maintainer::BrokerSender` (canonical §4 blade 3).
//!
//! ## Why this exists
//!
//! `wiki_maintainer` cannot depend on `desktop-core` — that would
//! create a cycle, since desktop-core wants to CALL the maintainer
//! from its HTTP handler layer. And when the optional private-cloud
//! broker is enabled, it can't impl `BrokerSender` for
//! `Arc<CodexBroker>` directly, because the orphan rule forbids
//! third-party trait impls on third-party types.
//!
//! Solution: a thin wrapper struct owned here in desktop-core that
//! optionally wraps the process-global private-cloud broker and then
//! falls back to `.claw/providers.json`. The maintainer crate sees
//! only `&impl BrokerSender`, so tests keep using their
//! `MockBrokerSender` and the production HTTP handler builds a
//! `BrokerAdapter::from_global()` per request.
//!
//! ## Error mapping
//!
//! `BrokerError` carries richer structure than `MaintainerError`
//! wants to surface. We flatten to `MaintainerError::Broker(String)`
//! so the maintainer crate doesn't need to know about the Codex
//! pool's internals. The HTTP handler then unpacks the string for
//! the user-facing 503 message. This keeps the dep graph one-way:
//! `desktop-server → desktop-core → wiki_maintainer`, never the
//! reverse.

use api::{MessageRequest, MessageResponse};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use wiki_maintainer::{BrokerSender, MaintainerError};

#[cfg(feature = "private-cloud")]
use std::sync::Arc;

#[cfg(feature = "private-cloud")]
use crate::codex_broker::{self, CodexBroker};

/// Wrapper around the optional private-cloud broker that implements
/// `wiki_maintainer::BrokerSender`. Construct one per
/// `propose_for_raw_entry` call — it is cheap to clone and stays
/// stateless when only the providers.json fallback is available.
#[derive(Clone, Default)]
pub struct BrokerAdapter {
    #[cfg(feature = "private-cloud")]
    broker: Option<Arc<CodexBroker>>,
}

impl BrokerAdapter {
    #[cfg(feature = "private-cloud")]
    /// Wrap a specific broker instance. Used by tests that build
    /// their own `CodexBroker` and by callers that already hold a
    /// handle (e.g. `AppState` in desktop-server).
    #[must_use]
    pub fn new(broker: Arc<CodexBroker>) -> Self {
        Self {
            broker: Some(broker),
        }
    }

    /// Build an adapter from the process-global broker installed by
    /// desktop-server's `AppState::new`. This constructor always
    /// succeeds: when the private-cloud broker is disabled or absent,
    /// the adapter simply skips that step and relies on the generic
    /// providers.json fallback.
    ///
    /// Combined with `propose_for_raw_entry`'s `&impl BrokerSender`
    /// signature, this lets an HTTP handler write:
    ///
    /// ```ignore
    /// let adapter = BrokerAdapter::from_global();
    /// let proposal = propose_for_raw_entry(&paths, id, &adapter).await?;
    /// ```
    #[must_use]
    pub fn from_global() -> Self {
        #[cfg(feature = "private-cloud")]
        {
            return Self {
                broker: codex_broker::global(),
            };
        }

        #[cfg(not(feature = "private-cloud"))]
        {
            Self::default()
        }
    }

    /// Return Ok when this process has at least one auth source that can
    /// plausibly serve a maintainer request before we enqueue long-running work.
    pub fn runtime_auth_health(&self) -> Result<(), String> {
        #[cfg(feature = "private-cloud")]
        if let Some(broker) = &self.broker {
            let status = broker.public_status();
            if status.fresh_count + status.expiring_count > 0 {
                return Ok(());
            }
        }

        providers_json_fallback_health()
    }

    #[cfg(feature = "private-cloud")]
    /// Expose the inner broker handle for callers that need the
    /// original API (e.g. to check pool status before deciding
    /// whether to propose at all).
    #[must_use]
    pub fn inner(&self) -> Option<&Arc<CodexBroker>> {
        self.broker.as_ref()
    }
}

/// Validate that the same `.claw/providers.json` fallback used by
/// `chat_completion` has an active provider with enough fields to make
/// a request. This is intentionally network-free; endpoint probing is
/// handled by the provider settings API.
pub fn providers_json_fallback_health() -> Result<(), String> {
    for root in provider_config_candidate_roots() {
        let path = root.join(".claw").join("providers.json");
        if !path.exists() {
            continue;
        }

        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let parsed = parse_provider_config_json(&raw)
            .ok_or_else(|| format!("failed to parse {}", path.display()))?;
        let active_id = parsed
            .get("active")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| format!("{} has no active provider", path.display()))?;
        let entry = parsed
            .get("providers")
            .and_then(|p| p.get(active_id))
            .ok_or_else(|| {
                format!(
                    "{} active provider `{active_id}` is missing",
                    path.display()
                )
            })?;

        let api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
        if resolve_provider_api_key(api_key).is_none() {
            return Err(format!("active provider `{active_id}` has empty api_key"));
        }

        let model = entry.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if model.trim().is_empty() {
            return Err(format!("active provider `{active_id}` has empty model"));
        }

        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "openai_compat" => {
                let base_url = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
                if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
                    return Err(format!(
                        "active provider `{active_id}` has invalid base_url"
                    ));
                }
                return Ok(());
            }
            "anthropic" => return Ok(()),
            _ => {
                return Err(format!(
                    "active provider `{active_id}` has unsupported kind `{kind}`"
                ));
            }
        }
    }

    Err("no active providers.json fallback found".to_string())
}

#[async_trait]
impl BrokerSender for BrokerAdapter {
    async fn chat_completion(
        &self,
        request: MessageRequest,
    ) -> wiki_maintainer::Result<MessageResponse> {
        #[cfg(feature = "private-cloud")]
        if let Some(broker) = &self.broker {
            match broker.chat_completion(request.clone()).await {
                Ok(resp) => return Ok(resp),
                Err(broker_err) => {
                    eprintln!(
                        "[maintainer-adapter] private-cloud broker failed: {broker_err}, \
                         trying providers.json fallback"
                    );
                }
            }
        }

        // Fallback: try .claw/providers.json active provider.
        // This lets "Maintain this" work with Kimi/DeepSeek/etc.
        // when no Codex account is available.
        let provider_result = try_providers_json_chat_completion(&request).await;
        match provider_result {
            Some(Ok(resp)) => Ok(resp),
            Some(Err(api_err)) => Err(MaintainerError::Broker(format!(
                "providers.json fallback failed: {api_err}"
            ))),
            None => Err(MaintainerError::Broker(
                "no codex account available and no providers.json fallback found".to_string(),
            )),
        }
    }
}

/// Try to use the active provider from .claw/providers.json to run
/// a chat_completion. Returns None if no providers.json or no active
/// provider found. Returns Some(result) if a provider was found and
/// the request was attempted.
async fn try_providers_json_chat_completion(
    request: &MessageRequest,
) -> Option<Result<MessageResponse, api::ApiError>> {
    use api::{AnthropicClient, AuthSource, OpenAiCompatClient, OpenAiCompatConfig};

    for root in provider_config_candidate_roots() {
        let path = root.join(".claw").join("providers.json");
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(parsed) = parse_provider_config_json(&raw) else {
            continue;
        };
        let Some(active_id) = parsed.get("active").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(entry) = parsed.get("providers").and_then(|p| p.get(active_id)) else {
            continue;
        };
        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
        let Some(api_key) = resolve_provider_api_key(api_key) else {
            continue;
        };
        let base_url = entry.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let model = entry.get("model").and_then(|v| v.as_str()).unwrap_or("");

        // Override the model in the request to match the provider's model.
        let mut req = request.clone();
        if !model.is_empty() {
            req.model = model.to_string();
        }
        // Strip tools for non-OpenAI providers (same fix as openai_compat.rs).
        if !base_url.contains("api.openai.com") {
            req.tools = None;
            req.tool_choice = None;
        }

        match kind {
            "openai_compat" if !base_url.is_empty() => {
                eprintln!(
                    "[maintainer-adapter] using providers.json OpenAiCompat \
                     {active_id:?} base_url={base_url:?} model={model:?}"
                );
                let client = OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::openai())
                    .with_base_url(base_url.to_string());
                return Some(client.send_message(&req).await);
            }
            "anthropic" => {
                let effective_base = if base_url.is_empty() {
                    "https://api.anthropic.com"
                } else {
                    base_url
                };
                eprintln!(
                    "[maintainer-adapter] using providers.json Anthropic \
                     {active_id:?} base_url={effective_base:?} model={model:?}"
                );
                let client = AnthropicClient::from_auth(AuthSource::ApiKey(api_key.clone()))
                    .with_base_url(effective_base.to_string());
                return Some(client.send_message(&req).await);
            }
            _ => continue,
        }
    }
    None
}

fn parse_provider_config_json(raw: &str) -> Option<serde_json::Value> {
    serde_json::from_str(raw.trim_start_matches('\u{feff}')).ok()
}

fn resolve_provider_api_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(name) = trimmed.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        return std::env::var(name)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
    }

    Some(trimmed.to_string())
}

fn provider_config_candidate_roots() -> Vec<PathBuf> {
    std::env::current_dir()
        .map(|cwd| provider_config_candidate_roots_from(&cwd))
        .unwrap_or_default()
}

fn provider_config_candidate_roots_from(start: &Path) -> Vec<PathBuf> {
    start.ancestors().map(Path::to_path_buf).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "private-cloud")]
    use crate::secure_storage;
    #[cfg(feature = "private-cloud")]
    use tempfile::tempdir;

    #[cfg(feature = "private-cloud")]
    fn seed_test_key() {
        secure_storage::seed_key_for_test([11u8; 32]);
    }

    #[test]
    fn provider_config_candidate_roots_walk_up_from_nested_cwd() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp
            .path()
            .join("apps")
            .join("desktop-shell")
            .join("src-tauri");
        std::fs::create_dir_all(&nested).expect("nested dirs");

        let roots = provider_config_candidate_roots_from(&nested);

        assert_eq!(roots.first().map(PathBuf::as_path), Some(nested.as_path()));
        assert!(
            roots.iter().any(|root| root == temp.path()),
            "candidate roots should include the repository/project root"
        );
    }

    #[test]
    fn provider_config_parser_accepts_utf8_bom() {
        let parsed = parse_provider_config_json(
            "\u{feff}{\"active\":\"deepseek\",\"providers\":{\"deepseek\":{\"kind\":\"openai_compat\"}}}",
        )
        .expect("UTF-8 BOM should not hide an otherwise valid providers.json");

        assert_eq!(
            parsed.get("active").and_then(|v| v.as_str()),
            Some("deepseek")
        );
    }

    #[test]
    fn provider_api_key_resolver_supports_env_placeholders() {
        std::env::set_var("CLAW_TEST_PROVIDER_KEY", "test-provider-key");

        assert_eq!(
            resolve_provider_api_key("<CLAW_TEST_PROVIDER_KEY>").as_deref(),
            Some("test-provider-key")
        );
        assert_eq!(
            resolve_provider_api_key(" inline-provider-key ").as_deref(),
            Some("inline-provider-key")
        );
        assert!(resolve_provider_api_key("<CLAW_TEST_MISSING_PROVIDER_KEY>").is_none());

        std::env::remove_var("CLAW_TEST_PROVIDER_KEY");
    }

    #[cfg(feature = "private-cloud")]
    #[tokio::test]
    async fn adapter_surfaces_empty_pool_as_broker_error() {
        // Build a broker with an empty pool and verify that calling
        // chat_completion through the adapter surfaces as
        // MaintainerError::Broker (not a panic, not a different
        // variant). This is the common case in dev where no account
        // has been enrolled yet.
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = Arc::new(CodexBroker::new(tmp.path()).unwrap());
        let adapter = BrokerAdapter::new(broker);

        // Minimal MessageRequest — the broker never sends it because
        // pick_account_bearer_token fails first.
        let req = MessageRequest {
            model: "test".to_string(),
            max_tokens: 16,
            system: None,
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: false,
        };

        let err = adapter.chat_completion(req).await.unwrap_err();
        match err {
            MaintainerError::Broker(msg) => {
                // Must mention the pool being empty so the HTTP
                // handler can render a useful 503.
                assert!(
                    msg.contains("no codex account") || msg.contains("pool_size"),
                    "unexpected broker error text: {msg}"
                );
            }
            other => panic!("expected MaintainerError::Broker, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_global_is_safe_without_a_private_cloud_broker() {
        // Tests do not guarantee a process-global broker. The adapter
        // should still construct so the providers.json fallback path
        // remains available in OSS builds and in server-side tests.
        let adapter = BrokerAdapter::from_global();
        let _ = adapter;
    }
}

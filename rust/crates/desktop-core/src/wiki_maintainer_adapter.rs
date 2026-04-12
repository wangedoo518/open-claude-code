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

    #[cfg(feature = "private-cloud")]
    /// Expose the inner broker handle for callers that need the
    /// original API (e.g. to check pool status before deciding
    /// whether to propose at all).
    #[must_use]
    pub fn inner(&self) -> Option<&Arc<CodexBroker>> {
        self.broker.as_ref()
    }
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
            Some(Err(api_err)) => {
                Err(MaintainerError::Broker(format!(
                    "providers.json fallback failed: {api_err}"
                )))
            }
            None => {
                Err(MaintainerError::Broker(
                    "no codex account available and no providers.json fallback found".to_string(),
                ))
            }
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

    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }

    for root in &roots {
        let path = root.join(".claw").join("providers.json");
        let Ok(raw) = std::fs::read_to_string(&path) else { continue };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };
        let Some(active_id) = parsed.get("active").and_then(|v| v.as_str()) else { continue };
        let Some(entry) = parsed.get("providers").and_then(|p| p.get(active_id)) else { continue };
        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
        if api_key.trim().is_empty() { continue }
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
                let client = OpenAiCompatClient::new(
                    api_key.to_string(),
                    OpenAiCompatConfig::openai(),
                )
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
                let client = AnthropicClient::from_auth(
                    AuthSource::ApiKey(api_key.to_string()),
                )
                .with_base_url(effective_base.to_string());
                return Some(client.send_message(&req).await);
            }
            _ => continue,
        }
    }
    None
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

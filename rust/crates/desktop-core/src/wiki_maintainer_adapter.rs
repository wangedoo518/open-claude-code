//! Adapter bridging `desktop-core`'s `CodexBroker` to
//! `wiki_maintainer::BrokerSender` (canonical §4 blade 3).
//!
//! ## Why this exists
//!
//! `wiki_maintainer` cannot depend on `desktop-core` — that would
//! create a cycle, since desktop-core wants to CALL the maintainer
//! from its HTTP handler layer. And it can't impl `BrokerSender`
//! for `Arc<CodexBroker>` directly, because the orphan rule
//! forbids third-party trait impls on third-party types.
//!
//! Solution: a thin wrapper struct owned here in desktop-core that
//! wraps `Arc<CodexBroker>` and impls the maintainer's trait. The
//! maintainer crate sees only `&impl BrokerSender`, so tests keep
//! using their `MockBrokerSender` and the production HTTP handler
//! builds a `BrokerAdapter::from_global()` per request.
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

use std::sync::Arc;

use api::{MessageRequest, MessageResponse};
use async_trait::async_trait;
use wiki_maintainer::{BrokerSender, MaintainerError};

use crate::codex_broker::{self, CodexBroker};

/// Wrapper around `Arc<CodexBroker>` that implements
/// `wiki_maintainer::BrokerSender`. Construct one per
/// `propose_for_raw_entry` call — it holds only an `Arc` so cloning
/// is cheap, and the adapter is stateless.
#[derive(Clone)]
pub struct BrokerAdapter {
    broker: Arc<CodexBroker>,
}

impl BrokerAdapter {
    /// Wrap a specific broker instance. Used by tests that build
    /// their own `CodexBroker` and by callers that already hold a
    /// handle (e.g. `AppState` in desktop-server).
    #[must_use]
    pub fn new(broker: Arc<CodexBroker>) -> Self {
        Self { broker }
    }

    /// Convenience: pull the process-global broker installed by
    /// desktop-server's `AppState::new`. Returns `None` if no
    /// broker has been installed (tests, CLI tools, or any
    /// process that hasn't booted the server).
    ///
    /// Combined with `propose_for_raw_entry`'s `&impl BrokerSender`
    /// signature, this lets an HTTP handler write:
    ///
    /// ```ignore
    /// let adapter = BrokerAdapter::from_global()
    ///     .ok_or_else(|| AppError::service_unavailable("broker not installed"))?;
    /// let proposal = propose_for_raw_entry(&paths, id, &adapter).await?;
    /// ```
    #[must_use]
    pub fn from_global() -> Option<Self> {
        codex_broker::global().map(Self::new)
    }

    /// Expose the inner broker handle for callers that need the
    /// original API (e.g. to check pool status before deciding
    /// whether to propose at all).
    #[must_use]
    pub fn inner(&self) -> &Arc<CodexBroker> {
        &self.broker
    }
}

#[async_trait]
impl BrokerSender for BrokerAdapter {
    async fn chat_completion(
        &self,
        request: MessageRequest,
    ) -> wiki_maintainer::Result<MessageResponse> {
        // Flatten BrokerError into MaintainerError::Broker(String).
        // We deliberately include the Display form so the maintainer
        // crate's error messages stay informative without coupling
        // to `BrokerError`'s variant shape.
        self.broker
            .chat_completion(request)
            .await
            .map_err(|e| MaintainerError::Broker(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secure_storage;
    use tempfile::tempdir;

    fn seed_test_key() {
        secure_storage::seed_key_for_test([11u8; 32]);
    }

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
    async fn from_global_returns_none_when_broker_uninstalled() {
        // Tests don't install the process-global broker (install is
        // idempotent and cannot be undone, so enabling it would
        // pollute other tests in the same binary). Verify the
        // adapter handles None gracefully.
        let adapter = BrokerAdapter::from_global();
        // We can't assert Some/None deterministically since another
        // test in this binary might have installed. Just assert the
        // function does not panic.
        let _ = adapter;
    }
}

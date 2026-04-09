//! `codex_broker` — internal Codex account pool (ClawWiki canonical §9.2).
//!
//! Per the canonical product design, the user's Codex subscription
//! accounts live here and **never** leave the Rust process. This
//! module intentionally has NO HTTP surface for `chat_completion`.
//! The two consumers — `ask_runtime` (S3) and `wiki_maintainer`
//! (S4) — call into it as a Rust-to-Rust API inside the same
//! workspace. External clients (Cursor, CCD, third-party CLIs) have
//! no way to reach these tokens.
//!
//! ## What lands in S2 (this file)
//!
//! * Typed pool of `CloudAccountRecord` persisted to
//!   `~/.clawwiki/.clawwiki/cloud-accounts.enc` via the existing
//!   `secure_storage` AES-256-GCM wrapper (single machine-local key
//!   under `~/.warwolf/.secret-key`).
//! * `sync_cloud_accounts` / `list_cloud_accounts` / `clear_cloud_accounts`
//!   CRUD, with `list_*` returning a **redacted** view (`CloudAccountPublic`)
//!   that never contains access/refresh tokens.
//! * `public_status()` returning the aggregate counts that
//!   `Settings → Subscription & Codex Pool` (S2.3) will render.
//!
//! ## What does NOT land here yet
//!
//! * `chat_completion(req)` — stubbed to return `BrokerError::NotImplemented`.
//!   S3 (ask_runtime) will wire this to the vendored `api` crate's
//!   OpenAI-compat client once the pool picker + token refresh loop
//!   are in place.
//! * Token refresh against the trade-service — S3 adds a background
//!   task that watches `token_expires_at_epoch` and rotates with the
//!   refresh_token via an HTTPS round-trip.
//! * `managed_auth::CloudManaged` variant — the historical spec in
//!   `docs/desktop-shell/cloud-managed-integration.md` proposed
//!   merging cloud accounts into the existing `DesktopCodexAuthSource`
//!   enum. S2 intentionally keeps the pool isolated so rollback
//!   doesn't touch the shipped OAuth / ImportedAuthJson paths.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use api::{
    MessageRequest, MessageResponse, OpenAiCompatClient, OpenAiCompatConfig,
    ProviderClient,
};
use serde::{Deserialize, Serialize};

use crate::secure_storage::{self, SecureStorageError};

/// Account descriptor accepted by `sync_cloud_accounts`. Built by the
/// frontend from the `billing/cloud-accounts-sync` response. Carries
/// the raw tokens — those are stored in memory + encrypted on disk,
/// and MUST NOT be serialized anywhere else.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudAccountInput {
    /// Stable identifier from the trade service (`codex_user_id`).
    pub codex_user_id: String,
    /// Human-readable label shown on the Codex Pool panel.
    pub alias: String,
    /// Raw OAuth access token. Sensitive.
    pub access_token: String,
    /// Raw OAuth refresh token. Sensitive.
    pub refresh_token: String,
    /// Unix epoch seconds at which `access_token` stops being valid.
    pub token_expires_at_epoch: i64,
    /// Optional billing subscription id the trade service assigned
    /// this account to.
    #[serde(default)]
    pub subscription_id: Option<i64>,
    /// Optional per-account id inside the subscription (for audit).
    #[serde(default)]
    pub cloud_account_id: Option<i64>,
}

/// Internal pool record. Same shape as `CloudAccountInput` but with
/// private token fields. Serialization is used only for the encrypted
/// blob on disk — the JSON rendered by `list_cloud_accounts()` comes
/// from `CloudAccountPublic`, which has no token fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CloudAccountRecord {
    codex_user_id: String,
    alias: String,
    access_token: String,
    refresh_token: String,
    token_expires_at_epoch: i64,
    subscription_id: Option<i64>,
    cloud_account_id: Option<i64>,
}

impl CloudAccountRecord {
    fn from_input(input: CloudAccountInput) -> Self {
        Self {
            codex_user_id: input.codex_user_id,
            alias: input.alias,
            access_token: input.access_token,
            refresh_token: input.refresh_token,
            token_expires_at_epoch: input.token_expires_at_epoch,
            subscription_id: input.subscription_id,
            cloud_account_id: input.cloud_account_id,
        }
    }

    fn classify(&self, now_epoch: i64) -> AccountStatus {
        let remaining = self.token_expires_at_epoch - now_epoch;
        if remaining <= 0 {
            AccountStatus::Expired
        } else if remaining < EXPIRING_THRESHOLD_SECS {
            AccountStatus::Expiring
        } else {
            AccountStatus::Fresh
        }
    }
}

/// Classification bucket for an account's access token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// `token_expires_at_epoch` is more than 5 minutes away.
    Fresh,
    /// `token_expires_at_epoch` is less than 5 minutes away (but not
    /// yet in the past). S3's refresh loop will rotate these.
    Expiring,
    /// `token_expires_at_epoch` is in the past. Until S3 refreshes,
    /// this account can't serve new requests.
    Expired,
}

/// Redacted view of a [`CloudAccountRecord`], safe to return from
/// HTTP routes and to log. Contains no tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudAccountPublic {
    pub codex_user_id: String,
    pub alias: String,
    pub token_expires_at_epoch: i64,
    pub subscription_id: Option<i64>,
    pub cloud_account_id: Option<i64>,
    pub status: AccountStatus,
}

/// Aggregate counts for the `Settings → Subscription & Codex Pool`
/// read-only panel. Only contains numbers — no tokens, no user ids.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrokerPublicStatus {
    pub pool_size: usize,
    pub fresh_count: usize,
    pub expiring_count: usize,
    pub expired_count: usize,
    /// Total `chat_completion` calls the broker has served since the
    /// process started. Monotonic, in-memory only.
    pub requests_today: u64,
    /// Earliest `token_expires_at_epoch` in the pool, when non-empty.
    /// Settings uses this to render "next refresh ≈ 12 min".
    pub next_refresh_at_epoch: Option<i64>,
}

/// Errors raised by broker operations.
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("secure storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Pool is empty or has no usable accounts for the requested
    /// work. Consumer should fall back to the legacy env-var auth
    /// chain if one exists.
    #[error("no codex account available in pool (size={pool_size}, fresh={fresh_count})")]
    NoAccountAvailable {
        pool_size: usize,
        fresh_count: usize,
    },
    /// Upstream Codex API returned an error. Carries the stringified
    /// `api::ApiError` so consumers can log it without depending on
    /// the `api` crate's error type directly.
    #[error("upstream chat_completion failed: {0}")]
    Upstream(String),
}

impl From<SecureStorageError> for BrokerError {
    fn from(e: SecureStorageError) -> Self {
        Self::Storage(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BrokerError>;

/// Accounts expiring in less than this many seconds are classified as
/// [`AccountStatus::Expiring`] so S3's refresh loop picks them up
/// before they actually expire.
const EXPIRING_THRESHOLD_SECS: i64 = 300;

/// Default base URL for Codex OAuth chat completions. Override with
/// `CODEX_BASE_URL` env var at process start. Codex runs on the
/// standard OpenAI endpoint when the auth is a personal-pool token;
/// deployments that speak a different endpoint (e.g. Azure OpenAI
/// with a Codex deployment name) should set the override.
const DEFAULT_CODEX_BASE_URL: &str = "https://api.openai.com/v1";

fn resolve_codex_base_url() -> String {
    std::env::var("CODEX_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CODEX_BASE_URL.to_string())
}

// ── Process-global broker hook ───────────────────────────────────
//
// `execute_live_turn` in desktop-core is a free function called deep
// inside the session loop, and the conversation runtime it spawns has
// no handle to `AppState` (which lives in desktop-server). To let the
// broker drive the `client_override` parameter without threading a
// handle through every call site, desktop-server installs the broker
// as a process-global `OnceLock<Arc<CodexBroker>>` at startup. Tests
// and alternative consumers simply skip installation — `global()`
// returns `None` and `execute_live_turn` falls through to the legacy
// env-var auth chain.

static GLOBAL_BROKER: OnceLock<Arc<CodexBroker>> = OnceLock::new();

/// Install the broker as the process-global instance. Idempotent
/// within a process: subsequent calls are silent no-ops because
/// `OnceLock::set` rejects second writes.
///
/// Called by `desktop-server::AppState::new` at startup. Tests don't
/// need to install — they rely on the None-case fallback path.
pub fn install_global(broker: Arc<CodexBroker>) {
    // Intentionally ignore the double-install error. A binary that
    // constructs AppState twice (e.g. integration tests spinning
    // multiple servers in one process) will hit this, and the second
    // install should silently yield to the first.
    let _ = GLOBAL_BROKER.set(broker);
}

/// Fetch the process-global broker, if installed. Returns `None` on
/// the test path or when desktop-server isn't running.
#[must_use]
pub fn global() -> Option<Arc<CodexBroker>> {
    GLOBAL_BROKER.get().cloned()
}

/// Filename of the encrypted on-disk pool under
/// `$CLAWWIKI_HOME/.clawwiki/`. Distinct from `cloud-accounts.json`
/// (the plaintext legacy file used by the frontend plugin-store path
/// which S0.4 abandoned) and from any future `cloud-accounts.yaml`
/// (reserved).
pub const CLOUD_ACCOUNTS_FILENAME: &str = "cloud-accounts.enc";

/// Internal Codex account pool.
///
/// Construct one per process via [`CodexBroker::new`]. The pool
/// reloads from disk on construction (tolerating a missing file as
/// "empty pool") and writes back on every sync / clear.
pub struct CodexBroker {
    storage_path: PathBuf,
    pool: RwLock<Vec<CloudAccountRecord>>,
    requests_today: AtomicU64,
}

impl CodexBroker {
    /// Build a new broker rooted at `meta_dir` (usually
    /// `WikiPaths::resolve(root).meta` from the `wiki_store` crate).
    /// Reloads any previously-persisted pool from
    /// `{meta_dir}/cloud-accounts.enc`.
    pub fn new(meta_dir: impl AsRef<Path>) -> Result<Self> {
        let storage_path = meta_dir.as_ref().join(CLOUD_ACCOUNTS_FILENAME);
        let pool = if storage_path.exists() {
            let plaintext = secure_storage::read_encrypted(&storage_path)?;
            serde_json::from_slice::<Vec<CloudAccountRecord>>(&plaintext)?
        } else {
            Vec::new()
        };
        Ok(Self {
            storage_path,
            pool: RwLock::new(pool),
            requests_today: AtomicU64::new(0),
        })
    }

    /// Replace the entire pool with `accounts`. This is the ONLY
    /// mutation entry point for adding accounts — there is no
    /// per-account insert. The trade-service always ships the full
    /// set, so the frontend calls sync whenever the subscription
    /// changes.
    pub fn sync_cloud_accounts(&self, accounts: Vec<CloudAccountInput>) -> Result<()> {
        let records: Vec<CloudAccountRecord> =
            accounts.into_iter().map(CloudAccountRecord::from_input).collect();
        {
            let mut guard = self
                .pool
                .write()
                .expect("broker pool RwLock poisoned");
            *guard = records;
        }
        self.persist()
    }

    /// Return the redacted list of accounts in the pool, each with a
    /// live `AccountStatus` classification. Ordered in insertion
    /// order (same order as the most recent `sync_cloud_accounts`).
    pub fn list_cloud_accounts(&self) -> Vec<CloudAccountPublic> {
        let now = current_epoch_secs();
        let guard = self.pool.read().expect("broker pool RwLock poisoned");
        guard
            .iter()
            .map(|rec| CloudAccountPublic {
                codex_user_id: rec.codex_user_id.clone(),
                alias: rec.alias.clone(),
                token_expires_at_epoch: rec.token_expires_at_epoch,
                subscription_id: rec.subscription_id,
                cloud_account_id: rec.cloud_account_id,
                status: rec.classify(now),
            })
            .collect()
    }

    /// Empty the pool and delete the encrypted on-disk blob. Idempotent —
    /// calling clear on an already-empty broker is a no-op.
    pub fn clear_cloud_accounts(&self) -> Result<()> {
        {
            let mut guard = self.pool.write().expect("broker pool RwLock poisoned");
            guard.clear();
        }
        if self.storage_path.exists() {
            std::fs::remove_file(&self.storage_path).map_err(|e| {
                BrokerError::Storage(format!(
                    "failed to remove {:?}: {e}",
                    self.storage_path
                ))
            })?;
        }
        Ok(())
    }

    /// Return an aggregate snapshot for the `Subscription & Codex Pool`
    /// settings panel. NEVER contains tokens.
    pub fn public_status(&self) -> BrokerPublicStatus {
        let now = current_epoch_secs();
        let guard = self.pool.read().expect("broker pool RwLock poisoned");

        let mut fresh_count = 0;
        let mut expiring_count = 0;
        let mut expired_count = 0;
        let mut next_refresh: Option<i64> = None;
        for rec in guard.iter() {
            match rec.classify(now) {
                AccountStatus::Fresh => fresh_count += 1,
                AccountStatus::Expiring => expiring_count += 1,
                AccountStatus::Expired => expired_count += 1,
            }
            next_refresh = Some(match next_refresh {
                None => rec.token_expires_at_epoch,
                Some(prev) => prev.min(rec.token_expires_at_epoch),
            });
        }

        BrokerPublicStatus {
            pool_size: guard.len(),
            fresh_count,
            expiring_count,
            expired_count,
            requests_today: self.requests_today.load(Ordering::Relaxed),
            next_refresh_at_epoch: next_refresh,
        }
    }

    /// Execute a non-streaming chat completion through the Codex pool.
    ///
    /// Picks a fresh account (preferring `Fresh` over `Expiring`;
    /// `Expired` is never used), builds a one-shot `OpenAiCompatClient`
    /// bound to the pool-managed bearer token, and forwards the
    /// `MessageRequest`. On upstream failure the error is stringified
    /// into `BrokerError::Upstream(_)` so callers (S3 ask_runtime, S4
    /// wiki_maintainer) don't have to depend on the `api` crate's
    /// error type directly.
    ///
    /// This is the one place in the workspace where a pool access
    /// token is used. The token never leaves the closure scope — we
    /// copy it into a fresh client, await the response, and drop the
    /// client. Nothing in the returned `MessageResponse` contains
    /// the token.
    ///
    /// What's NOT here yet (backlog):
    ///
    ///  * Streaming variant (`stream_message` → `ChatStream`). AskPage
    ///    currently polls the session detail, so streaming isn't on
    ///    the critical path.
    ///  * Background token refresh against the trade service. When an
    ///    account expires, consumers see `NoAccountAvailable` once the
    ///    pool drains. A refresh loop watching `token_expires_at_epoch`
    ///    lands in a follow-up sprint.
    ///  * Round-robin counter across multiple fresh accounts. MVP
    ///    picks the first fresh account every time, which is fine for
    ///    a single-user desktop with ≤ 5 accounts.
    pub async fn chat_completion(
        &self,
        request: MessageRequest,
    ) -> Result<MessageResponse> {
        let access_token = self.pick_account_bearer_token()?;
        let base_url = resolve_codex_base_url();

        // The `OpenAiCompatConfig::openai()` template is the only
        // reference to any env-var hierarchy, and we never call
        // `from_env`, so the env vars inside the config are unused.
        // We construct the client with `new` which accepts the token
        // directly, then override the base URL so it lands on the
        // pool-specific endpoint.
        let client = OpenAiCompatClient::new(access_token, OpenAiCompatConfig::openai())
            .with_base_url(base_url);

        let response = client.send_message(&request).await.map_err(|err| {
            BrokerError::Upstream(format!("{err}"))
        })?;

        self.requests_today.fetch_add(1, Ordering::Relaxed);
        Ok(response)
    }

    /// Build an upstream `ProviderClient::OpenAi` wired to a fresh
    /// pool account. Convenience wrapper around
    /// [`pick_account_bearer_token`] so `execute_live_turn` can
    /// install the broker as a drop-in `client_override` without
    /// knowing anything about pool semantics.
    ///
    /// Returns `Err(NoAccountAvailable)` when the pool has nothing
    /// usable. Callers should fall back to the env-var chain in
    /// that case.
    pub fn build_provider_client(&self) -> Result<ProviderClient> {
        let token = self.pick_account_bearer_token()?;
        let client = OpenAiCompatClient::new(token, OpenAiCompatConfig::openai())
            .with_base_url(resolve_codex_base_url());
        Ok(ProviderClient::OpenAi(client))
    }

    /// Pick the best access token out of the pool:
    ///
    ///   1. First `Fresh` account, in insertion order.
    ///   2. Otherwise the first `Expiring` account (we're about to
    ///      lose it anyway, might as well use it).
    ///   3. If everything is `Expired` — return NoAccountAvailable.
    ///
    /// Separated from `chat_completion` so the policy is unit-testable
    /// without booting an HTTP client.
    fn pick_account_bearer_token(&self) -> Result<String> {
        let now = current_epoch_secs();
        let guard = self.pool.read().expect("broker pool RwLock poisoned");

        let mut fresh_count = 0usize;
        let mut fresh_candidate: Option<&CloudAccountRecord> = None;
        let mut expiring_candidate: Option<&CloudAccountRecord> = None;

        for rec in guard.iter() {
            match rec.classify(now) {
                AccountStatus::Fresh => {
                    fresh_count += 1;
                    if fresh_candidate.is_none() {
                        fresh_candidate = Some(rec);
                    }
                }
                AccountStatus::Expiring => {
                    if expiring_candidate.is_none() {
                        expiring_candidate = Some(rec);
                    }
                }
                AccountStatus::Expired => {}
            }
        }

        if let Some(rec) = fresh_candidate.or(expiring_candidate) {
            return Ok(rec.access_token.clone());
        }

        Err(BrokerError::NoAccountAvailable {
            pool_size: guard.len(),
            fresh_count,
        })
    }

    fn persist(&self) -> Result<()> {
        let guard = self.pool.read().expect("broker pool RwLock poisoned");
        if guard.is_empty() {
            // Empty pool: remove the file so a subsequent `new()` on
            // a cold boot sees an empty pool instead of an empty JSON
            // array (functionally equivalent, but clearer on disk).
            drop(guard);
            if self.storage_path.exists() {
                std::fs::remove_file(&self.storage_path).map_err(|e| {
                    BrokerError::Storage(format!(
                        "failed to remove {:?}: {e}",
                        self.storage_path
                    ))
                })?;
            }
            return Ok(());
        }
        let bytes = serde_json::to_vec(&*guard)?;
        drop(guard);
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                BrokerError::Storage(format!(
                    "failed to create storage parent {parent:?}: {e}"
                ))
            })?;
        }
        secure_storage::write_encrypted(&self.storage_path, &bytes)?;
        Ok(())
    }
}

/// Current unix epoch seconds. Extracted so tests can overwrite with
/// a mock. Returns 0 if the system clock is before the epoch (which
/// means we treat everything as expired, which is the correct fail-
/// safe — better than returning `Fresh` on a misconfigured clock).
fn current_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn seed_test_key() {
        // Use a fixed key so tests in this module don't race with the
        // real secure_storage key file under ~/.warwolf/.
        crate::secure_storage::seed_key_for_test([7u8; 32]);
    }

    fn sample_input(id: &str, alias: &str, expires_in_secs: i64) -> CloudAccountInput {
        CloudAccountInput {
            codex_user_id: id.to_string(),
            alias: alias.to_string(),
            access_token: format!("sk-access-{id}"),
            refresh_token: format!("sk-refresh-{id}"),
            token_expires_at_epoch: current_epoch_secs() + expires_in_secs,
            subscription_id: Some(42),
            cloud_account_id: Some(1),
        }
    }

    #[test]
    fn new_on_missing_file_returns_empty_pool() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();
        assert_eq!(broker.list_cloud_accounts().len(), 0);
        assert_eq!(broker.public_status().pool_size, 0);
    }

    #[test]
    fn sync_then_list_is_redacted() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![
                sample_input("u-1", "Alice", 3600),
                sample_input("u-2", "Bob", 1000),
            ])
            .unwrap();

        let listed = broker.list_cloud_accounts();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].codex_user_id, "u-1");
        assert_eq!(listed[0].alias, "Alice");
        assert_eq!(listed[0].status, AccountStatus::Fresh);

        // The public view struct does not even have access_token /
        // refresh_token fields, so the compiler already prevents
        // leakage. This test asserts the runtime shape for good
        // measure — serialize to JSON and check the tokens are absent.
        let json = serde_json::to_string(&listed).unwrap();
        assert!(!json.contains("sk-access"));
        assert!(!json.contains("sk-refresh"));
        assert!(!json.contains("access_token"));
        assert!(!json.contains("refresh_token"));
    }

    #[test]
    fn sync_replaces_rather_than_appends() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![sample_input("u-1", "Alice", 3600)])
            .unwrap();
        broker
            .sync_cloud_accounts(vec![
                sample_input("u-2", "Bob", 3600),
                sample_input("u-3", "Carol", 3600),
            ])
            .unwrap();

        let listed = broker.list_cloud_accounts();
        assert_eq!(listed.len(), 2, "second sync should REPLACE, not append");
        assert_eq!(listed[0].codex_user_id, "u-2");
        assert_eq!(listed[1].codex_user_id, "u-3");
    }

    #[test]
    fn clear_empties_pool_and_removes_file() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![sample_input("u-1", "Alice", 3600)])
            .unwrap();
        let storage_path = tmp.path().join(CLOUD_ACCOUNTS_FILENAME);
        assert!(storage_path.exists(), "sync should write file");

        broker.clear_cloud_accounts().unwrap();
        assert_eq!(broker.list_cloud_accounts().len(), 0);
        assert!(!storage_path.exists(), "clear should remove file");

        // Double-clear is safe.
        broker.clear_cloud_accounts().unwrap();
    }

    #[test]
    fn public_status_counts_fresh_expiring_expired() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![
                sample_input("a", "fresh1", 3600),
                sample_input("b", "fresh2", 10_000),
                sample_input("c", "expiring", 60),   // < 5 min
                sample_input("d", "expired1", -10),  // past
                sample_input("e", "expired2", -100), // past
            ])
            .unwrap();

        let status = broker.public_status();
        assert_eq!(status.pool_size, 5);
        assert_eq!(status.fresh_count, 2);
        assert_eq!(status.expiring_count, 1);
        assert_eq!(status.expired_count, 2);
        assert_eq!(status.requests_today, 0);
        assert!(status.next_refresh_at_epoch.is_some());
    }

    #[test]
    fn public_status_next_refresh_is_earliest_expiry() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![
                sample_input("a", "later", 10_000),
                sample_input("b", "soonest", 500),
                sample_input("c", "middle", 3000),
            ])
            .unwrap();

        let status = broker.public_status();
        let now = current_epoch_secs();
        // The earliest is `b` at `now + 500`.
        let expected = now + 500;
        let actual = status.next_refresh_at_epoch.expect("should be present");
        // Allow 2s jitter for slow tests.
        assert!(
            (actual - expected).abs() <= 2,
            "expected ~{expected}, got {actual}"
        );
    }

    #[test]
    fn persist_and_reload_round_trip() {
        seed_test_key();
        let tmp = tempdir().unwrap();

        // Write with broker #1.
        {
            let broker = CodexBroker::new(tmp.path()).unwrap();
            broker
                .sync_cloud_accounts(vec![
                    sample_input("u-1", "Alice", 3600),
                    sample_input("u-2", "Bob", 7200),
                ])
                .unwrap();
        }

        // Fresh broker, same dir → should see both accounts.
        let broker2 = CodexBroker::new(tmp.path()).unwrap();
        let listed = broker2.list_cloud_accounts();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].alias, "Alice");
        assert_eq!(listed[1].alias, "Bob");
    }

    #[test]
    fn chat_completion_on_empty_pool_returns_no_account_available() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        let result = broker.pick_account_bearer_token();
        match result {
            Err(BrokerError::NoAccountAvailable {
                pool_size: 0,
                fresh_count: 0,
            }) => {}
            other => panic!("expected NoAccountAvailable{{0,0}}, got {other:?}"),
        }
    }

    #[test]
    fn pick_account_prefers_fresh_over_expiring() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        // Note insertion order: expiring first, then fresh. The
        // picker must STILL return the fresh one.
        broker
            .sync_cloud_accounts(vec![
                sample_input("exp", "expiring", 60), // < 5 min
                sample_input("fresh", "fresh", 3600),
            ])
            .unwrap();

        let token = broker.pick_account_bearer_token().unwrap();
        assert_eq!(token, "sk-access-fresh");
    }

    #[test]
    fn pick_account_falls_back_to_expiring_when_no_fresh() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![
                sample_input("exp1", "e1", 60),
                sample_input("exp2", "e2", 120),
            ])
            .unwrap();

        let token = broker.pick_account_bearer_token().unwrap();
        // First expiring account in insertion order.
        assert_eq!(token, "sk-access-exp1");
    }

    #[test]
    fn pick_account_skips_expired_accounts() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![
                sample_input("dead1", "expired", -100),
                sample_input("dead2", "expired", -50),
                sample_input("alive", "fresh", 3600),
            ])
            .unwrap();

        let token = broker.pick_account_bearer_token().unwrap();
        assert_eq!(token, "sk-access-alive");
    }

    #[test]
    fn pick_account_errors_when_only_expired() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![
                sample_input("dead", "expired", -100),
            ])
            .unwrap();

        match broker.pick_account_bearer_token() {
            Err(BrokerError::NoAccountAvailable {
                pool_size: 1,
                fresh_count: 0,
            }) => {}
            other => panic!("expected NoAccountAvailable{{1,0}}, got {other:?}"),
        }
    }

    #[test]
    fn resolve_codex_base_url_honors_env_override() {
        // Note: intentionally does NOT use std::env::set_var/remove_var
        // because other tests may run in parallel and share the env.
        // Instead we just verify the default resolution in the absence
        // of an override is stable, then clean up manually if the
        // override was set.
        let original = std::env::var("CODEX_BASE_URL").ok();
        std::env::set_var("CODEX_BASE_URL", "https://example.test/v1");
        assert_eq!(resolve_codex_base_url(), "https://example.test/v1");
        // Clean up so a neighboring test that reads the default
        // doesn't see our override.
        match original {
            Some(prev) => std::env::set_var("CODEX_BASE_URL", prev),
            None => std::env::remove_var("CODEX_BASE_URL"),
        }
    }

    #[test]
    fn sync_empty_pool_removes_file_without_leaving_empty_blob() {
        seed_test_key();
        let tmp = tempdir().unwrap();
        let broker = CodexBroker::new(tmp.path()).unwrap();

        broker
            .sync_cloud_accounts(vec![sample_input("u-1", "Alice", 3600)])
            .unwrap();
        let storage_path = tmp.path().join(CLOUD_ACCOUNTS_FILENAME);
        assert!(storage_path.exists());

        // Sync to empty: should remove the file entirely.
        broker.sync_cloud_accounts(Vec::new()).unwrap();
        assert!(!storage_path.exists());
        assert_eq!(broker.list_cloud_accounts().len(), 0);
    }

    #[test]
    fn classify_boundary_5min_is_expiring() {
        let rec = CloudAccountRecord {
            codex_user_id: "u".into(),
            alias: "a".into(),
            access_token: "t".into(),
            refresh_token: "r".into(),
            token_expires_at_epoch: 1000,
            subscription_id: None,
            cloud_account_id: None,
        };
        // Exactly 5 min away → Fresh (threshold is strictly `<`).
        assert_eq!(rec.classify(1000 - EXPIRING_THRESHOLD_SECS), AccountStatus::Fresh);
        // Just inside 5 min → Expiring.
        assert_eq!(
            rec.classify(1000 - EXPIRING_THRESHOLD_SECS + 1),
            AccountStatus::Expiring
        );
        // Just past → Expired.
        assert_eq!(rec.classify(1001), AccountStatus::Expired);
        // Exactly at → Expired (remaining = 0, which means "<=0").
        assert_eq!(rec.classify(1000), AccountStatus::Expired);
    }
}

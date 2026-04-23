//! `TaskManager` — tracks in-flight async SKILL tasks and enforces
//! one-per-kind concurrency (the `absorb_handler` 409 semantics).
//!
//! Canonical spec: `docs/design/technical-design.md §4.5.2` (L1977+).
//!
//! ## Responsibilities
//!
//! 1. **Registration** — `register(kind)` mints a `{kind}-{ts}-{hex}`
//!    task id + a fresh `CancellationToken`, stores both, returns them
//!    to the caller. Returns `Err(TaskConflictError)` if another task
//!    of the same `kind` is already active (this is the §2.1 "409
//!    `ABSORB_IN_PROGRESS`" behaviour, generalised over SKILL kinds).
//! 2. **Completion** — `complete(task_id)` removes the entry so future
//!    `register(kind)` calls of the same kind succeed again.
//! 3. **Cancellation** — `cancel(task_id)` trips the stored token,
//!    letting the running `absorb_batch` / `query_wiki` / patrol loop
//!    observe cancellation on its next iteration check. The caller is
//!    still responsible for calling `complete(task_id)` once the task
//!    has actually unwound.
//! 4. **Query** — `is_running(kind)` and `list_active()` expose state
//!    for HTTP handlers that need to render a status card or refuse
//!    new work.
//!
//! ## Scope in Sprint 1-B.1
//!
//! Step 2 lands the standalone `TaskManager` + integrates it into
//! `DesktopState`. Step 6 of the same sprint rewrites
//! `absorb_handler` to use it (replacing the local
//! `ABSORB_RUNNING: AtomicBool`) and wires the cancel token into
//! `absorb_batch`. Until step 6 lands, the manager holds a single
//! task registry that nobody populates — harmless.
//!
//! ## Why the registry is an `RwLock<HashMap>` and not a slab / dashmap
//!
//! The canonical spec pins `RwLock<HashMap<String, TaskInfo>>`. The
//! expected concurrent-write rate is single-digit QPS (HTTP POST
//! requests gated by §2.1 409) and the map is tiny (<10 active
//! tasks). The `RwLock` is ample. We can swap for `DashMap` if the
//! concurrency grows by 2+ orders of magnitude.

use std::collections::HashMap;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Metadata for a single in-flight SKILL task.
///
/// Cloneable (the [`CancellationToken`] clones cheaply via an internal
/// `Arc`) so callers that want to hold a snapshot of active tasks can
/// iterate without holding the registry lock.
#[derive(Debug, Clone)]
pub struct TaskInfo {
    /// Canonical form: `{kind}-{unix_ts}-{4hex}`. Minted by `register`.
    pub task_id: String,
    /// One of `"absorb"`, `"query"`, `"cleanup"`, `"patrol"`.
    pub kind: String,
    /// ISO-8601 timestamp captured at `register` time. Formatted the
    /// same way as `wiki_store::now_iso8601` so it round-trips with the
    /// absorb log and other ISO-8601 fields.
    pub started_at: String,
    /// Trips when `cancel(task_id)` is called. The task body's
    /// `CancellationToken` must be this same token (via `.clone()`)
    /// so the cascade from HTTP → running loop works.
    pub cancel_token: CancellationToken,
}

/// Error surfaced by [`TaskManager::register`] when a task of the same
/// `kind` is already running. The HTTP layer maps this to `409
/// Conflict` with `{"error": "<KIND>_IN_PROGRESS"}`.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TaskConflictError {
    /// Another task of this `kind` is already in the registry.
    #[error("a {0} task is already running")]
    KindAlreadyRunning(String),
}

/// In-memory registry of running SKILL tasks. See the module-level docs.
pub struct TaskManager {
    active_tasks: RwLock<HashMap<String, TaskInfo>>,
}

impl TaskManager {
    /// Fresh, empty registry. Cheap — allocates only the `HashMap`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new task of the given `kind`. Returns `(task_id,
    /// cancel_token)` on success; the caller must pass the cancel
    /// token into the actual task body (e.g. `absorb_batch(..., token)`)
    /// and must eventually call [`Self::complete`] when the task ends
    /// for ANY reason (success, error, cancel) so the kind slot frees.
    ///
    /// Returns [`TaskConflictError::KindAlreadyRunning`] if another
    /// task with the same `kind` is currently active. This is the
    /// one-per-kind guarantee §2.1 relies on for the 409 response.
    pub async fn register(
        &self,
        kind: &str,
    ) -> Result<(String, CancellationToken), TaskConflictError> {
        let mut tasks = self.active_tasks.write().await;
        if tasks.values().any(|t| t.kind == kind) {
            return Err(TaskConflictError::KindAlreadyRunning(kind.to_string()));
        }
        let task_id = generate_task_id(kind);
        let cancel_token = CancellationToken::new();
        let info = TaskInfo {
            task_id: task_id.clone(),
            kind: kind.to_string(),
            started_at: wiki_store::now_iso8601(),
            cancel_token: cancel_token.clone(),
        };
        tasks.insert(task_id.clone(), info);
        Ok((task_id, cancel_token))
    }

    /// Remove the task from the registry. Safe to call multiple times
    /// with the same id (subsequent calls are no-ops). Typical call
    /// site: the `tokio::spawn` body's guard struct's `Drop` impl.
    pub async fn complete(&self, task_id: &str) {
        let mut tasks = self.active_tasks.write().await;
        tasks.remove(task_id);
    }

    /// Trip the cancel token for the given task id. Returns `true` if
    /// the task existed (and thus the token was tripped), `false` if
    /// the id was unknown (e.g. already completed / never registered).
    ///
    /// Does **not** remove the entry from the registry — the running
    /// task should call [`Self::complete`] when it actually unwinds.
    pub async fn cancel(&self, task_id: &str) -> bool {
        let tasks = self.active_tasks.read().await;
        match tasks.get(task_id) {
            Some(info) => {
                info.cancel_token.cancel();
                true
            }
            None => false,
        }
    }

    /// `true` iff any task with the given `kind` is currently in the
    /// registry. Used by HTTP handlers to skip work rather than hit a
    /// 409 path.
    pub async fn is_running(&self, kind: &str) -> bool {
        let tasks = self.active_tasks.read().await;
        tasks.values().any(|t| t.kind == kind)
    }

    /// Snapshot of all currently-active tasks. Returns clones so the
    /// caller doesn't hold the registry lock while rendering UI /
    /// serialising to JSON.
    pub async fn list_active(&self) -> Vec<TaskInfo> {
        let tasks = self.active_tasks.read().await;
        tasks.values().cloned().collect()
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Format: `{kind}-{unix_secs}-{4hex}`. Matches the task id shape used
/// by `SkillRouter::generate_task_id` and the §2.1 response spec.
fn generate_task_id(kind: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{kind}-{}-{:04x}", t.as_secs(), t.subsec_nanos() & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_returns_unique_task_id_and_token() {
        let mgr = TaskManager::new();
        let (id, token) = mgr.register("absorb").await.unwrap();
        assert!(id.starts_with("absorb-"));
        assert!(!token.is_cancelled());
        assert!(mgr.is_running("absorb").await);
    }

    #[tokio::test]
    async fn register_duplicate_kind_returns_conflict() {
        let mgr = TaskManager::new();
        let _ = mgr.register("absorb").await.unwrap();
        let err = mgr.register("absorb").await.unwrap_err();
        match err {
            TaskConflictError::KindAlreadyRunning(kind) => assert_eq!(kind, "absorb"),
        }
    }

    #[tokio::test]
    async fn register_different_kinds_coexist() {
        let mgr = TaskManager::new();
        let _ = mgr.register("absorb").await.unwrap();
        let _ = mgr.register("cleanup").await.unwrap();
        let _ = mgr.register("patrol").await.unwrap();
        assert!(mgr.is_running("absorb").await);
        assert!(mgr.is_running("cleanup").await);
        assert!(mgr.is_running("patrol").await);
    }

    #[tokio::test]
    async fn complete_removes_task_and_frees_kind_slot() {
        let mgr = TaskManager::new();
        let (id, _) = mgr.register("absorb").await.unwrap();
        assert!(mgr.is_running("absorb").await);
        mgr.complete(&id).await;
        assert!(!mgr.is_running("absorb").await);
        // After complete, a new register of same kind succeeds.
        let _ = mgr.register("absorb").await.unwrap();
    }

    #[tokio::test]
    async fn complete_is_idempotent() {
        let mgr = TaskManager::new();
        let (id, _) = mgr.register("cleanup").await.unwrap();
        mgr.complete(&id).await;
        mgr.complete(&id).await; // second call: no-op
        mgr.complete("unknown-id-42").await; // unknown id: no-op
        assert!(!mgr.is_running("cleanup").await);
    }

    #[tokio::test]
    async fn cancel_trips_token_and_returns_true() {
        let mgr = TaskManager::new();
        let (id, token) = mgr.register("absorb").await.unwrap();
        assert!(!token.is_cancelled());
        let ok = mgr.cancel(&id).await;
        assert!(ok);
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_unknown_returns_false() {
        let mgr = TaskManager::new();
        let ok = mgr.cancel("missing-123").await;
        assert!(!ok);
    }

    #[tokio::test]
    async fn cancel_does_not_remove_entry() {
        // Cancelling flips the token but leaves the registry entry in
        // place; the task body must still call `complete(id)` when it
        // actually unwinds.
        let mgr = TaskManager::new();
        let (id, _) = mgr.register("patrol").await.unwrap();
        mgr.cancel(&id).await;
        assert!(mgr.is_running("patrol").await);
        // Only complete() removes the entry.
        mgr.complete(&id).await;
        assert!(!mgr.is_running("patrol").await);
    }

    #[tokio::test]
    async fn is_running_false_for_unknown_kind() {
        let mgr = TaskManager::new();
        assert!(!mgr.is_running("absorb").await);
        assert!(!mgr.is_running("does-not-exist").await);
    }

    #[tokio::test]
    async fn list_active_returns_all_currently_running() {
        let mgr = TaskManager::new();
        let (absorb_id, _) = mgr.register("absorb").await.unwrap();
        let (cleanup_id, _) = mgr.register("cleanup").await.unwrap();
        let list = mgr.list_active().await;
        assert_eq!(list.len(), 2);
        let ids: Vec<&str> = list.iter().map(|t| t.task_id.as_str()).collect();
        assert!(ids.contains(&absorb_id.as_str()));
        assert!(ids.contains(&cleanup_id.as_str()));
    }

    #[tokio::test]
    async fn list_active_empty_when_no_tasks() {
        let mgr = TaskManager::new();
        assert!(mgr.list_active().await.is_empty());
    }

    #[tokio::test]
    async fn default_impl_equivalent_to_new() {
        let a = TaskManager::default();
        let b = TaskManager::new();
        assert!(a.list_active().await.is_empty());
        assert!(b.list_active().await.is_empty());
    }

    #[tokio::test]
    async fn task_info_carries_iso8601_timestamp() {
        let mgr = TaskManager::new();
        let (id, _) = mgr.register("absorb").await.unwrap();
        let list = mgr.list_active().await;
        let info = list.iter().find(|t| t.task_id == id).unwrap();
        // ISO-8601 minimum sanity: starts with 4-digit year + dash.
        let chars: Vec<char> = info.started_at.chars().take(5).collect();
        assert!(chars[0].is_ascii_digit(), "year[0]: {}", info.started_at);
        assert!(chars[3].is_ascii_digit(), "year[3]: {}", info.started_at);
        assert_eq!(chars[4], '-', "ISO-8601 'YYYY-': {}", info.started_at);
    }
}

//! In-memory ring buffer of recent URL ingest decisions for diagnostics.
//!
//! M3 observability: the orchestrator calls [`push`] after every
//! [`crate::url_ingest::ingest_url`] terminal outcome. `desktop-server`
//! exposes a snapshot through `GET /api/desktop/url-ingest/recent` so
//! developers and the WeChat Bridge diagnostics panel can inspect
//! "why was this reused / suppressed / rejected".
//!
//! Capacity is capped at [`RECENT_LOG_CAPACITY`] (default 100). The
//! buffer lives in a static `OnceLock<RecentIngestLog>`; restart clears
//! it — diagnostics are transient by design (no persistence).
//!
//! Concurrency: a plain `std::sync::Mutex<VecDeque<…>>` — `push`/
//! `snapshot` are both O(1)/O(n) under hundreds-of-entries scale and
//! never held across `await` points, so there is no contention worth
//! reaching for `parking_lot` over. The mutex is also never poisoned in
//! practice because neither operation panics while holding the guard.
//!
//! ### IngestDecision coupling (M3 integration note)
//!
//! M3 Worker A is adding a structured `IngestDecision` type in
//! `url_ingest::dedupe` (canonical / chosen-strategy / reuse-reason
//! fields). Until that lands, we serialise the decision as an opaque
//! `serde_json::Value` so this module can compile independently. When
//! Worker A merges, Main can:
//!
//!   * add `pub mod dedupe;` to `url_ingest/mod.rs`,
//!   * swap [`RecentIngestEntry::decision`]'s field type to
//!     `Option<crate::url_ingest::dedupe::IngestDecision>`,
//!
//! without any call-site churn — serialised JSON stays wire-compatible
//! because `IngestDecision` derives `Serialize`.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Max entries retained in the ring buffer. When exceeded, the oldest
/// entry is evicted. 100 is plenty for operator debugging — the WeChat
/// Bridge diagnostic panel paginates by 20.
pub const RECENT_LOG_CAPACITY: usize = 100;

/// One terminal decision from the URL ingest orchestrator.
///
/// Serialised verbatim by the `GET /api/desktop/url-ingest/recent`
/// endpoint, so field names are the wire contract with the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentIngestEntry {
    /// Epoch millis when the decision was finalised (see [`now_millis`]).
    pub timestamp_ms: u64,
    /// Canonicalised URL (post-normalization by `url_ingest::canonical`).
    pub canonical_url: String,
    /// Original URL as received from the caller, pre-trim, pre-canonical.
    /// Kept for debugging canonicalization regressions.
    pub original_url: String,
    /// Entry point tag from [`crate::url_ingest::IngestRequest::origin_tag`]
    /// — e.g. `"ask-enrich"`, `"ilink"`, `"kefu"`, `"wechat-fetch"`.
    pub entry_point: String,
    /// Terminal outcome classification. Stable short strings:
    ///   `"ingested"`, `"reused_existing"`, `"inbox_suppressed"`,
    ///   `"fallback_to_text"`, `"rejected_quality"`, `"fetch_failed"`,
    ///   `"prerequisite_missing"`, `"invalid_url"`.
    pub outcome_kind: String,
    /// Structured decision payload for success paths. During M3 staging
    /// this is an opaque JSON blob; Worker A's `dedupe::IngestDecision`
    /// will replace the value type at integration time (JSON stays
    /// wire-compatible).
    pub decision: Option<serde_json::Value>,
    /// Raw id when a raw was persisted or an existing one reused.
    pub raw_id: Option<u32>,
    /// Inbox id when an inbox task was created or an existing one reused.
    pub inbox_id: Option<u32>,
    /// Which adapter path ran (e.g. `"playwright"`, `"http"`, `"wechat-fetch"`).
    /// `None` when the decision short-circuited before adapter selection
    /// (invalid URL, prerequisite missing before fetch, etc.).
    pub adapter: Option<String>,
    /// Full-operation duration in millis (fetch + validate + persist).
    /// `None` when not measured.
    pub duration_ms: Option<u64>,
    /// Human-readable one-liner that mirrors
    /// [`crate::url_ingest::IngestOutcome::as_display`].
    pub summary: String,
    /// M4: human-readable reason string derived from the
    /// [`crate::url_ingest::IngestDecision::reason`]. Carries the inbox
    /// state classification (pending / approved / rejected / silent) or
    /// content-dedupe tag (content_duplicate / refreshed_content) so
    /// diagnostics can filter without decoding the full `decision` JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<String>,
    /// M4: SHA-256 of the cleaned body when available. Hex-encoded
    /// (64 chars lowercase). `None` when the decision short-circuited
    /// before a fetch (URL-level dedupe hit, invalid URL, fetch
    /// failure, empty body).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// M4: `Some(true)` when the terminal decision was driven by a
    /// content-hash match (either `ContentDuplicate` reusing an
    /// existing raw, or `RefreshedContent` detecting a content drift
    /// on the same URL). `None` on decisions where content identity
    /// was not a factor (URL-only reuse, first-time ingest, errors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash_hit: Option<bool>,
}

/// Bounded ring buffer behind a mutex. Public so tests can instantiate
/// a private log without touching the global [`log`] singleton.
pub struct RecentIngestLog {
    entries: Mutex<VecDeque<RecentIngestEntry>>,
}

impl RecentIngestLog {
    /// Create an empty log sized to [`RECENT_LOG_CAPACITY`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(RECENT_LOG_CAPACITY)),
        }
    }

    /// Append a decision, evicting the oldest entry when full.
    ///
    /// Mutex poisoning is handled by recovering the inner data — a
    /// poisoned log still surfaces the most recent decisions, which
    /// is exactly what diagnostics need.
    pub fn push(&self, entry: RecentIngestEntry) {
        let mut q = match self.entries.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if q.len() >= RECENT_LOG_CAPACITY {
            q.pop_front();
        }
        q.push_back(entry);
    }

    /// Return up to `limit` entries in newest-first order. When `limit`
    /// is `None`, return everything currently retained. Cheap clone —
    /// the endpoint handler calls this with the mutex briefly held.
    #[must_use]
    pub fn snapshot(&self, limit: Option<usize>) -> Vec<RecentIngestEntry> {
        let q = match self.entries.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let take = limit.unwrap_or(RECENT_LOG_CAPACITY).min(q.len());
        q.iter().rev().take(take).cloned().collect()
    }

    /// Drop every retained entry. Currently only used by tests; kept
    /// public so a future `/api/desktop/url-ingest/recent/clear`
    /// endpoint can reuse it without adding more plumbing.
    pub fn clear(&self) {
        if let Ok(mut q) = self.entries.lock() {
            q.clear();
        }
    }
}

impl Default for RecentIngestLog {
    fn default() -> Self {
        Self::new()
    }
}

static RECENT_LOG: OnceLock<RecentIngestLog> = OnceLock::new();

/// Global singleton accessor. Lazily initialised on first use —
/// process restart wipes the buffer (diagnostics are transient).
pub fn log() -> &'static RecentIngestLog {
    RECENT_LOG.get_or_init(RecentIngestLog::new)
}

/// Push a decision into the global log. Called from the orchestrator's
/// terminal branches.
pub fn push(entry: RecentIngestEntry) {
    log().push(entry);
}

/// Newest-first snapshot from the global log.
#[must_use]
pub fn snapshot(limit: Option<usize>) -> Vec<RecentIngestEntry> {
    log().snapshot(limit)
}

/// Current epoch millis helper. Returns `0` if the system clock is
/// set before the UNIX epoch (impossible in practice but avoids an
/// `unwrap` in a path that should never panic).
#[must_use]
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_entry(ts: u64) -> RecentIngestEntry {
        RecentIngestEntry {
            timestamp_ms: ts,
            canonical_url: format!("https://example.com/{ts}"),
            original_url: format!("https://example.com/{ts}"),
            entry_point: "test".into(),
            outcome_kind: "ingested".into(),
            decision: None,
            raw_id: Some(ts as u32),
            inbox_id: None,
            adapter: None,
            duration_ms: None,
            summary: String::new(),
            decision_reason: None,
            content_hash: None,
            content_hash_hit: None,
        }
    }

    #[test]
    fn push_and_snapshot_newest_first() {
        let log = RecentIngestLog::new();
        for i in 0..5 {
            log.push(mk_entry(i));
        }
        let snap = log.snapshot(Some(3));
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].timestamp_ms, 4, "newest entry should come first");
        assert_eq!(snap[1].timestamp_ms, 3);
        assert_eq!(snap[2].timestamp_ms, 2);
    }

    #[test]
    fn cap_at_capacity() {
        let log = RecentIngestLog::new();
        for i in 0..(RECENT_LOG_CAPACITY as u64 + 10) {
            log.push(mk_entry(i));
        }
        let snap = log.snapshot(None);
        assert_eq!(snap.len(), RECENT_LOG_CAPACITY);
        // Newest retained entry is the last one pushed.
        assert_eq!(snap[0].timestamp_ms, RECENT_LOG_CAPACITY as u64 + 9);
        // Oldest retained entry: we pushed 110 total, kept latest 100,
        // so the oldest kept has ts = 10.
        assert_eq!(snap[snap.len() - 1].timestamp_ms, 10);
    }

    #[test]
    fn snapshot_limit_saturates_to_len() {
        let log = RecentIngestLog::new();
        log.push(mk_entry(1));
        log.push(mk_entry(2));
        let snap = log.snapshot(Some(999));
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn clear_empties_buffer() {
        let log = RecentIngestLog::new();
        log.push(mk_entry(1));
        log.clear();
        assert!(log.snapshot(None).is_empty());
    }

    #[test]
    fn now_millis_is_monotonic_ish() {
        // We can't guarantee strict monotonicity across calls (the clock
        // can skew), but we can sanity-check we got a plausible epoch.
        let t = now_millis();
        // 2024-01-01 = 1_704_067_200_000 ms. Anything below indicates a
        // badly misconfigured host — accept a generous lower bound.
        assert!(t > 1_700_000_000_000, "now_millis returned {t}");
    }
}

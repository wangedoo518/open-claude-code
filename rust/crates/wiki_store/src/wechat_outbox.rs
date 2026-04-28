//! `wechat_outbox` — durable WeChat reply queue (R1.2 reliability sprint).
//!
//! Every reply that the kefu / iLink handler tries to send is enqueued
//! into `_wechat_outbox.json` **before** the network call. On send-success
//! the entry transitions to `Sent`; on transient failure it bounces back
//! to `Pending` with exponential backoff (`next_retry_at`) and an
//! `attempts` bump; on terminal failure (API errcode, attempts cap)
//! it lands as `Failed`.
//!
//! ## Why durable
//!
//! Pre-R1.2 the kefu handler called `let _ = client.send_text(...)`.
//! When the WeChat HTTP endpoint was momentarily unreachable (anti-bot
//! page, DNS hiccup, expired access token), the failure surfaced as
//! `eprintln!` and the user saw "一条可以，一条没回复" with no
//! recovery path. The outbox makes every reply replayable across
//! crashes and gives the UI a complete view of in-flight / failed
//! deliveries.
//!
//! ## State machine
//!
//! ```text
//!   enqueue      claim          ok
//!   ─────► Pending ─────► Sending ─────► Sent
//!                            │
//!                  attempts<MAX, transport
//!                            │
//!                            ├──► Pending (with next_retry_at)
//!                            │
//!                  attempts>=MAX OR api/session_expired
//!                            │
//!                            └──► Failed
//!
//!   any-nonterminal ────► Cancelled (manual)
//! ```
//!
//! On process startup, [`reconcile_outbox_on_startup`] reverts every
//! `Sending` row to `Pending` so a crash mid-send is not lost.
//!
//! ## On-disk format
//!
//! `{meta}/_wechat_outbox.json` — single JSON array, atomic write
//! (`json.tmp` + rename), pretty-printed for human inspection. Same
//! storage convention as `_absorb_log.json` and `inbox.json`.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{format_iso8601, now_iso8601, Result, WikiPaths, WikiStoreError};

/// Filename for the outbox persistence file under `{meta}/`.
pub const OUTBOX_FILENAME: &str = "_wechat_outbox.json";

/// Maximum number of send attempts before an entry is permanently
/// `Failed`. After 8 attempts with the schedule below, total wall-clock
/// time spent retrying is ~2 hours — long enough to ride out most
/// transport hiccups, short enough that the UI shows a clean failure
/// signal rather than retrying forever.
pub const MAX_OUTBOX_ATTEMPTS: u32 = 8;

/// Backoff base in seconds. Schedule is `min(BASE * 2^attempts, CAP)`,
/// so attempts 1..=8 produce delays 30s, 1m, 2m, 4m, 8m, 16m, 32m, 1h.
const BACKOFF_BASE_SECS: u64 = 30;

/// Backoff cap in seconds. After ~32 minutes, every subsequent retry
/// is exactly 1 hour out — bounded retry pressure on the WeChat API.
const BACKOFF_CAP_SECS: u64 = 3_600;

/// Process-global guard serializing read-modify-write access to
/// `_wechat_outbox.json`. Same rationale as `INBOX_WRITE_GUARD`:
/// concurrent appenders / status mutators race on the load → mutate
/// → save cycle without it. Poison recovery is safe because every
/// failure path leaves the file in either its pre-write or
/// post-rename state.
static OUTBOX_WRITE_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_outbox_writes() -> MutexGuard<'static, ()> {
    OUTBOX_WRITE_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Transport-tagged identity of a WeChat outbox row. Different
/// transports carry different routing fields; the tagged enum keeps
/// the on-disk shape readable and avoids Optional-field soup.
///
/// R1.2 ships `Kefu` only — iLink already has an in-memory retry
/// helper and is a follow-up. The enum is open-ended so adding
/// `Ilink {...}` later is purely additive.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum OutboxTransport {
    /// WeChat customer-service (kefu) reply.
    Kefu {
        /// External user id supplied by the kefu inbound message.
        external_userid: String,
        /// Customer-service open kfid the message was directed at.
        open_kfid: String,
        /// `msgid` (uuid v4) generated on first send attempt and
        /// reused across retries so the WeChat API's de-dupe (which
        /// is keyed by `msgid`) does not re-deliver the same reply
        /// as a fresh message. `None` until first
        /// [`mark_outbox_sending`] call. See R1.2 plan §10 (ii).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        msgid: Option<String>,
    },
}

/// Lifecycle state of an outbox entry.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    /// Newly enqueued, not yet sent. `next_retry_at` may be `None`
    /// (immediate) or a future timestamp (after a transient failure).
    Pending,
    /// A worker has claimed the row and is mid-send. On crash, the
    /// startup reconcile reverts this to `Pending`.
    Sending,
    /// Successfully delivered. Terminal state.
    Sent,
    /// Permanent failure: API errcode, session expired, or attempts
    /// exhausted. Manual retry resets to `Pending` with `attempts=0`.
    Failed,
    /// User-initiated stop. Terminal until manual retry.
    Cancelled,
}

/// Last-error breakdown so the UI can show "needs re-login" vs
/// "transport flap" vs "permanent API error" without parsing strings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutboxLastError {
    /// Coarse error class:
    ///   * `"transport"` — HTTP / DNS / TCP failure; retry helps.
    ///   * `"timeout"` — request did not complete; retry helps.
    ///   * `"api"` — non-zero errcode from WeChat. Terminal.
    ///   * `"session_expired"` — auth blob no longer valid (iLink
    ///     `errcode -14`). Terminal until user re-logs in.
    pub kind: String,
    /// Human-readable message; intentionally NOT secret-bearing
    /// (callers must not put bearer tokens / encryption keys here).
    pub message: String,
}

/// One row of `_wechat_outbox.json`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    /// Monotonic id, `max(id)+1` on append (matches inbox / raw conventions).
    pub id: u32,
    /// Tagged transport identity (kefu / iLink / ...).
    pub transport: OutboxTransport,
    /// Plaintext content to send, post-formatting and post-chunking.
    /// The transport plaintext lives here — never the encrypted form.
    pub content: String,
    /// Lifecycle state.
    pub status: OutboxStatus,
    /// How many times the worker has attempted send. `Pending` rows
    /// with `attempts == 0` are first-ever; with `attempts >= 1` they
    /// are bouncebacks from transient failures.
    pub attempts: u32,
    /// Most recent error. Cleared on `Sent`; preserved on `Failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<OutboxLastError>,
    /// ISO-8601 enqueue time.
    pub created_at: String,
    /// ISO-8601 last status change.
    pub updated_at: String,
    /// ISO-8601 first successful send (set on transition to `Sent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    /// ISO-8601 earliest moment the worker should try again. `None`
    /// means "ready now". Set after a transient failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<String>,
}

fn outbox_path(paths: &WikiPaths) -> PathBuf {
    paths.meta.join(OUTBOX_FILENAME)
}

fn load_outbox_file(paths: &WikiPaths) -> Result<Vec<OutboxEntry>> {
    let path = outbox_path(paths);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let parsed: Vec<OutboxEntry> = serde_json::from_slice(&bytes).map_err(|e| {
        WikiStoreError::Invalid(format!("{OUTBOX_FILENAME} parse error: {e}"))
    })?;
    Ok(parsed)
}

fn save_outbox_file(paths: &WikiPaths, entries: &[OutboxEntry]) -> Result<()> {
    fs::create_dir_all(&paths.meta).map_err(|e| WikiStoreError::io(paths.meta.clone(), e))?;
    let bytes = serde_json::to_vec_pretty(entries).map_err(|e| {
        WikiStoreError::Invalid(format!("{OUTBOX_FILENAME} serialize error: {e}"))
    })?;
    let path = outbox_path(paths);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(())
}

/// Append a new outbox entry in `Pending` state and return the stored
/// record. Assigns the next monotonic id, writes atomically.
///
/// Thread-safe via [`OUTBOX_WRITE_GUARD`].
pub fn append_outbox_entry(
    paths: &WikiPaths,
    transport: OutboxTransport,
    content: &str,
) -> Result<OutboxEntry> {
    let _guard = lock_outbox_writes();
    let mut entries = load_outbox_file(paths)?;
    let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    let now = now_iso8601();
    let entry = OutboxEntry {
        id: next_id,
        transport,
        content: content.to_string(),
        status: OutboxStatus::Pending,
        attempts: 0,
        last_error: None,
        created_at: now.clone(),
        updated_at: now,
        sent_at: None,
        next_retry_at: None,
    };
    entries.push(entry.clone());
    save_outbox_file(paths, &entries)?;
    Ok(entry)
}

/// List **all** outbox entries (any status), in id order. Used by the
/// HTTP read endpoint and by tests; the worker uses
/// [`list_pending_outbox_due`] instead so it doesn't iterate `Sent`.
pub fn list_outbox_entries(paths: &WikiPaths) -> Result<Vec<OutboxEntry>> {
    let _guard = lock_outbox_writes();
    load_outbox_file(paths)
}

/// List entries the worker should attempt this tick: status is
/// `Pending` AND (`next_retry_at` is None or already in the past).
///
/// `now_iso` is the caller's "now" timestamp — passed in (rather than
/// captured here) so tests can fast-forward time deterministically.
pub fn list_pending_outbox_due(paths: &WikiPaths, now_iso: &str) -> Result<Vec<OutboxEntry>> {
    let _guard = lock_outbox_writes();
    let entries = load_outbox_file(paths)?;
    Ok(entries
        .into_iter()
        .filter(|e| e.status == OutboxStatus::Pending)
        .filter(|e| match &e.next_retry_at {
            None => true,
            Some(at) => at.as_str() <= now_iso,
        })
        .collect())
}

/// Claim a `Pending` entry: transition to `Sending` (incrementing
/// `attempts`), and optionally stamp / persist the `msgid` so retries
/// reuse it for WeChat de-dupe.
///
/// Returns `NotFound` when `id` doesn't exist; `Invalid` when the
/// current status is not `Pending` (caller raced with another claim).
pub fn mark_outbox_sending(
    paths: &WikiPaths,
    id: u32,
    msgid: Option<&str>,
) -> Result<OutboxEntry> {
    let _guard = lock_outbox_writes();
    let mut entries = load_outbox_file(paths)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    if entry.status != OutboxStatus::Pending {
        return Err(WikiStoreError::Invalid(format!(
            "outbox#{id} is not Pending (status={:?})",
            entry.status
        )));
    }
    entry.status = OutboxStatus::Sending;
    entry.attempts = entry.attempts.saturating_add(1);
    entry.updated_at = now_iso8601();
    entry.next_retry_at = None;
    if let Some(mid) = msgid {
        // Single-variant enum today (`Kefu`); the `match` keeps it
        // explicit and forward-compatible — adding `Ilink {...}` later
        // forces this site to be revisited.
        match &mut entry.transport {
            OutboxTransport::Kefu {
                msgid: existing, ..
            } => {
                if existing.is_none() {
                    *existing = Some(mid.to_string());
                }
            }
        }
    }
    let updated = entry.clone();
    save_outbox_file(paths, &entries)?;
    Ok(updated)
}

/// Mark an entry as `Sent`. Idempotent on already-`Sent` rows.
/// Returns `NotFound` for missing ids.
pub fn mark_outbox_sent(paths: &WikiPaths, id: u32) -> Result<OutboxEntry> {
    let _guard = lock_outbox_writes();
    let mut entries = load_outbox_file(paths)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    let now = now_iso8601();
    entry.status = OutboxStatus::Sent;
    entry.last_error = None;
    entry.next_retry_at = None;
    entry.updated_at = now.clone();
    if entry.sent_at.is_none() {
        entry.sent_at = Some(now);
    }
    let updated = entry.clone();
    save_outbox_file(paths, &entries)?;
    Ok(updated)
}

/// Mark an in-flight entry as failed. Either bounces back to
/// `Pending` with a backoff (transport / timeout error and
/// `attempts < MAX_OUTBOX_ATTEMPTS`) OR transitions to terminal
/// `Failed` (api / session_expired / cap exceeded).
///
/// Returns the updated entry so the caller can log + decide whether
/// to surface the failure immediately to the UI.
pub fn mark_outbox_failed(
    paths: &WikiPaths,
    id: u32,
    error: OutboxLastError,
) -> Result<OutboxEntry> {
    let _guard = lock_outbox_writes();
    let mut entries = load_outbox_file(paths)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    let terminal = is_terminal_error(&error.kind) || entry.attempts >= MAX_OUTBOX_ATTEMPTS;
    let now = now_iso8601();
    entry.updated_at = now;
    entry.last_error = Some(error);
    if terminal {
        entry.status = OutboxStatus::Failed;
        entry.next_retry_at = None;
    } else {
        entry.status = OutboxStatus::Pending;
        entry.next_retry_at = Some(compute_next_retry_at(entry.attempts));
    }
    let updated = entry.clone();
    save_outbox_file(paths, &entries)?;
    Ok(updated)
}

/// Cancel an entry. Works from any non-terminal status; idempotent
/// on already-`Cancelled`. Refuses to cancel `Sent` (they're done).
pub fn mark_outbox_cancelled(paths: &WikiPaths, id: u32) -> Result<OutboxEntry> {
    let _guard = lock_outbox_writes();
    let mut entries = load_outbox_file(paths)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    if entry.status == OutboxStatus::Sent {
        return Err(WikiStoreError::Invalid(format!(
            "outbox#{id} is already Sent — cancel refused"
        )));
    }
    entry.status = OutboxStatus::Cancelled;
    entry.next_retry_at = None;
    entry.updated_at = now_iso8601();
    let updated = entry.clone();
    save_outbox_file(paths, &entries)?;
    Ok(updated)
}

/// Crash-recovery sweep: revert every `Sending` row to `Pending`
/// (with `next_retry_at = now`, so the worker picks them up
/// immediately on the next tick).
///
/// Call from `DesktopState` bootstrap exactly once before spawning
/// the replay worker. Returns the count of rows reverted.
pub fn reconcile_outbox_on_startup(paths: &WikiPaths) -> Result<usize> {
    let _guard = lock_outbox_writes();
    let mut entries = load_outbox_file(paths)?;
    let mut reverted = 0usize;
    for entry in &mut entries {
        if entry.status == OutboxStatus::Sending {
            entry.status = OutboxStatus::Pending;
            entry.next_retry_at = None;
            entry.updated_at = now_iso8601();
            reverted += 1;
        }
    }
    if reverted > 0 {
        save_outbox_file(paths, &entries)?;
    }
    Ok(reverted)
}

/// Move a `Failed` entry back to `Pending` for one more attempt.
/// Resets `attempts` so the user-initiated retry gets the full
/// backoff budget. Returns `Invalid` when current status is not
/// `Failed` or `Cancelled`.
pub fn retry_outbox_entry(paths: &WikiPaths, id: u32) -> Result<OutboxEntry> {
    let _guard = lock_outbox_writes();
    let mut entries = load_outbox_file(paths)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    if !matches!(entry.status, OutboxStatus::Failed | OutboxStatus::Cancelled) {
        return Err(WikiStoreError::Invalid(format!(
            "outbox#{id} can only retry from Failed or Cancelled (status={:?})",
            entry.status
        )));
    }
    entry.status = OutboxStatus::Pending;
    entry.attempts = 0;
    entry.last_error = None;
    entry.next_retry_at = None;
    entry.updated_at = now_iso8601();
    let updated = entry.clone();
    save_outbox_file(paths, &entries)?;
    Ok(updated)
}

/// Aggregate counts for the UI summary chip.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct OutboxCounts {
    pub pending: usize,
    pub sending: usize,
    pub sent: usize,
    pub failed: usize,
    pub cancelled: usize,
}

/// Summarise the outbox by status. Cheap to compute — single full
/// scan, no grouping by transport (UI consumers can derive that from
/// the per-entry list).
pub fn outbox_counts(paths: &WikiPaths) -> Result<OutboxCounts> {
    let _guard = lock_outbox_writes();
    let entries = load_outbox_file(paths)?;
    let mut counts = OutboxCounts::default();
    for e in entries {
        match e.status {
            OutboxStatus::Pending => counts.pending += 1,
            OutboxStatus::Sending => counts.sending += 1,
            OutboxStatus::Sent => counts.sent += 1,
            OutboxStatus::Failed => counts.failed += 1,
            OutboxStatus::Cancelled => counts.cancelled += 1,
        }
    }
    Ok(counts)
}

fn is_terminal_error(kind: &str) -> bool {
    matches!(kind, "api" | "session_expired")
}

fn compute_next_retry_at(attempts: u32) -> String {
    // attempts is the value AFTER the just-failed send; e.g. failed
    // on first try → attempts == 1. Schedule:
    //   attempts=1 → 30s
    //   attempts=2 → 1m
    //   attempts=3 → 2m
    //   ...
    //   attempts=8 → 1h (capped)
    let exp = attempts.saturating_sub(1).min(20);
    let backoff = BACKOFF_BASE_SECS
        .saturating_mul(1u64.checked_shl(exp).unwrap_or(u64::MAX))
        .min(BACKOFF_CAP_SECS);
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Reuse `wiki_store::format_iso8601` so on-disk `next_retry_at`
    // shape is identical to `created_at` / `updated_at` — the
    // worker compares them as strings and a format mismatch would
    // silently break the "due now" filter.
    format_iso8601(now_unix.saturating_add(backoff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    use crate::init_wiki;

    fn fixture_kefu(uid: &str, kfid: &str) -> OutboxTransport {
        OutboxTransport::Kefu {
            external_userid: uid.to_string(),
            open_kfid: kfid.to_string(),
            msgid: None,
        }
    }

    #[test]
    fn outbox_append_assigns_monotonic_id_starting_at_one() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let a = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "one").unwrap();
        let b = append_outbox_entry(&paths, fixture_kefu("u2", "k1"), "two").unwrap();
        let c = append_outbox_entry(&paths, fixture_kefu("u3", "k1"), "three").unwrap();

        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
        assert_eq!(c.id, 3);
        assert_eq!(a.status, OutboxStatus::Pending);
        assert_eq!(a.attempts, 0);
        assert_eq!(a.content, "one");
    }

    #[test]
    fn outbox_round_trip_serializes_with_transport_tag() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "hi").unwrap();
        let raw = std::fs::read_to_string(outbox_path(&paths)).unwrap();
        // Tagged enum: the JSON must mention the discriminator.
        assert!(raw.contains("\"transport\": \"kefu\""), "missing tag: {raw}");
        assert!(raw.contains("\"external_userid\": \"u1\""));
        assert!(raw.contains("\"status\": \"pending\""));
        // msgid is None on append → skip_serializing_if elides it.
        assert!(!raw.contains("\"msgid\""), "msgid should not be on disk yet");
    }

    #[test]
    fn outbox_load_empty_file_returns_empty_vec() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let entries = list_outbox_entries(&paths).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn outbox_corrupt_file_returns_invalid_error() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        std::fs::write(outbox_path(&paths), b"not json").unwrap();

        let err = list_outbox_entries(&paths).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)), "got {err:?}");
    }

    #[test]
    fn outbox_concurrent_append_is_thread_safe() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = Arc::new(WikiPaths::resolve(tmp.path()));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let p = Arc::clone(&paths);
                thread::spawn(move || {
                    append_outbox_entry(
                        &p,
                        fixture_kefu(&format!("u{i}"), "k1"),
                        &format!("msg-{i}"),
                    )
                    .expect("append should not race")
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let entries = list_outbox_entries(&paths).unwrap();
        assert_eq!(entries.len(), 10);
        let mut ids: Vec<u32> = entries.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=10).collect::<Vec<u32>>(), "duplicate ids: {ids:?}");
    }

    #[test]
    fn outbox_mark_sending_increments_attempts_and_persists_msgid() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let entry = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "hi").unwrap();

        let claimed = mark_outbox_sending(&paths, entry.id, Some("uuid-abc")).unwrap();
        assert_eq!(claimed.status, OutboxStatus::Sending);
        assert_eq!(claimed.attempts, 1);
        let OutboxTransport::Kefu { msgid, .. } = &claimed.transport;
        assert_eq!(msgid.as_deref(), Some("uuid-abc"));

        // Second attempt reuses the same msgid (de-dupe contract).
        let bounced = mark_outbox_failed(
            &paths,
            entry.id,
            OutboxLastError {
                kind: "transport".to_string(),
                message: "connection reset".to_string(),
            },
        )
        .unwrap();
        assert_eq!(bounced.status, OutboxStatus::Pending);

        let claimed2 = mark_outbox_sending(&paths, entry.id, Some("uuid-different")).unwrap();
        let OutboxTransport::Kefu { msgid, .. } = &claimed2.transport;
        assert_eq!(
            msgid.as_deref(),
            Some("uuid-abc"),
            "msgid must be reused across retries"
        );
    }

    #[test]
    fn outbox_mark_sent_sets_status_and_sent_at_once() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let entry = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "hi").unwrap();
        mark_outbox_sending(&paths, entry.id, Some("m1")).unwrap();

        let sent = mark_outbox_sent(&paths, entry.id).unwrap();
        assert_eq!(sent.status, OutboxStatus::Sent);
        assert!(sent.sent_at.is_some());
        let first_sent_at = sent.sent_at.clone();
        assert!(sent.last_error.is_none());

        // Idempotent re-mark keeps the original sent_at.
        let sent_again = mark_outbox_sent(&paths, entry.id).unwrap();
        assert_eq!(sent_again.sent_at, first_sent_at);
    }

    #[test]
    fn outbox_mark_failed_below_cap_returns_pending_with_backoff() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let entry = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "hi").unwrap();
        mark_outbox_sending(&paths, entry.id, None).unwrap();

        let bounced = mark_outbox_failed(
            &paths,
            entry.id,
            OutboxLastError {
                kind: "transport".to_string(),
                message: "boom".to_string(),
            },
        )
        .unwrap();
        assert_eq!(bounced.status, OutboxStatus::Pending);
        assert_eq!(bounced.attempts, 1);
        assert!(bounced.next_retry_at.is_some(), "must schedule retry");
        assert!(
            bounced.last_error.as_ref().is_some_and(|e| e.kind == "transport"),
            "must preserve last_error: {:?}",
            bounced.last_error
        );
    }

    #[test]
    fn outbox_mark_failed_with_api_kind_is_terminal() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let entry = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "hi").unwrap();
        mark_outbox_sending(&paths, entry.id, None).unwrap();

        let dead = mark_outbox_failed(
            &paths,
            entry.id,
            OutboxLastError {
                kind: "api".to_string(),
                message: "errcode=40001".to_string(),
            },
        )
        .unwrap();
        assert_eq!(dead.status, OutboxStatus::Failed);
        assert!(dead.next_retry_at.is_none());
    }

    #[test]
    fn outbox_mark_failed_at_attempt_cap_is_terminal() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let entry = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "hi").unwrap();

        // Force attempts to MAX_OUTBOX_ATTEMPTS by faking the on-disk
        // record (production path would loop through actual sends).
        {
            let _g = lock_outbox_writes();
            let mut all = load_outbox_file(&paths).unwrap();
            all[0].status = OutboxStatus::Sending;
            all[0].attempts = MAX_OUTBOX_ATTEMPTS;
            save_outbox_file(&paths, &all).unwrap();
        }

        let dead = mark_outbox_failed(
            &paths,
            entry.id,
            OutboxLastError {
                kind: "transport".to_string(),
                message: "still flaky".to_string(),
            },
        )
        .unwrap();
        assert_eq!(dead.status, OutboxStatus::Failed);
        assert!(dead.next_retry_at.is_none());
    }

    #[test]
    fn outbox_reconcile_on_startup_reverts_sending_to_pending() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let a = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "a").unwrap();
        let b = append_outbox_entry(&paths, fixture_kefu("u2", "k1"), "b").unwrap();
        mark_outbox_sending(&paths, a.id, None).unwrap();
        mark_outbox_sending(&paths, b.id, None).unwrap();
        // Simulate crash: status stays Sending on disk.

        let reverted = reconcile_outbox_on_startup(&paths).unwrap();
        assert_eq!(reverted, 2);

        let entries = list_outbox_entries(&paths).unwrap();
        for e in entries {
            assert_eq!(e.status, OutboxStatus::Pending, "must be revived: {e:?}");
            assert!(e.next_retry_at.is_none(), "ready immediately: {e:?}");
        }
    }

    #[test]
    fn outbox_list_pending_due_filters_future_retries() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let a = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "ready").unwrap();
        let b = append_outbox_entry(&paths, fixture_kefu("u2", "k1"), "delayed").unwrap();
        // Manually push entry b's next_retry_at into the future.
        {
            let _g = lock_outbox_writes();
            let mut all = load_outbox_file(&paths).unwrap();
            all[1].next_retry_at = Some("2099-01-01T00:00:00Z".to_string());
            save_outbox_file(&paths, &all).unwrap();
        }

        let due = list_pending_outbox_due(&paths, "2026-04-28T00:00:00Z").unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, a.id);
        assert_ne!(due[0].id, b.id);
    }

    #[test]
    fn outbox_mark_cancelled_works_from_any_non_terminal() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let a = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "a").unwrap();
        let cancelled = mark_outbox_cancelled(&paths, a.id).unwrap();
        assert_eq!(cancelled.status, OutboxStatus::Cancelled);

        // Sent → cancel refused.
        let b = append_outbox_entry(&paths, fixture_kefu("u2", "k1"), "b").unwrap();
        mark_outbox_sending(&paths, b.id, None).unwrap();
        mark_outbox_sent(&paths, b.id).unwrap();
        let err = mark_outbox_cancelled(&paths, b.id).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn outbox_retry_resets_attempts_only_from_terminal_states() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let a = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "a").unwrap();
        mark_outbox_sending(&paths, a.id, None).unwrap();
        let dead = mark_outbox_failed(
            &paths,
            a.id,
            OutboxLastError {
                kind: "api".to_string(),
                message: "perm".to_string(),
            },
        )
        .unwrap();
        assert_eq!(dead.status, OutboxStatus::Failed);

        let revived = retry_outbox_entry(&paths, a.id).unwrap();
        assert_eq!(revived.status, OutboxStatus::Pending);
        assert_eq!(revived.attempts, 0);
        assert!(revived.last_error.is_none());

        // From Pending: refused.
        let err = retry_outbox_entry(&paths, a.id).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn outbox_counts_aggregates_by_status() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let a = append_outbox_entry(&paths, fixture_kefu("u1", "k1"), "a").unwrap();
        let b = append_outbox_entry(&paths, fixture_kefu("u2", "k1"), "b").unwrap();
        let c = append_outbox_entry(&paths, fixture_kefu("u3", "k1"), "c").unwrap();
        mark_outbox_sending(&paths, b.id, None).unwrap();
        mark_outbox_sent(&paths, b.id).unwrap();
        mark_outbox_sending(&paths, c.id, None).unwrap();
        mark_outbox_failed(
            &paths,
            c.id,
            OutboxLastError {
                kind: "api".to_string(),
                message: "no".to_string(),
            },
        )
        .unwrap();

        let counts = outbox_counts(&paths).unwrap();
        assert_eq!(counts.pending, 1);
        assert_eq!(counts.sent, 1);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.sending, 0);
        assert_eq!(counts.cancelled, 0);
        let _ = a;
    }

    #[test]
    fn outbox_compute_next_retry_at_caps_at_one_hour() {
        // Schedule shape: 30s, 1m, 2m, 4m, 8m, 16m, 32m, 1h (cap),
        // 1h, 1h. ISO-8601 strings sort lexicographically when same
        // format, so we compare as strings (the format guarantees
        // same width / order). The test pins:
        //   1. attempts=8 produces a future time that is AT LEAST
        //      (cap - 5) seconds out and AT MOST (cap + 5) seconds out.
        //   2. attempts=20 doesn't keep climbing past the cap.
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cap = BACKOFF_CAP_SECS;

        let now_plus_below_cap = format_iso8601(now_secs + cap - 5);
        let now_plus_above_cap = format_iso8601(now_secs + cap + 5);

        let attempt_8 = compute_next_retry_at(8);
        let attempt_20 = compute_next_retry_at(20);

        // attempt 8 must land in [now+cap-5, now+cap+5].
        assert!(
            attempt_8.as_str() >= now_plus_below_cap.as_str(),
            "attempt 8 too small: {attempt_8} vs lower {now_plus_below_cap}"
        );
        assert!(
            attempt_8.as_str() <= now_plus_above_cap.as_str(),
            "attempt 8 exceeded cap: {attempt_8} vs upper {now_plus_above_cap}"
        );

        // attempt 20 must NOT keep climbing — same upper bound.
        assert!(
            attempt_20.as_str() <= now_plus_above_cap.as_str(),
            "attempt 20 exceeded cap: {attempt_20}"
        );
    }
}

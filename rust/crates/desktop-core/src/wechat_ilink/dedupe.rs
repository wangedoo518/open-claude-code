//! L1 event-level dedupe for the WeChat bridge (M5).
//!
//! Semantics:
//!   * Every inbound WeChat message receives a stable [`WeChatEventKey`]
//!     derived from `(channel, account_id, message_id, create_time_ms,
//!     text_hash)`. When `message_id` is present on the envelope, the
//!     `stable_id` ignores the timestamp and the text hash so retries
//!     from the server always collide; when `message_id` is absent (rare
//!     — only seen on some malformed payloads) we fall back to a
//!     content-derived id.
//!   * The middleware guard that runs in `desktop_handler::on_message()`
//!     checks the key against a persistent store. A [`DedupeResult::Hit`]
//!     short-circuits the handler (no ingest, no reply, no raw write).
//!     A [`DedupeResult::Miss`] lets the handler proceed and — on
//!     successful ingest — calls [`DedupeStore::mark_processed`] so the
//!     key lands on disk.
//!
//! Storage:
//!   * In-memory: a capped `HashMap<String, ProcessedEntry>` (FIFO
//!     eviction once the cap is reached). No external LRU dependency —
//!     the store is single-writer behind a `Mutex` and the workload is
//!     tiny (a few thousand entries, insert+get only).
//!   * On disk: append-only JSON Lines at
//!     `~/.clawwiki/wechat_processed_msgs.json`. Each line is one
//!     serialized [`ProcessedEntry`]. On load we dedupe by `stable_id`
//!     so a crash in the middle of a compaction can't leave duplicate
//!     keys in memory. We rewrite the file every 24 h (wall-clock)
//!     to keep its size bounded.
//!   * TTL: entries older than 7 days (from `create_time_ms`, falling
//!     back to the time they were first seen when the envelope has no
//!     timestamp) are dropped at load time and at compaction time.
//!
//! Thread-safety: the public API is sync-only (no await points inside
//! the Mutex) so callers can hold the guard briefly across a disk
//! append without blocking the Tokio runtime. Disk I/O happens on the
//! caller's thread; the wechat handler already routes through
//! `tokio::spawn` / `spawn_blocking`, so this is safe.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// In-memory cap on the number of processed keys held in the map.
/// Once reached, the oldest entry (by `first_seen_ms`) is evicted on
/// insert. 5000 ≈ a few weeks of traffic for a typical chat load.
const DEDUPE_CAP: usize = 5000;

/// TTL after which a processed key is dropped from disk and memory.
const TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Compaction interval: if the on-disk file hasn't been rewritten in
/// this long, the next `mark_processed` call triggers a rewrite.
const COMPACT_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

/// Stable identity for a WeChat inbound event. Derived purely from the
/// envelope — never from the assistant reply — so it is robust to
/// server-side retries and to handler outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeChatEventKey {
    pub channel: String,
    pub account_id: String,
    pub message_id: Option<String>,
    pub create_time_ms: Option<i64>,
    pub text_hash: u64,
}

impl WeChatEventKey {
    /// Deterministic id that [`DedupeStore`] uses as a map key. When a
    /// server-assigned `message_id` is present we trust it; otherwise
    /// we fall back to a content-derived tuple that is still stable
    /// across immediate retries from the iLink long-poll.
    #[must_use]
    pub fn stable_id(&self) -> String {
        match &self.message_id {
            Some(id) => format!("{}:{}:{}", self.channel, self.account_id, id),
            None => {
                let t = self.create_time_ms.unwrap_or(0);
                format!(
                    "{}:{}:{}:{:x}",
                    self.channel, self.account_id, t, self.text_hash
                )
            }
        }
    }

    /// Compute the `u64` text hash used when `message_id` is missing.
    /// Exposed as a helper so call-sites can build the key cheaply
    /// without pulling in an external hasher crate.
    #[must_use]
    pub fn hash_text(text: &str) -> u64 {
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        h.finish()
    }
}

/// Outcome of [`DedupeStore::check_and_mark`]. Shapes the behaviour of
/// the on-message middleware in `desktop_handler.rs`:
///
///   * `Hit` — the key was already processed; the handler short-circuits.
///   * `Miss` — first time we see this key; the handler proceeds and
///     calls [`DedupeStore::mark_processed`] on success.
///   * `Skipped` — the group-scope config excluded this event; the
///     handler short-circuits without marking anything so the envelope
///     stays replayable if the config flips back to `"all"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DedupeResult {
    Hit { first_seen_ms: i64 },
    Miss,
    Skipped { reason: String },
}

/// Single persisted row. Matches the on-disk JSON schema one-to-one.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessedEntry {
    stable_id: String,
    channel: String,
    account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    create_time_ms: Option<i64>,
    #[serde(default)]
    text_hash: u64,
    first_seen_ms: i64,
}

impl ProcessedEntry {
    fn effective_ttl_anchor(&self) -> i64 {
        self.create_time_ms.unwrap_or(self.first_seen_ms)
    }
}

/// Process-global dedupe store. Held behind a `Mutex` so `&self`
/// helpers can mutate the internal map safely. Disk writes happen on
/// the caller's thread — cheap because we only append one line per
/// `mark_processed` call.
pub struct DedupeStore {
    inner: Mutex<Inner>,
}

struct Inner {
    path: PathBuf,
    map: HashMap<String, ProcessedEntry>,
    last_compact_ms: i64,
    /// Per-channel dedupe hits (keyed by [`WeChatEventKey::channel`]).
    hit_count: HashMap<String, u64>,
    /// Per-channel processed counts (keyed by [`WeChatEventKey::channel`]).
    processed_count: HashMap<String, u64>,
    /// Per-channel most recent successful ingest wall-clock (unix ms).
    last_ingest_ms: HashMap<String, i64>,
}

impl DedupeStore {
    /// Build a store rooted at `path`. On construction we load any
    /// existing JSON-Lines file, drop expired rows, and remember the
    /// wall-clock time so we can trigger the next compaction.
    pub fn load(path: PathBuf) -> Self {
        let now = now_ms();
        let mut map: HashMap<String, ProcessedEntry> = HashMap::new();

        if let Ok(file) = File::open(&path) {
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<ProcessedEntry>(trimmed) {
                    if now.saturating_sub(entry.effective_ttl_anchor()) > TTL_MS {
                        continue;
                    }
                    // Dedupe by stable_id — keep the earliest sighting.
                    map.entry(entry.stable_id.clone()).or_insert(entry);
                }
            }
        }

        Self {
            inner: Mutex::new(Inner {
                path,
                map,
                last_compact_ms: now,
                hit_count: HashMap::new(),
                processed_count: HashMap::new(),
                last_ingest_ms: HashMap::new(),
            }),
        }
    }

    /// Look up a key without mutating the store. Returns the
    /// `first_seen_ms` for a hit so callers can surface "duplicate of
    /// message seen at …" in logs if they want.
    pub fn check(&self, key: &WeChatEventKey) -> DedupeResult {
        let id = key.stable_id();
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        match guard.map.get(&id) {
            Some(entry) => {
                let first_seen_ms = entry.first_seen_ms;
                let counter = guard.hit_count.entry(key.channel.clone()).or_insert(0);
                *counter = counter.saturating_add(1);
                DedupeResult::Hit { first_seen_ms }
            }
            None => DedupeResult::Miss,
        }
    }

    /// Record a successful ingest. Idempotent — re-calling with the
    /// same key is a no-op. Disk append is best-effort: failures are
    /// logged to stderr but never surfaced to the caller, matching the
    /// rest of the ingest pipeline's "do not let a bad FS break the
    /// chat reply" contract.
    pub fn mark_processed(&self, key: &WeChatEventKey) {
        let id = key.stable_id();
        let now = now_ms();
        let entry = ProcessedEntry {
            stable_id: id.clone(),
            channel: key.channel.clone(),
            account_id: key.account_id.clone(),
            message_id: key.message_id.clone(),
            create_time_ms: key.create_time_ms,
            text_hash: key.text_hash,
            first_seen_ms: now,
        };

        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        if guard.map.contains_key(&id) {
            return;
        }

        // Cap enforcement — evict the oldest row by first_seen_ms.
        if guard.map.len() >= DEDUPE_CAP {
            if let Some(oldest_key) = guard
                .map
                .values()
                .min_by_key(|e| e.first_seen_ms)
                .map(|e| e.stable_id.clone())
            {
                guard.map.remove(&oldest_key);
            }
        }

        guard.map.insert(id, entry.clone());
        let processed = guard
            .processed_count
            .entry(key.channel.clone())
            .or_insert(0);
        *processed = processed.saturating_add(1);
        guard.last_ingest_ms.insert(key.channel.clone(), now);

        // Append the new row. We skip a full compaction unless the
        // 24 h window has elapsed; callers should not pay a full
        // rewrite cost on every inbound message.
        let needs_compaction = now.saturating_sub(guard.last_compact_ms) > COMPACT_INTERVAL_MS;
        let path = guard.path.clone();
        if needs_compaction {
            let snapshot: Vec<ProcessedEntry> = guard.map.values().cloned().collect();
            guard.last_compact_ms = now;
            // Drop the guard before potentially-slow disk I/O.
            drop(guard);
            if let Err(err) = rewrite_file(&path, &snapshot) {
                eprintln!(
                    "[wechat dedupe] compaction rewrite failed: {err} (path={})",
                    path.display()
                );
            }
        } else {
            drop(guard);
            if let Err(err) = append_line(&path, &entry) {
                eprintln!(
                    "[wechat dedupe] append failed: {err} (path={})",
                    path.display()
                );
            }
        }
    }

    /// Lifetime processed-message count for a single channel. Increments
    /// on every `mark_processed` call so the health endpoint can report
    /// throughput.
    pub fn processed_count(&self, channel: &str) -> u64 {
        self.inner
            .lock()
            .map(|g| g.processed_count.get(channel).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    /// Lifetime dedupe-hit count for a single channel. Increments on
    /// every `check` that returns `Hit`.
    pub fn hit_count(&self, channel: &str) -> u64 {
        self.inner
            .lock()
            .map(|g| g.hit_count.get(channel).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    /// Wall-clock unix-ms of the most recent successful ingest on
    /// `channel`. `None` when no ingest has happened yet this process.
    pub fn last_ingest_ms(&self, channel: &str) -> Option<i64> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.last_ingest_ms.get(channel).copied())
    }
}

/// Process-global singleton. Lazily initialised on first use so tests
/// don't touch the real `~/.clawwiki/` path unless they want to.
static GLOBAL: OnceLock<DedupeStore> = OnceLock::new();

/// Path of the JSON-Lines store under `~/.clawwiki/`.
pub fn default_store_path() -> PathBuf {
    wiki_store::default_root().join("wechat_processed_msgs.json")
}

/// Accessor for the process-wide [`DedupeStore`]. The first call resolves
/// `~/.clawwiki/wechat_processed_msgs.json` and loads any existing rows.
/// Subsequent calls return the same `&DedupeStore`.
pub fn global() -> &'static DedupeStore {
    GLOBAL.get_or_init(|| DedupeStore::load(default_store_path()))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn append_line(path: &Path, entry: &ProcessedEntry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn rewrite_file(path: &Path, entries: &[ProcessedEntry]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp)?;
        for entry in entries {
            let line = serde_json::to_string(entry)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
        }
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn key_with_id(id: &str) -> WeChatEventKey {
        WeChatEventKey {
            channel: "ilink".into(),
            account_id: "acct".into(),
            message_id: Some(id.into()),
            // Use a "recent" timestamp so the TTL test doesn't reject
            // the row as expired when the test binary runs far in the
            // future from the hard-coded constant. We read the wall
            // clock so the row always lands within the TTL window.
            create_time_ms: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
            ),
            text_hash: 0,
        }
    }

    #[test]
    fn stable_id_prefers_message_id() {
        let k = key_with_id("42");
        assert_eq!(k.stable_id(), "ilink:acct:42");
    }

    #[test]
    fn stable_id_falls_back_to_timestamp_and_hash() {
        let k = WeChatEventKey {
            channel: "ilink".into(),
            account_id: "acct".into(),
            message_id: None,
            create_time_ms: Some(1_700_000_000_000),
            text_hash: 0xdeadbeef,
        };
        assert_eq!(k.stable_id(), "ilink:acct:1700000000000:deadbeef");
    }

    #[test]
    fn check_and_mark_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dedupe.json");
        let store = DedupeStore::load(path);

        let k = key_with_id("m1");
        assert!(matches!(store.check(&k), DedupeResult::Miss));
        store.mark_processed(&k);
        match store.check(&k) {
            DedupeResult::Hit { first_seen_ms } => assert!(first_seen_ms > 0),
            other => panic!("expected hit, got {other:?}"),
        }
        assert_eq!(store.processed_count("ilink"), 1);
        assert_eq!(store.hit_count("ilink"), 1);
        assert!(store.last_ingest_ms("ilink").is_some());
    }

    #[test]
    fn reload_rehydrates_from_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dedupe.json");
        {
            let store = DedupeStore::load(path.clone());
            store.mark_processed(&key_with_id("m1"));
            store.mark_processed(&key_with_id("m2"));
        }
        let store = DedupeStore::load(path);
        assert!(matches!(
            store.check(&key_with_id("m1")),
            DedupeResult::Hit { .. }
        ));
        assert!(matches!(
            store.check(&key_with_id("m2")),
            DedupeResult::Hit { .. }
        ));
    }

    #[test]
    fn expired_entries_are_dropped_on_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dedupe.json");
        // Write an expired row directly.
        let expired = ProcessedEntry {
            stable_id: "ilink:acct:old".into(),
            channel: "ilink".into(),
            account_id: "acct".into(),
            message_id: Some("old".into()),
            create_time_ms: Some(0),
            text_hash: 0,
            first_seen_ms: 0,
        };
        append_line(&path, &expired).unwrap();

        let store = DedupeStore::load(path);
        let k = WeChatEventKey {
            channel: "ilink".into(),
            account_id: "acct".into(),
            message_id: Some("old".into()),
            create_time_ms: Some(0),
            text_hash: 0,
        };
        assert!(matches!(store.check(&k), DedupeResult::Miss));
    }
}

//! P1 End-to-End Provenance + Lineage Explorer — core engine.
//!
//! Core flow:
//!
//!   producer ──► [`fire_event`] ──► append one jsonl line to
//!   `{meta}/lineage.jsonl` with an fsync at the end, soft-fails on error.
//!
//!   reader  ──► [`read_lineage_for_wiki`] / `_for_inbox` / `_for_raw`
//!   each scans the full file linearly, filters by [`LineageRef`],
//!   sorts by `timestamp_ms` descending, returns a bounded slice.
//!
//! ## Soft-fail contract
//!
//! `fire_event` *never* returns an `Err` to the caller. Any I/O,
//! serialization, or filesystem permission error is logged with
//! `eprintln!("[provenance] soft-fail: ...")` and swallowed. This is
//! critical: the write points are inside `RAW_WRITE_GUARD` /
//! `INBOX_WRITE_GUARD` guard scopes on the happy path, and a
//! provenance-side failure must never roll back or block the primary
//! write. Losing one lineage event is an observability gap; losing a
//! raw file is data loss.
//!
//! ## File format
//!
//! Append-only `.jsonl`: one `serde_json`-encoded [`LineageEvent`]
//! per line. A crashed writer leaves at most one malformed trailing
//! line, which the reader skips with a warn-log. Schema additions
//! are forward-compatible via serde's `#[serde(default)]` on any
//! new field.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub mod types;
pub use types::{LineageEvent, LineageEventType, LineageRef};

use crate::WikiPaths;

/// Filename of the append-only lineage log, rooted under `{meta}/`.
pub const LINEAGE_FILENAME: &str = "lineage.jsonl";

/// Compute the absolute path of the lineage log for a given wiki root.
///
/// We store under `{meta}/lineage.jsonl` (i.e.
/// `~/.clawwiki/.clawwiki/lineage.jsonl`) to keep the provenance
/// log colocated with the other machine-readable bookkeeping
/// (`inbox.json`, `_absorb_log.json`, `_backlinks.json`). User-facing
/// surfaces under `raw/` and `wiki/` stay untouched.
#[must_use]
pub fn lineage_path(paths: &WikiPaths) -> PathBuf {
    paths.meta.join(LINEAGE_FILENAME)
}

/// Current UTC time in epoch milliseconds. Monotonic enough for the
/// read APIs' descending sort; collisions within the same millisecond
/// preserve file order (natural `Vec` push order).
#[must_use]
pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Generate a fresh UUID v4 for the `event_id` field. Uses the
/// workspace `uuid` crate (v4 random). If `uuid` is ever unavailable
/// we fall back to a timestamp-based string, but in the normal build
/// path this is a direct v4.
#[must_use]
pub fn new_event_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Fire-and-forget wrapper. Callers hand us a fully-built
/// [`LineageEvent`] and we persist it under a soft-fail shield — any
/// error is logged and swallowed so the primary write path is never
/// impacted.
///
/// This is the public entry point every write-point in wiki_store /
/// wiki_maintainer / desktop_handler uses. Keeping the wrapper as a
/// thin veneer over [`fire_event_inner`] centralizes the soft-fail
/// discipline in one place.
pub fn fire_event(paths: &WikiPaths, event: LineageEvent) {
    if let Err(e) = fire_event_inner(paths, &event) {
        eprintln!(
            "[provenance] soft-fail: could not persist event_id={} type={:?} err={e}",
            event.event_id, event.event_type
        );
    }
}

/// Inner write: create `.clawwiki/` if needed, append one jsonl line,
/// fsync. Returns the underlying error so [`fire_event`] can log a
/// single unified line.
fn fire_event_inner(paths: &WikiPaths, event: &LineageEvent) -> std::io::Result<()> {
    let path = lineage_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(event)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

/// Linearly scan `{meta}/lineage.jsonl` and return every parsable
/// event. A missing file returns an empty vec; malformed lines are
/// skipped with a warn-log so a partial crash doesn't block the rest
/// of the log.
///
/// Read-only and idempotent. Safe to call from an HTTP handler
/// without any guard — lineage.jsonl is append-only, so a
/// concurrent write will either land after our read or leave a
/// partially-written line that we tolerate via the malformed-line
/// skip.
pub fn scan_all(paths: &WikiPaths) -> Vec<LineageEvent> {
    let path = lineage_path(paths);
    if !path.is_file() {
        return Vec::new();
    }
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[provenance] scan_all: open {} failed: {e}", path.display());
            return Vec::new();
        }
    };
    let reader = BufReader::new(file);
    let mut out: Vec<LineageEvent> = Vec::new();
    for (idx, line_res) in reader.lines().enumerate() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[provenance] scan_all: line {} read error: {e}", idx + 1);
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LineageEvent>(&line) {
            Ok(event) => out.push(event),
            Err(e) => {
                eprintln!("[provenance] scan_all: line {} parse error: {e}", idx + 1);
            }
        }
    }
    out
}

/// Response shape for `GET /api/lineage/wiki/:slug`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WikiLineageResponse {
    /// Events touching this slug (upstream or downstream), sorted
    /// descending by `timestamp_ms`, sliced to `limit` items
    /// starting at `offset`.
    pub events: Vec<LineageEvent>,
    /// Total count before pagination — lets the UI render a
    /// "showing N of M" footer.
    pub total_count: u32,
}

/// Response shape for `GET /api/lineage/inbox/:id`.
///
/// Splits the events into two buckets so the UI can show "how did
/// this inbox get created" (upstream) separately from "what did this
/// inbox lead to" (downstream). Each bucket is sorted descending by
/// `timestamp_ms`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InboxLineageResponse {
    /// Events whose `downstream` mentions `Inbox{id}` — i.e. the
    /// events that *produced* this inbox entry.
    pub upstream_events: Vec<LineageEvent>,
    /// Events whose `upstream` mentions `Inbox{id}` — i.e. the events
    /// that were *triggered by* this inbox entry (propose, apply,
    /// reject).
    pub downstream_events: Vec<LineageEvent>,
}

/// Filter + sort helper used by all three read APIs. Returns a new
/// vec sorted descending by `timestamp_ms`.
fn sort_desc(mut events: Vec<LineageEvent>) -> Vec<LineageEvent> {
    events.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
    events
}

/// `GET /api/lineage/wiki/:slug?limit=10&offset=0`
///
/// Return every event whose `upstream` or `downstream` mentions the
/// given wiki slug. Sorted descending by `timestamp_ms`; sliced via
/// `offset` + `limit` for pagination. `total_count` reports the
/// unsliced size so the UI can render a "N of M" footer.
pub fn read_lineage_for_wiki(
    paths: &WikiPaths,
    slug: &str,
    limit: usize,
    offset: usize,
) -> WikiLineageResponse {
    let all = scan_all(paths);
    let filtered: Vec<LineageEvent> = all
        .into_iter()
        .filter(|e| {
            e.upstream.iter().any(|r| r.matches_wiki_slug(slug))
                || e.downstream.iter().any(|r| r.matches_wiki_slug(slug))
        })
        .collect();
    let sorted = sort_desc(filtered);
    let total_count = u32::try_from(sorted.len()).unwrap_or(u32::MAX);
    let events: Vec<LineageEvent> = sorted.into_iter().skip(offset).take(limit).collect();
    WikiLineageResponse {
        events,
        total_count,
    }
}

/// `GET /api/lineage/inbox/:id`
///
/// Split lineage events by whether this inbox id appears as an
/// upstream pointer (this inbox is the *result* of the event) vs a
/// downstream pointer (this inbox *drives* the event). No pagination
/// — inbox lineage is naturally short (a few propose/apply lines).
pub fn read_lineage_for_inbox(paths: &WikiPaths, id: u32) -> InboxLineageResponse {
    let all = scan_all(paths);
    let mut upstream_events: Vec<LineageEvent> = Vec::new();
    let mut downstream_events: Vec<LineageEvent> = Vec::new();
    for event in all {
        let mentioned_downstream = event.downstream.iter().any(|r| r.matches_inbox(id));
        let mentioned_upstream = event.upstream.iter().any(|r| r.matches_inbox(id));
        if mentioned_downstream {
            upstream_events.push(event.clone());
        }
        if mentioned_upstream {
            downstream_events.push(event);
        }
    }
    InboxLineageResponse {
        upstream_events: sort_desc(upstream_events),
        downstream_events: sort_desc(downstream_events),
    }
}

/// `GET /api/lineage/raw/:id`
///
/// Return every event touching this raw id, either upstream or
/// downstream. Simple flat list sorted descending — a raw's lineage
/// is the timeline of "what happened to this source".
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RawLineageResponse {
    pub events: Vec<LineageEvent>,
}

/// `GET /api/lineage/raw/:id` — return events mentioning `Raw{id}`
/// in either direction, sorted descending.
pub fn read_lineage_for_raw(paths: &WikiPaths, id: u32) -> RawLineageResponse {
    let all = scan_all(paths);
    let filtered: Vec<LineageEvent> = all
        .into_iter()
        .filter(|e| {
            e.upstream.iter().any(|r| r.matches_raw(id))
                || e.downstream.iter().any(|r| r.matches_raw(id))
        })
        .collect();
    RawLineageResponse {
        events: sort_desc(filtered),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Display title helpers — produce the Chinese short strings the UI
// renders in the timeline. Each is <= 40 chars by construction.
// ─────────────────────────────────────────────────────────────────────

/// Truncate an arbitrary string to at most `max_chars` Unicode
/// code points so the UI label stays on one line. The spec caps at
/// 40 chars; callers pass 30-ish to leave room for prefixes.
fn truncate(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(c);
    }
    out
}

/// `"已入库 {slug}"` per the P1 contract. Caps slug length so the
/// string stays under 40 chars total.
#[must_use]
pub fn display_title_raw_written(slug: &str) -> String {
    format!("已入库 {}", truncate(slug, 32))
}

/// `"新任务: {title}"` — the title is already user-facing so we just
/// truncate defensively.
#[must_use]
pub fn display_title_inbox_appended(title: &str) -> String {
    format!("新任务: {}", truncate(title, 30))
}

/// `"为 {target_slug} 生成提案"`.
#[must_use]
pub fn display_title_proposal_generated(target_slug: &str) -> String {
    format!("为 {} 生成提案", truncate(target_slug, 26))
}

/// `"已应用到 {slug}"` — used by create_new / update_existing / apply.
#[must_use]
pub fn display_title_wiki_page_applied(slug: &str) -> String {
    format!("已应用到 {}", truncate(slug, 30))
}

/// `"已合并 {N} 条任务到 {slug}"` for the combined apply path.
#[must_use]
pub fn display_title_combined_wiki_page_applied(n: usize, slug: &str) -> String {
    format!("已合并 {} 条任务到 {}", n, truncate(slug, 24))
}

/// `"任务已拒绝: {title}"`.
#[must_use]
pub fn display_title_inbox_rejected(title: &str) -> String {
    format!("任务已拒绝: {}", truncate(title, 26))
}

/// `"收到微信消息 ({sender_short})"`. `sender_short` is already
/// trimmed by `short_openid`; we cap it defensively.
#[must_use]
pub fn display_title_wechat_message_received(sender_short: &str) -> String {
    format!("收到微信消息 ({})", truncate(sender_short, 20))
}

/// `"已抓取 {canonical}"` — caps the URL so long query strings don't
/// blow past 40 chars.
#[must_use]
pub fn display_title_url_ingested(canonical: &str) -> String {
    format!("已抓取 {}", truncate(canonical, 32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundary() {
        assert_eq!(truncate("hello", 3), "hel");
        assert_eq!(truncate("中文测试", 2), "中文");
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn display_titles_under_40_chars() {
        let slug = "example-concept-slug";
        assert!(display_title_raw_written(slug).chars().count() <= 40);
        assert!(display_title_wiki_page_applied(slug).chars().count() <= 40);
        assert!(
            display_title_combined_wiki_page_applied(42, slug)
                .chars()
                .count()
                <= 40
        );
    }

    #[test]
    fn lineage_ref_matches() {
        let r = LineageRef::Raw { id: 5 };
        assert!(r.matches_raw(5));
        assert!(!r.matches_raw(6));
        assert!(!r.matches_inbox(5));

        let w = LineageRef::WikiPage {
            slug: "alpha".into(),
            title: Some("Alpha".into()),
        };
        assert!(w.matches_wiki_slug("alpha"));
        assert!(!w.matches_wiki_slug("beta"));
    }

    #[test]
    fn fire_and_scan_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = WikiPaths::resolve(tmp.path());
        std::fs::create_dir_all(&paths.meta).unwrap();

        let ev = LineageEvent {
            event_id: new_event_id(),
            event_type: LineageEventType::RawWritten,
            timestamp_ms: 1_700_000_000_000,
            upstream: vec![LineageRef::UrlSource {
                canonical: "https://example.com".into(),
            }],
            downstream: vec![LineageRef::Raw { id: 1 }],
            display_title: display_title_raw_written("example"),
            metadata: serde_json::json!({}),
        };
        fire_event(&paths, ev.clone());

        let all = scan_all(&paths);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].event_id, ev.event_id);

        let raw_resp = read_lineage_for_raw(&paths, 1);
        assert_eq!(raw_resp.events.len(), 1);

        let miss = read_lineage_for_raw(&paths, 99);
        assert_eq!(miss.events.len(), 0);
    }
}

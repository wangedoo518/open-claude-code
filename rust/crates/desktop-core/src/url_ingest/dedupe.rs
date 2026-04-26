//! Dedupe decision core for the ingest orchestrator.
//!
//! Given a canonical URL + mode (`Default` / `Force`), this module
//! inspects the existing `raw/` + `inbox.json` state and returns a
//! [`DedupeResult`] that tells the orchestrator whether to:
//!
//!   * Fetch and write a fresh raw entry ([`IngestDecision::CreatedNew`]).
//!   * Skip the fetch and reuse an existing raw ([`IngestDecision::ReusedWithPendingInbox`],
//!     [`IngestDecision::ReusedApproved`], [`IngestDecision::ReusedAfterReject`],
//!     [`IngestDecision::ReusedSilent`]).
//!   * Force a fresh write even though an existing raw matches
//!     ([`IngestDecision::ExplicitReingest`]) — used by the UI's
//!     explicit "re-ingest this URL" button and the
//!     `wechat-fetch?force=true` query param.
//!
//! # Decision tree (Explorer B §B-4 + M4 content identity)
//!
//! M4 layers **content identity** (SHA-256 over the cleaned body) as
//! a secondary dedupe signal on top of the M3 canonical-URL primary
//! signal. The caller passes `content_hash` when known (post-fetch);
//! `None` at pre-fetch time lets the URL-only fast path short-circuit
//! without touching the network.
//!
//! ```text
//! if mode == Force:
//!     → ExplicitReingest { previous_raw_id: <existing.id or 0> }
//!     # Force path never consults content_hash — user asked for a
//!     # fresh fetch, and we always run one.
//!
//! else:
//!     let url_match = find_recent_raw_by_source_url(canonical)
//!     match url_match:
//!         Some(raw) =>
//!             # URL hit. Does content agree?
//!             match (content_hash, raw.content_hash):
//!                 (Some(new), Some(old)) if new != old =>
//!                     → RefreshedContent { previous_raw_id, previous_content_hash }
//!                     # Same URL, new body. Orchestrator writes a
//!                     # fresh raw with the updated hash.
//!                 _ =>
//!                     # content matches OR either side is unknown
//!                     # → reuse existing raw, classify by inbox state
//!                     match find_inbox_by_source_raw_id(raw.id).status:
//!                         Some(Pending)  → ReusedWithPendingInbox
//!                         Some(Approved) → ReusedApproved
//!                         Some(Rejected) → ReusedAfterReject
//!                         None           → ReusedSilent
//!         None =>
//!             # URL miss. Content may still match a prior raw from
//!             # a different URL.
//!             match content_hash:
//!                 Some(hash) if find_raw_by_content_hash(hash).is_some() =>
//!                     → ContentDuplicate { matching_raw_id, matching_url }
//!                 _ =>
//!                     → CreatedNew
//! ```
//!
//! # Scope
//!
//! This module is pure decision logic — it never writes. The
//! orchestrator (`url_ingest::ingest_url`) is responsible for
//! translating the [`DedupeResult`] into a fetch+write or a
//! short-circuit return.

use serde::{Deserialize, Serialize};

use wiki_store::{InboxEntry, InboxStatus, RawEntry, WikiPaths};

/// The reason-tagged outcome of a dedupe check. Serializable so the
/// orchestrator can surface it through `IngestOutcome::ReusedExisting`
/// and ultimately to the frontend's inline enrich banner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngestDecision {
    /// First-time ingestion: no raw entry matched the canonical URL.
    CreatedNew,
    /// Raw entry already exists + a pending `NewRaw` inbox entry is
    /// still awaiting user resolution. Reused without re-fetching.
    ReusedWithPendingInbox { reason: String },
    /// Raw entry already exists + the user approved the inbox task.
    /// Reused without re-fetching.
    ReusedApproved { reason: String },
    /// Raw entry already exists + the user rejected the inbox task.
    /// Default mode still reuses; callers who want to retry must pass
    /// `force=true`.
    ReusedAfterReject { reason: String },
    /// Raw entry exists but no inbox entry references it (e.g. task
    /// was purged, or the raw was written via a non-M1 path). Reused
    /// silently — no inbox noise.
    ReusedSilent { reason: String },
    /// `force=true` requested a fresh ingest even though a prior raw
    /// exists. The previous id is carried through so the caller can
    /// surface a diff-style "re-ingested, replacing #NNNNN" label.
    /// `previous_raw_id == 0` means force was requested but no prior
    /// raw existed — functionally equivalent to `CreatedNew`, but
    /// preserves the user's explicit intent in the audit log.
    ExplicitReingest { previous_raw_id: u32 },
    /// **M4**: canonical URL matched an existing raw, but the fetched
    /// body hashes to a different `content_hash` than the prior raw
    /// — the source has been updated in place. The orchestrator
    /// writes a **new** raw entry (new id, new content_hash) so both
    /// landings are retained for diff/audit. The UI can render
    /// "content refreshed from #NNNNN" using the carried `previous_raw_id`.
    ///
    /// This is NOT a reuse — the fetch ran, a new raw was persisted,
    /// and a new inbox NewRaw task is queued. We model it as a
    /// distinct decision kind so observability can differentiate
    /// "same URL refetched because we asked" (Force) from "same URL
    /// refetched because content drifted" (this).
    RefreshedContent {
        previous_raw_id: u32,
        previous_content_hash: String,
    },
    /// **M4**: canonical URL missed the dedupe lookup, but the
    /// computed content hash matched an existing raw from a different
    /// URL — the same article was already archived via an alternate
    /// share link. The orchestrator skips the fresh write and returns
    /// the existing raw, tagging the outcome so the UI can render
    /// "已在素材库 #NNNNN（内容相同，来源 {matching_url}）".
    ///
    /// Like `ReusedWithPendingInbox` / `ReusedSilent`, this is a
    /// reuse — `is_reuse()` returns `true`.
    ContentDuplicate {
        matching_raw_id: u32,
        matching_url: String,
    },
}

impl IngestDecision {
    /// True when this decision represents "skip the fetch, use the
    /// existing raw". `CreatedNew` / `ExplicitReingest` /
    /// `RefreshedContent` all require the orchestrator to run the
    /// fetch+write pipeline. `ContentDuplicate` is a reuse — the
    /// matching raw already captures the same body under a different
    /// URL.
    #[must_use]
    pub fn is_reuse(&self) -> bool {
        matches!(
            self,
            IngestDecision::ReusedWithPendingInbox { .. }
                | IngestDecision::ReusedApproved { .. }
                | IngestDecision::ReusedAfterReject { .. }
                | IngestDecision::ReusedSilent { .. }
                | IngestDecision::ContentDuplicate { .. }
        )
    }

    /// Stable machine tag used by logs and the frontend status chip.
    /// Kept separate from the serde tag so future variants can
    /// introduce internal refinements without breaking the string.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            IngestDecision::CreatedNew => "created_new",
            IngestDecision::ReusedWithPendingInbox { .. } => "reused_with_pending_inbox",
            IngestDecision::ReusedApproved { .. } => "reused_approved",
            IngestDecision::ReusedAfterReject { .. } => "reused_after_reject",
            IngestDecision::ReusedSilent { .. } => "reused_silent",
            IngestDecision::ExplicitReingest { .. } => "explicit_reingest",
            IngestDecision::RefreshedContent { .. } => "refreshed_content",
            IngestDecision::ContentDuplicate { .. } => "content_duplicate",
        }
    }

    /// Human-readable reason string. For `CreatedNew` / `ExplicitReingest`
    /// this is a synthetic label since those variants don't carry a
    /// `reason` field. Useful for logging and the frontend banner.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            IngestDecision::CreatedNew => "created_new".to_string(),
            IngestDecision::ReusedWithPendingInbox { reason }
            | IngestDecision::ReusedApproved { reason }
            | IngestDecision::ReusedAfterReject { reason }
            | IngestDecision::ReusedSilent { reason } => reason.clone(),
            IngestDecision::ExplicitReingest { previous_raw_id } => {
                if *previous_raw_id == 0 {
                    "explicit_reingest_no_prior".to_string()
                } else {
                    format!("explicit_reingest_of_{previous_raw_id:05}")
                }
            }
            IngestDecision::RefreshedContent {
                previous_raw_id, ..
            } => format!("refreshed_content_prev_{previous_raw_id:05}"),
            IngestDecision::ContentDuplicate {
                matching_raw_id, ..
            } => format!("content_duplicate_match_{matching_raw_id:05}"),
        }
    }
}

/// Dedupe mode supplied by the caller. `Force` bypasses the lookup
/// and always returns `ExplicitReingest`, otherwise default behavior
/// applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupeMode {
    /// Normal ingest: reuse on match, create on miss.
    Default,
    /// Explicit re-ingest requested by the user. Always fetches fresh.
    Force,
}

/// Bundle returned by [`decide`]. The orchestrator branches on
/// `decision` and may consume `existing_raw` / `existing_inbox` when
/// short-circuiting.
#[derive(Debug, Clone)]
pub struct DedupeResult {
    /// The classification outcome. See [`IngestDecision`].
    pub decision: IngestDecision,
    /// The matched raw entry, if any. `None` for `CreatedNew` and
    /// `ExplicitReingest { previous_raw_id: 0 }`.
    pub existing_raw: Option<RawEntry>,
    /// The matched inbox entry, if any. May be `Some(..)` even when
    /// `existing_raw` is `None`? No — the inbox lookup goes through
    /// `raw.id`, so `existing_inbox` only populates when
    /// `existing_raw` is `Some`.
    pub existing_inbox: Option<InboxEntry>,
}

/// Run the dedupe decision for `canonical_url` plus optional
/// `content_hash`.
///
/// # M4 two-pass usage
///
/// The orchestrator calls `decide` twice per ingest:
///
///   1. **Pre-fetch** with `content_hash: None`. Reuse-on-URL-match
///      short-circuits before touching the network — same behavior
///      as M3.
///   2. **Post-fetch** with `content_hash: Some(hash)`. URL-only
///      reuse branches are unreachable at this point (the pre-fetch
///      call already took them); the post-fetch call exists to
///      detect content drift (`RefreshedContent`) or cross-URL
///      content dedupe (`ContentDuplicate`).
///
/// Passing `Force` mode always returns `ExplicitReingest`
/// immediately — content_hash is ignored.
///
/// Errors propagate through a `String` rather than a `WikiStoreError`
/// so the orchestrator doesn't drag the error type across its public
/// API. Filesystem errors here are extremely rare (same I/O surface
/// as `list_raw_entries`, which the orchestrator already exercises).
pub fn decide(
    paths: &WikiPaths,
    canonical_url: &str,
    content_hash: Option<&str>,
    mode: DedupeMode,
) -> Result<DedupeResult, String> {
    match mode {
        DedupeMode::Force => {
            // Even on Force, look up the existing raw so we can carry
            // its id into `ExplicitReingest { previous_raw_id }`. The
            // UI relies on this to render "replacing #00042".
            let existing_raw = wiki_store::find_recent_raw_by_source_url(paths, canonical_url)
                .map_err(|e| format!("find_recent_raw_by_source_url failed: {e}"))?;
            let existing_inbox = match existing_raw.as_ref() {
                Some(raw) => wiki_store::find_inbox_by_source_raw_id(paths, raw.id)
                    .map_err(|e| format!("find_inbox_by_source_raw_id failed: {e}"))?,
                None => None,
            };
            let previous_raw_id = existing_raw.as_ref().map_or(0, |r| r.id);
            Ok(DedupeResult {
                decision: IngestDecision::ExplicitReingest { previous_raw_id },
                existing_raw,
                existing_inbox,
            })
        }
        DedupeMode::Default => {
            let url_match = wiki_store::find_recent_raw_by_source_url(paths, canonical_url)
                .map_err(|e| format!("find_recent_raw_by_source_url failed: {e}"))?;

            if let Some(raw) = url_match {
                // URL matched. Does the content agree?
                //
                // `RefreshedContent` only fires when BOTH sides have a
                // known hash and they differ. If either side is `None`
                // (pre-M4 raw, or pre-fetch decide call), we fall back
                // to the classic "reuse by URL" classification — there
                // is nothing to compare against.
                if let (Some(new_hash), Some(old_hash)) =
                    (content_hash, raw.content_hash.as_deref())
                {
                    if new_hash != old_hash {
                        // Still load inbox for symmetry with reuse
                        // branches — the caller may want to cascade
                        // the refresh into a fresh NewRaw task.
                        let existing_inbox = wiki_store::find_inbox_by_source_raw_id(paths, raw.id)
                            .map_err(|e| format!("find_inbox_by_source_raw_id failed: {e}"))?;
                        return Ok(DedupeResult {
                            decision: IngestDecision::RefreshedContent {
                                previous_raw_id: raw.id,
                                previous_content_hash: old_hash.to_string(),
                            },
                            existing_raw: Some(raw),
                            existing_inbox,
                        });
                    }
                }

                // Content agrees (or unknown on either side): classify
                // by inbox state exactly as M3 did.
                let existing_inbox = wiki_store::find_inbox_by_source_raw_id(paths, raw.id)
                    .map_err(|e| format!("find_inbox_by_source_raw_id failed: {e}"))?;

                let decision = match existing_inbox.as_ref().map(|i| i.status) {
                    Some(InboxStatus::Pending) => IngestDecision::ReusedWithPendingInbox {
                        reason: format!("pending_inbox_on_raw_{:05}", raw.id),
                    },
                    Some(InboxStatus::Approved) => IngestDecision::ReusedApproved {
                        reason: format!("approved_inbox_on_raw_{:05}", raw.id),
                    },
                    Some(InboxStatus::Rejected) => IngestDecision::ReusedAfterReject {
                        reason: format!("rejected_inbox_on_raw_{:05}", raw.id),
                    },
                    None => IngestDecision::ReusedSilent {
                        reason: format!("no_inbox_for_raw_{:05}", raw.id),
                    },
                };

                return Ok(DedupeResult {
                    decision,
                    existing_raw: Some(raw),
                    existing_inbox,
                });
            }

            // URL miss. If the caller supplied a content hash, try the
            // secondary identity lookup.
            if let Some(hash) = content_hash {
                if let Some(match_raw) = wiki_store::find_raw_by_content_hash(paths, hash)
                    .map_err(|e| format!("find_raw_by_content_hash failed: {e}"))?
                {
                    let matching_url = match_raw.source_url.clone().unwrap_or_default();
                    let existing_inbox =
                        wiki_store::find_inbox_by_source_raw_id(paths, match_raw.id)
                            .map_err(|e| format!("find_inbox_by_source_raw_id failed: {e}"))?;
                    return Ok(DedupeResult {
                        decision: IngestDecision::ContentDuplicate {
                            matching_raw_id: match_raw.id,
                            matching_url,
                        },
                        existing_raw: Some(match_raw),
                        existing_inbox,
                    });
                }
            }

            Ok(DedupeResult {
                decision: IngestDecision::CreatedNew,
                existing_raw: None,
                existing_inbox: None,
            })
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use wiki_store::{append_inbox_pending, InboxKind, RawFrontmatter};

    /// Materialize a wiki root in a tempdir + return the resolved
    /// `WikiPaths`. Mirrors the one-shot test harness used in
    /// `wiki_store::tests`.
    fn fresh_paths() -> (TempDir, WikiPaths) {
        let temp = TempDir::new().expect("tempdir");
        let root: PathBuf = temp.path().to_path_buf();
        wiki_store::init_wiki(&root).expect("init wiki");
        let paths = WikiPaths::resolve(&root);
        (temp, paths)
    }

    /// Write a raw entry with the given canonical `source_url`.
    /// Helper for setting up dedupe scenarios.
    fn seed_raw(paths: &WikiPaths, slug: &str, source_url: &str) -> RawEntry {
        let fm = RawFrontmatter::for_paste("paste", Some(source_url.to_string()));
        wiki_store::write_raw_entry(paths, "paste", slug, "body content for dedupe test", &fm)
            .expect("write raw")
    }

    /// M4 helper: seed a raw entry carrying a canonical URL + content
    /// hash so the dedupe decision tree can branch on content
    /// identity. Body is long enough to satisfy `validate_raw_content`
    /// for non-exempt sources.
    fn seed_raw_with_hash(
        paths: &WikiPaths,
        slug: &str,
        source_url: &str,
        content_hash: &str,
    ) -> RawEntry {
        let fm = RawFrontmatter::for_paste_with_identity(
            "paste",
            Some(source_url.to_string()),
            None,
            Some(content_hash.to_string()),
        );
        wiki_store::write_raw_entry(
            paths,
            "paste",
            slug,
            "body content for dedupe test with enough characters to be substantive",
            &fm,
        )
        .expect("write raw")
    }

    #[test]
    fn force_with_no_prior_returns_explicit_reingest_id_zero() {
        let (_tmp, paths) = fresh_paths();
        let result = decide(&paths, "https://example.com/new", None, DedupeMode::Force).unwrap();
        assert!(matches!(
            result.decision,
            IngestDecision::ExplicitReingest { previous_raw_id: 0 }
        ));
        assert!(result.existing_raw.is_none());
    }

    #[test]
    fn force_with_prior_carries_previous_id() {
        let (_tmp, paths) = fresh_paths();
        let raw = seed_raw(&paths, "forced", "https://example.com/force");
        let result = decide(&paths, "https://example.com/force", None, DedupeMode::Force).unwrap();
        match result.decision {
            IngestDecision::ExplicitReingest { previous_raw_id } => {
                assert_eq!(previous_raw_id, raw.id);
            }
            other => panic!("expected ExplicitReingest, got {other:?}"),
        }
        assert!(result.existing_raw.is_some());
    }

    #[test]
    fn default_with_no_prior_returns_created_new() {
        let (_tmp, paths) = fresh_paths();
        let result = decide(
            &paths,
            "https://example.com/fresh",
            None,
            DedupeMode::Default,
        )
        .unwrap();
        assert!(matches!(result.decision, IngestDecision::CreatedNew));
        assert!(result.existing_raw.is_none());
        assert!(result.existing_inbox.is_none());
    }

    #[test]
    fn default_with_raw_and_no_inbox_returns_reused_silent() {
        let (_tmp, paths) = fresh_paths();
        let _raw = seed_raw(&paths, "silent", "https://example.com/silent");
        let result = decide(
            &paths,
            "https://example.com/silent",
            None,
            DedupeMode::Default,
        )
        .unwrap();
        assert!(
            matches!(result.decision, IngestDecision::ReusedSilent { .. }),
            "got {:?}",
            result.decision
        );
        assert!(result.existing_raw.is_some());
        assert!(result.existing_inbox.is_none());
    }

    #[test]
    fn default_with_pending_inbox_returns_reused_with_pending() {
        let (_tmp, paths) = fresh_paths();
        let raw = seed_raw(&paths, "pending", "https://example.com/pending");
        let _ = wiki_store::append_new_raw_task(&paths, &raw, "test-origin").unwrap();
        let result = decide(
            &paths,
            "https://example.com/pending",
            None,
            DedupeMode::Default,
        )
        .unwrap();
        assert!(
            matches!(
                result.decision,
                IngestDecision::ReusedWithPendingInbox { .. }
            ),
            "got {:?}",
            result.decision
        );
        assert!(result.existing_inbox.is_some());
        assert_eq!(
            result.existing_inbox.as_ref().unwrap().status,
            InboxStatus::Pending
        );
    }

    #[test]
    fn default_with_approved_inbox_returns_reused_approved() {
        let (_tmp, paths) = fresh_paths();
        let raw = seed_raw(&paths, "approved", "https://example.com/approved");
        let inbox = wiki_store::append_new_raw_task(&paths, &raw, "test-origin").unwrap();
        // Resolve the inbox as Approved via the wire-facing action
        // string (`"approve"`) that `InboxStatus::from_action` accepts.
        let _ = wiki_store::resolve_inbox_entry(&paths, inbox.id, "approve").unwrap();

        let result = decide(
            &paths,
            "https://example.com/approved",
            None,
            DedupeMode::Default,
        )
        .unwrap();
        assert!(
            matches!(result.decision, IngestDecision::ReusedApproved { .. }),
            "got {:?}",
            result.decision
        );
        assert_eq!(
            result.existing_inbox.as_ref().unwrap().status,
            InboxStatus::Approved
        );
    }

    #[test]
    fn default_with_rejected_inbox_returns_reused_after_reject() {
        let (_tmp, paths) = fresh_paths();
        let raw = seed_raw(&paths, "rejected", "https://example.com/rejected");
        let inbox = wiki_store::append_new_raw_task(&paths, &raw, "test-origin").unwrap();
        let _ = wiki_store::resolve_inbox_entry(&paths, inbox.id, "reject").unwrap();

        let result = decide(
            &paths,
            "https://example.com/rejected",
            None,
            DedupeMode::Default,
        )
        .unwrap();
        assert!(
            matches!(result.decision, IngestDecision::ReusedAfterReject { .. }),
            "got {:?}",
            result.decision
        );
        assert_eq!(
            result.existing_inbox.as_ref().unwrap().status,
            InboxStatus::Rejected
        );
    }

    #[test]
    fn conflict_inbox_is_ignored_only_newraw_counts() {
        // `find_inbox_by_source_raw_id` is neutral to `kind`, but
        // since `append_new_raw_task` is the only path that creates
        // the linkage we exercise, this test pins that a Conflict
        // entry created via a different API still registers as "an
        // inbox exists" for the raw.
        let (_tmp, paths) = fresh_paths();
        let raw = seed_raw(&paths, "conflict", "https://example.com/conflict");
        let _ = append_inbox_pending(
            &paths,
            InboxKind::Conflict,
            "conflict title",
            "conflict desc",
            Some(raw.id),
        )
        .unwrap();
        let result = decide(
            &paths,
            "https://example.com/conflict",
            None,
            DedupeMode::Default,
        )
        .unwrap();
        // We should see ReusedWithPendingInbox since the Conflict
        // is Pending. This is fine — the orchestrator reuse shortcut
        // keys on "some pending task referenced this raw", not the
        // kind.
        assert!(matches!(
            result.decision,
            IngestDecision::ReusedWithPendingInbox { .. }
        ));
    }

    #[test]
    fn is_reuse_correctly_classifies_variants() {
        assert!(!IngestDecision::CreatedNew.is_reuse());
        assert!(!IngestDecision::ExplicitReingest { previous_raw_id: 0 }.is_reuse());
        assert!(!IngestDecision::ExplicitReingest {
            previous_raw_id: 42,
        }
        .is_reuse());
        assert!(IngestDecision::ReusedWithPendingInbox { reason: "x".into() }.is_reuse());
        assert!(IngestDecision::ReusedApproved { reason: "x".into() }.is_reuse());
        assert!(IngestDecision::ReusedAfterReject { reason: "x".into() }.is_reuse());
        assert!(IngestDecision::ReusedSilent { reason: "x".into() }.is_reuse());
        // M4: ContentDuplicate is a reuse; RefreshedContent is not.
        assert!(IngestDecision::ContentDuplicate {
            matching_raw_id: 7,
            matching_url: "https://a".into(),
        }
        .is_reuse());
        assert!(!IngestDecision::RefreshedContent {
            previous_raw_id: 7,
            previous_content_hash: "x".into(),
        }
        .is_reuse());
    }

    #[test]
    fn decision_tag_is_stable() {
        assert_eq!(IngestDecision::CreatedNew.tag(), "created_new");
        assert_eq!(
            IngestDecision::ExplicitReingest { previous_raw_id: 1 }.tag(),
            "explicit_reingest"
        );
        // M4 tags
        assert_eq!(
            IngestDecision::RefreshedContent {
                previous_raw_id: 1,
                previous_content_hash: "x".into(),
            }
            .tag(),
            "refreshed_content"
        );
        assert_eq!(
            IngestDecision::ContentDuplicate {
                matching_raw_id: 1,
                matching_url: "https://x".into(),
            }
            .tag(),
            "content_duplicate"
        );
    }

    #[test]
    fn explicit_reingest_reason_includes_previous_id() {
        let d = IngestDecision::ExplicitReingest {
            previous_raw_id: 42,
        };
        assert_eq!(d.reason(), "explicit_reingest_of_00042");
        let d0 = IngestDecision::ExplicitReingest { previous_raw_id: 0 };
        assert_eq!(d0.reason(), "explicit_reingest_no_prior");
    }

    // ─── M4 content dedupe branches ───────────────────────────────

    #[test]
    fn default_url_match_content_changed_returns_refreshed_content() {
        // Same URL, existing raw has hash "aaa...", new fetch yields
        // a different hash "bbb...". Must flip to RefreshedContent.
        let (_tmp, paths) = fresh_paths();
        let url = "https://example.com/refreshed";
        let old_hash = "a".repeat(64);
        let new_hash = "b".repeat(64);
        let raw = seed_raw_with_hash(&paths, "refreshed", url, &old_hash);

        let result = decide(&paths, url, Some(&new_hash), DedupeMode::Default).unwrap();
        match result.decision {
            IngestDecision::RefreshedContent {
                previous_raw_id,
                previous_content_hash,
            } => {
                assert_eq!(previous_raw_id, raw.id);
                assert_eq!(previous_content_hash, old_hash);
            }
            other => panic!("expected RefreshedContent, got {other:?}"),
        }
        assert!(result.existing_raw.is_some());
    }

    #[test]
    fn default_url_match_content_unchanged_reuses_silently() {
        // Same URL, same hash → regular ReusedSilent (no inbox).
        let (_tmp, paths) = fresh_paths();
        let url = "https://example.com/same";
        let hash = "c".repeat(64);
        let _raw = seed_raw_with_hash(&paths, "same", url, &hash);

        let result = decide(&paths, url, Some(&hash), DedupeMode::Default).unwrap();
        assert!(
            matches!(result.decision, IngestDecision::ReusedSilent { .. }),
            "got {:?}",
            result.decision
        );
    }

    #[test]
    fn default_url_match_content_hash_unknown_reuses_normally() {
        // URL matches; content_hash passed as None (the pre-fetch
        // call). Must fall back to the M3 inbox-classification branch
        // rather than firing RefreshedContent prematurely.
        let (_tmp, paths) = fresh_paths();
        let url = "https://example.com/nohash";
        let _raw = seed_raw_with_hash(&paths, "nohash", url, &"d".repeat(64));

        let result = decide(&paths, url, None, DedupeMode::Default).unwrap();
        assert!(
            matches!(result.decision, IngestDecision::ReusedSilent { .. }),
            "None content_hash must short-circuit to reuse, got {:?}",
            result.decision
        );
    }

    #[test]
    fn default_url_miss_content_hit_returns_content_duplicate() {
        // URL doesn't match any existing raw, but the content hash
        // matches a previously-ingested raw from a different URL.
        let (_tmp, paths) = fresh_paths();
        let hash = "e".repeat(64);
        let prior = seed_raw_with_hash(&paths, "prior", "https://first.example/article", &hash);

        let new_url = "https://second.example/same-article";
        let result = decide(&paths, new_url, Some(&hash), DedupeMode::Default).unwrap();
        match result.decision {
            IngestDecision::ContentDuplicate {
                matching_raw_id,
                matching_url,
            } => {
                assert_eq!(matching_raw_id, prior.id);
                assert_eq!(matching_url, "https://first.example/article");
            }
            other => panic!("expected ContentDuplicate, got {other:?}"),
        }
        assert!(result.existing_raw.is_some());
    }

    #[test]
    fn default_url_miss_content_miss_returns_created_new() {
        // New URL + new hash = fully novel ingest.
        let (_tmp, paths) = fresh_paths();
        let _prior = seed_raw_with_hash(&paths, "prior", "https://a.example/", &"f".repeat(64));

        let result = decide(
            &paths,
            "https://brand-new.example/",
            Some(&"9".repeat(64)),
            DedupeMode::Default,
        )
        .unwrap();
        assert!(matches!(result.decision, IngestDecision::CreatedNew));
    }

    #[test]
    fn force_ignores_content_hash_and_returns_explicit_reingest() {
        // Force must NOT inspect content_hash even when provided.
        // The user explicitly asked for a re-ingest regardless of
        // identity state.
        let (_tmp, paths) = fresh_paths();
        let url = "https://example.com/force-with-hash";
        let hash = "1".repeat(64);
        let raw = seed_raw_with_hash(&paths, "force-hash", url, &hash);

        let result = decide(&paths, url, Some(&hash), DedupeMode::Force).unwrap();
        match result.decision {
            IngestDecision::ExplicitReingest { previous_raw_id } => {
                assert_eq!(previous_raw_id, raw.id);
            }
            other => panic!("expected ExplicitReingest, got {other:?}"),
        }
    }

    #[test]
    fn refreshed_content_reason_and_tag() {
        let d = IngestDecision::RefreshedContent {
            previous_raw_id: 42,
            previous_content_hash: "x".into(),
        };
        assert_eq!(d.tag(), "refreshed_content");
        assert_eq!(d.reason(), "refreshed_content_prev_00042");
    }

    #[test]
    fn content_duplicate_reason_and_tag() {
        let d = IngestDecision::ContentDuplicate {
            matching_raw_id: 17,
            matching_url: "https://a".into(),
        };
        assert_eq!(d.tag(), "content_duplicate");
        assert_eq!(d.reason(), "content_duplicate_match_00017");
    }
}

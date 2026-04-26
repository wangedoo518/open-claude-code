//! URL ingest orchestrator — single entry point for URL → raw + inbox.
//!
//! M2 centralised the "fetch a URL, validate, write to `~/.clawwiki/raw/`,
//! append an inbox NewRaw task" flow that was previously copy-pasted
//! across four call sites:
//!
//!   * `desktop-core::DesktopState::maybe_enrich_url` (simple fetch +
//!     synchronous Playwright + background Playwright — three clones in
//!     one function)
//!   * `desktop-core::wechat_ilink::desktop_handler::ingest_wechat_text_to_wiki`
//!   * `desktop-server::wechat_fetch_handler` (`POST /api/desktop/wechat-fetch`)
//!
//! Every one of those used to hand-roll the same `write_raw_entry` +
//! `append_new_raw_task` + wiki-paths-resolve block. The orchestrator
//! owns that logic now, so call sites drop down to a single
//! `ingest_url(...)` call and branch on [`IngestOutcome`].
//!
//! # M3 extension — canonical URL + dedupe
//!
//! M3 layers two new concerns on top of the M2 foundation:
//!
//!   1. **Canonical URL** (`canonical::canonicalize`) — strip
//!      tracking/share-noise query params, lowercase host/scheme,
//!      drop fragment + default port, sort remaining params. Multiple
//!      share URLs pointing at the same article collapse to one
//!      dedupe key.
//!   2. **Dedupe decision** (`dedupe::decide`) — before any network
//!      call, check if a raw entry already exists for the canonical
//!      URL. Default mode reuses the existing raw (regardless of
//!      inbox status: pending / approved / rejected / silent-no-
//!      inbox), returning [`IngestOutcome::ReusedExisting`] without
//!      re-fetching. `force=true` bypasses the check and runs a
//!      fresh fetch+write, tagging the outcome with
//!      [`dedupe::IngestDecision::ExplicitReingest`] so the UI can
//!      render "replacing #NNNNN".
//!
//! The underlying adapter still fetches the canonical URL (not the
//! raw user input), and `source_url` on disk is canonicalized so
//! future re-submissions of tracking-annotated variants also dedupe.
//!
//! # Scope boundaries
//!
//!   * Adapters (`wiki_ingest::url::fetch_and_body`,
//!     `wiki_ingest::wechat_fetch::fetch_wechat_article`) still own
//!     network + Playwright I/O. The orchestrator only picks which
//!     adapter runs based on `prefer_playwright` + URL host.
//!   * `wiki_store::append_new_raw_task` keeps its own dedupe
//!     semantics (M1): if a pending `NewRaw` for a given `source_raw_id`
//!     exists, it returns the existing entry and the orchestrator
//!     surfaces that as [`IngestOutcome::IngestedInboxSuppressed`] so
//!     the caller can choose what to do (usually: still report success).
//!   * Pure-text (non-URL) ingest is NOT handled here — `wechat_ilink`'s
//!     plain-text branch keeps its own `write_raw_entry` call because
//!     the orchestrator is explicitly "URL" scope.

use std::time::Duration;

use wiki_ingest::{IngestError, IngestResult};
use wiki_store::{InboxEntry, RawEntry, RawFrontmatter, WikiPaths};

pub mod canonical;
pub mod content_hash;
pub mod dedupe;
pub mod recent;

// M3 observability: `recent` owns the in-memory ring buffer of recent
// ingest decisions that `desktop-server`'s
// `GET /api/desktop/url-ingest/recent` exposes. We emit into it via
// [`push_recent_log`] at every orchestrator terminal branch.

pub use dedupe::{DedupeMode, DedupeResult, IngestDecision};

// ─────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────

/// Orchestrator input. Constructed per call — cheap to build, all
/// owned strings so callers don't fight the borrow checker when
/// crossing `async` / `spawn_blocking` boundaries.
#[derive(Debug, Clone)]
pub struct IngestRequest<'a> {
    /// The URL to fetch. Trimmed + canonicalized internally before
    /// any network call. Callers stay responsible for extracting a
    /// URL from a larger message (the orchestrator only accepts a
    /// single-URL input).
    pub url: &'a str,
    /// `origin` string passed straight into
    /// `wiki_store::append_new_raw_task` to disambiguate which trigger
    /// path created the inbox entry. Examples:
    /// `"url-fetch"`, `"playwright-fetch"`, `"wechat-fetch"`,
    /// `"WeChat iLink · <openid>"`.
    pub origin_tag: String,
    /// Adapter selection override:
    /// * `None` — auto: any `weixin.qq.com` host goes to Playwright,
    ///   everything else goes to the generic HTTP fetch.
    /// * `Some(true)` — force Playwright regardless of host.
    /// * `Some(false)` — force generic HTTP fetch regardless of host.
    pub prefer_playwright: Option<bool>,
    /// Timeout applied to whichever adapter is chosen. The underlying
    /// Playwright worker also has its own 90s inner budget — this is
    /// the outer guard.
    pub fetch_timeout: Duration,
    /// When `Some`, and the URL fetch is rejected by quality validation
    /// OR fails outright, fall back to writing a `wechat-text` raw
    /// entry with the provided body + slug. Used by the WeChat iLink
    /// path where "user copied a URL that 404s" still needs *something*
    /// to land in raw/ so the conversation is archived.
    pub allow_text_fallback: Option<TextFallback>,
    /// M3: when `true`, bypass the dedupe lookup and always run a
    /// fresh fetch+write even if an existing raw matches the
    /// canonical URL. Tagged as
    /// [`dedupe::IngestDecision::ExplicitReingest`] in the outcome so
    /// the UI can render a "replacing #NNNNN" label. Default is
    /// `false` (normal dedupe).
    pub force: bool,
}

/// Fallback payload for the WeChat iLink path. Used when the URL
/// fetch fails or gets rejected by validation — we still write the
/// user's original text to `raw/` as a `wechat-text` entry so the
/// conversation doesn't vanish into the void.
#[derive(Debug, Clone)]
pub struct TextFallback {
    /// Used to derive the raw filename slug. E.g. `"WeChat · o9cq12345"`.
    pub slug_seed: String,
    /// The body to write verbatim (usually the user's raw text including
    /// the offending URL).
    pub fallback_body: String,
}

/// Orchestrator result. Surfaces enough detail that the four call
/// sites can keep their existing response shapes without re-doing the
/// `write_raw_entry` ceremony.
#[derive(Debug)]
pub enum IngestOutcome {
    /// Happy path: URL fetched, validated, written to raw/, and a
    /// fresh inbox NewRaw task was queued. `decision` distinguishes
    /// a first-time ingestion ([`IngestDecision::CreatedNew`]) from
    /// a user-forced re-ingest ([`IngestDecision::ExplicitReingest`]).
    Ingested {
        entry: RawEntry,
        inbox: InboxEntry,
        title: String,
        body: String,
        decision: IngestDecision,
    },
    /// Raw was written (or matched an existing entry) but the inbox
    /// append hit M1's dedupe path — i.e. an older pending `NewRaw`
    /// for the same `source_raw_id` was returned verbatim. Callers
    /// should treat this as success; no new UI surface is needed.
    IngestedInboxSuppressed {
        entry: RawEntry,
        existing_inbox: InboxEntry,
    },
    /// M3: canonical-URL dedupe hit in `Default` mode — an existing
    /// raw already matches the canonical URL, so the orchestrator
    /// skipped the network fetch and returned the existing entry.
    /// `decision` carries the inbox state classification
    /// (pending / approved / rejected / silent).
    ReusedExisting {
        entry: RawEntry,
        existing_inbox: Option<InboxEntry>,
        decision: IngestDecision,
    },
    /// URL fetch failed or was rejected; the caller provided a text
    /// fallback and the orchestrator wrote that instead.
    FallbackToText {
        entry: RawEntry,
        inbox: InboxEntry,
        reason: String,
    },
    /// URL was fetched successfully but the body failed
    /// `wiki_ingest::validate_fetched_content` (anti-bot page, empty,
    /// image-only, etc.). No raw was written.
    RejectedQuality { reason: String },
    /// Adapter returned a transport error (timeout, non-200, parse
    /// failure from the Playwright worker). No raw was written.
    FetchFailed { error: IngestError },
    /// Playwright specifically failed because a host-level dependency
    /// is missing (e.g. `pip install playwright` never ran). The
    /// caller can render a dedicated install prompt instead of the
    /// generic error banner.
    PrerequisiteMissing { dep: String, hint: String },
    /// URL failed structural validation (empty, whitespace-only, etc.).
    /// No network call was made.
    InvalidUrl { reason: String },
}

impl IngestOutcome {
    /// True if the outcome resulted in at least one raw entry being
    /// written to disk OR reused from disk. Used by logs / side-
    /// channel enrich-status so the UI can distinguish "nothing
    /// happened" from "persisted".
    #[must_use]
    pub fn is_persisted(&self) -> bool {
        matches!(
            self,
            IngestOutcome::Ingested { .. }
                | IngestOutcome::IngestedInboxSuppressed { .. }
                | IngestOutcome::ReusedExisting { .. }
                | IngestOutcome::FallbackToText { .. }
        )
    }

    /// One-line human-readable summary for `eprintln!` logging.
    /// Does NOT include the raw body — just the status shape.
    #[must_use]
    pub fn as_display(&self) -> String {
        match self {
            IngestOutcome::Ingested {
                entry,
                title,
                decision,
                ..
            } => {
                format!(
                    "ingested raw #{:05} ({}) title={:?} [{}]",
                    entry.id,
                    entry.source,
                    title,
                    decision.tag()
                )
            }
            IngestOutcome::IngestedInboxSuppressed {
                entry,
                existing_inbox,
            } => {
                format!(
                    "ingested raw #{:05} ({}), inbox dedupe hit existing #{}",
                    entry.id, entry.source, existing_inbox.id
                )
            }
            IngestOutcome::ReusedExisting {
                entry, decision, ..
            } => {
                format!(
                    "reused existing raw #{:05} ({}) [{}]",
                    entry.id,
                    entry.source,
                    decision.tag()
                )
            }
            IngestOutcome::FallbackToText { entry, reason, .. } => {
                format!(
                    "fallback to wechat-text raw #{:05} (reason: {})",
                    entry.id, reason
                )
            }
            IngestOutcome::RejectedQuality { reason } => {
                format!("rejected by quality check: {reason}")
            }
            IngestOutcome::FetchFailed { error } => {
                format!("fetch failed: {error}")
            }
            IngestOutcome::PrerequisiteMissing { dep, .. } => {
                format!("prerequisite missing: {dep}")
            }
            IngestOutcome::InvalidUrl { reason } => {
                format!("invalid url: {reason}")
            }
        }
    }
}

/// Single entry point used by every URL-ingest caller.
///
/// Flow:
///   1. Validate the URL (non-empty, has an http/https scheme).
///   2. **M3**: Canonicalize the URL (strip tracking params, sort
///      params, lowercase host). All downstream ops use the
///      canonical form — the fetch, the dedupe key, and the
///      recorded `source_url` on disk.
///   3. **M3**: Run the dedupe decision against existing raw +
///      inbox state. In `Default` mode, a match short-circuits
///      to [`IngestOutcome::ReusedExisting`] without touching the
///      network. `Force` mode carries the previous raw id
///      through but still runs a fresh fetch+write.
///   4. Pick the adapter (`prefer_playwright` override → generic vs
///      `wechat_fetch`; auto defaults to Playwright for
///      `weixin.qq.com`).
///   5. Run the adapter with `fetch_timeout`. On Playwright error
///      strings that clearly mean "dependency missing", return
///      `PrerequisiteMissing` early so the UI can prompt an install.
///   6. Validate content quality via
///      `wiki_ingest::validate_fetched_content`. On failure, if
///      `allow_text_fallback` is set, fall back to writing a
///      `wechat-text` raw with the provided body; otherwise return
///      `RejectedQuality`.
///   7. Write the raw entry + append an inbox NewRaw task. The
///      `source_url` field on disk is the canonicalized URL so
///      future tracking-annotated re-submissions still dedupe.
///   8. Emit into the M3 recent-log (observability hook).
///
/// All filesystem + HTTP errors are caught and returned as
/// `IngestOutcome` variants — the function itself does not return
/// `Result`.
pub async fn ingest_url(req: IngestRequest<'_>) -> IngestOutcome {
    let trimmed_url = req.url.trim();
    if trimmed_url.is_empty() {
        let outcome = IngestOutcome::InvalidUrl {
            reason: "url must not be empty".into(),
        };
        push_recent_log(trimmed_url, trimmed_url, &req.origin_tag, &outcome, None);
        return outcome;
    }
    if !(trimmed_url.starts_with("http://") || trimmed_url.starts_with("https://")) {
        let outcome = IngestOutcome::InvalidUrl {
            reason: format!("url must start with http(s)://: {trimmed_url}"),
        };
        push_recent_log(trimmed_url, trimmed_url, &req.origin_tag, &outcome, None);
        return outcome;
    }

    // ── M3: canonicalize ──────────────────────────────────────────
    // Run this before everything else so the fetch, the dedupe key,
    // and the recorded `source_url` on disk all agree on one
    // normalized form.
    let canonical = canonical::canonicalize(trimmed_url);
    if canonical.is_empty() {
        let outcome = IngestOutcome::InvalidUrl {
            reason: format!("canonicalization produced empty url from: {trimmed_url}"),
        };
        push_recent_log(trimmed_url, trimmed_url, &req.origin_tag, &outcome, None);
        return outcome;
    }

    // ── M3: dedupe decision (pre-fetch URL-only pass) ─────────────
    // Resolve paths once up front: both the dedupe check and the
    // eventual write need them, and failing-early is cheaper than
    // failing halfway through a fetch.
    let paths = match resolve_wiki_paths() {
        Ok(p) => p,
        Err(reason) => {
            let outcome = IngestOutcome::FetchFailed {
                error: IngestError::Parse(format!("wiki paths resolve failed: {reason}")),
            };
            push_recent_log(trimmed_url, &canonical, &req.origin_tag, &outcome, None);
            return outcome;
        }
    };

    let mode = if req.force {
        DedupeMode::Force
    } else {
        DedupeMode::Default
    };
    // Pre-fetch pass: no content_hash yet. URL-only match short-
    // circuits to ReusedExisting without touching the network.
    let early_decision = match dedupe::decide(&paths, &canonical, None, mode) {
        Ok(r) => r,
        Err(err) => {
            // Dedupe I/O failure should not block ingest; log and
            // fall through to the normal create path. The subsequent
            // `write_raw_entry` will surface any real disk issue.
            eprintln!("[url_ingest] dedupe check failed (continuing): {err}");
            DedupeResult {
                decision: IngestDecision::CreatedNew,
                existing_raw: None,
                existing_inbox: None,
            }
        }
    };

    // Short-circuit only in Default mode when the pre-fetch pass
    // classified the decision as a reuse (inbox variants). Force +
    // CreatedNew fall through to the fetch pipeline. `RefreshedContent`
    // / `ContentDuplicate` can't appear here because content_hash
    // was None, so their preconditions were unreachable.
    if !req.force && early_decision.decision.is_reuse() && early_decision.existing_raw.is_some() {
        let DedupeResult {
            decision,
            existing_raw,
            existing_inbox,
        } = early_decision;
        let raw = existing_raw.expect("checked is_some above");
        let outcome = IngestOutcome::ReusedExisting {
            entry: raw,
            existing_inbox,
            decision,
        };
        push_recent_log(trimmed_url, &canonical, &req.origin_tag, &outcome, None);
        return outcome;
    }

    // `previous_raw_id` is carried into the final Ingested outcome
    // so the UI can render "replacing #NNNNN" on force-reingest.
    let previous_raw_id = match &early_decision.decision {
        IngestDecision::ExplicitReingest { previous_raw_id } => *previous_raw_id,
        _ => 0,
    };

    let use_playwright = match req.prefer_playwright {
        Some(force_pw) => force_pw,
        None => canonical.contains("weixin.qq.com"),
    };

    let fetch_result = run_adapter(&canonical, use_playwright, req.fetch_timeout).await;

    let ingest_result = match fetch_result {
        Ok(r) => r,
        Err(err) => {
            // Detect "Playwright not installed" flavoured errors so the
            // UI can surface a dedicated install CTA. See
            // `detect_playwright_prereq` for the classifier.
            if use_playwright {
                if let Some(prereq) = detect_playwright_prereq(&err) {
                    push_recent_log(trimmed_url, &canonical, &req.origin_tag, &prereq, None);
                    return prereq;
                }
            }
            // URL fetch failed → optional text fallback, else bubble up.
            let outcome =
                handle_fetch_failure(req.allow_text_fallback.as_ref(), &req.origin_tag, err);
            push_recent_log(trimmed_url, &canonical, &req.origin_tag, &outcome, None);
            return outcome;
        }
    };

    // Quality gate — unified with `desktop-core::maybe_enrich_url` and
    // `wechat_fetch_handler` so anti-bot pages never land in raw/.
    if let Err(reason) = wiki_ingest::validate_fetched_content(&ingest_result.body) {
        let outcome = if let Some(fallback) = req.allow_text_fallback.as_ref() {
            write_fallback(fallback, &req.origin_tag, reason)
        } else {
            IngestOutcome::RejectedQuality { reason }
        };
        push_recent_log(trimmed_url, &canonical, &req.origin_tag, &outcome, None);
        return outcome;
    }

    // ── M4: compute content hash + second dedupe pass ────────────
    // Hash the cleaned body (already sanitised by the adapter). We
    // pass this into a second `dedupe::decide` so the decision tree
    // can classify:
    //   * RefreshedContent — same URL, new body
    //   * ContentDuplicate — new URL, body matches an archived raw
    //   * CreatedNew / ExplicitReingest — otherwise
    let body_hash = content_hash::compute_content_hash(&ingest_result.body);

    let final_decision = if req.force {
        // Force path: keep the M3 semantics regardless of content
        // state — user explicitly asked for a fresh re-ingest.
        IngestDecision::ExplicitReingest { previous_raw_id }
    } else {
        match dedupe::decide(&paths, &canonical, body_hash.as_deref(), mode) {
            Ok(r) => r.decision,
            Err(err) => {
                eprintln!("[url_ingest] post-fetch dedupe check failed (continuing): {err}");
                IngestDecision::CreatedNew
            }
        }
    };

    // ContentDuplicate short-circuits writing — the body is already
    // archived on disk under a different URL. Return the matching
    // raw entry so callers can seed the LLM context with its body.
    if let IngestDecision::ContentDuplicate {
        matching_raw_id,
        matching_url,
    } = &final_decision
    {
        match wiki_store::read_raw_entry(&paths, *matching_raw_id) {
            Ok((matching_raw, _body)) => {
                let existing_inbox =
                    wiki_store::find_inbox_by_source_raw_id(&paths, matching_raw.id)
                        .ok()
                        .flatten();
                let outcome = IngestOutcome::ReusedExisting {
                    entry: matching_raw,
                    existing_inbox,
                    decision: IngestDecision::ContentDuplicate {
                        matching_raw_id: *matching_raw_id,
                        matching_url: matching_url.clone(),
                    },
                };
                push_recent_log(
                    trimmed_url,
                    &canonical,
                    &req.origin_tag,
                    &outcome,
                    body_hash.clone(),
                );
                return outcome;
            }
            Err(err) => {
                // Raw disappeared between the index scan and the
                // read — treat as CreatedNew and fall through.
                eprintln!(
                    "[url_ingest] content_duplicate raw #{matching_raw_id} read failed: {err}"
                );
            }
        }
    }

    // CreatedNew / ExplicitReingest / RefreshedContent: write a new
    // raw carrying the content hash + original URL.
    let write_decision = match final_decision {
        IngestDecision::RefreshedContent { .. }
        | IngestDecision::ExplicitReingest { .. }
        | IngestDecision::CreatedNew => final_decision,
        // Unreachable in practice — URL-only reuse variants were
        // short-circuited above, and ContentDuplicate was handled in
        // the match arm right before this. Fall through defensively.
        other => other,
    };

    let result_with_canonical = IngestResult {
        source_url: Some(canonical.clone()),
        ..ingest_result
    };
    let outcome = write_and_queue(
        &paths,
        result_with_canonical,
        &req.origin_tag,
        trimmed_url,
        &canonical,
        body_hash.clone(),
        write_decision,
    );
    push_recent_log(
        trimmed_url,
        &canonical,
        &req.origin_tag,
        &outcome,
        body_hash,
    );
    outcome
}

// ─────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────

/// Shared `WikiPaths` resolver. Replaces the four inlined closures
/// that used to live at each call site (each of which called
/// `default_root` + `init_wiki` + `resolve` in the same order).
///
/// Returns a plain `String` error so the orchestrator doesn't drag
/// `wiki_store::WikiStoreError` into every caller.
fn resolve_wiki_paths() -> Result<WikiPaths, String> {
    let root = wiki_store::default_root();
    wiki_store::init_wiki(&root).map_err(|e| format!("{e}"))?;
    Ok(WikiPaths::resolve(&root))
}

async fn run_adapter(
    url: &str,
    use_playwright: bool,
    fetch_timeout: Duration,
) -> Result<IngestResult, IngestError> {
    let fetched = if use_playwright {
        tokio::time::timeout(
            fetch_timeout,
            wiki_ingest::wechat_fetch::fetch_wechat_article(url),
        )
        .await
    } else {
        tokio::time::timeout(fetch_timeout, wiki_ingest::url::fetch_and_body(url)).await
    };
    match fetched {
        Ok(inner) => inner,
        Err(_elapsed) => Err(IngestError::Parse(format!(
            "fetch timed out after {:?}",
            fetch_timeout
        ))),
    }
}

/// Delegate to the shared `prerequisites::MissingPrerequisite::detect`
/// classifier so every ingest path surfaces the same Chinese guidance
/// (Playwright / Python / Chromium / Node / etc.). The classifier lives
/// in `desktop-core::prerequisites` and is also consumed by the
/// desktop-server env-check endpoints and the frontend format-error
/// layer, keeping the dep names / hint text identical across Ask,
/// iLink, and the WeChat Bridge UI.
fn detect_playwright_prereq(err: &IngestError) -> Option<IngestOutcome> {
    let s = err.to_string();
    crate::prerequisites::MissingPrerequisite::detect(&s).map(|p| {
        IngestOutcome::PrerequisiteMissing {
            dep: p.as_str().to_string(),
            hint: p.human_hint().to_string(),
        }
    })
}

fn handle_fetch_failure(
    fallback: Option<&TextFallback>,
    origin_tag: &str,
    err: IngestError,
) -> IngestOutcome {
    if let Some(fb) = fallback {
        let reason = format!("fetch failed: {err}");
        return write_fallback(fb, origin_tag, reason);
    }
    IngestOutcome::FetchFailed { error: err }
}

/// Write the happy-path raw + inbox entry. Splits out so
/// `ingest_url` reads top-to-bottom without nested matches.
///
/// M4: the caller threads the original (pre-canonical) URL and the
/// computed content hash through so the frontmatter carries the
/// full identity signal. `RawFrontmatter::for_paste_with_identity`
/// elides `original_url` when it matches the canonical form.
#[allow(clippy::too_many_arguments)]
fn write_and_queue(
    paths: &WikiPaths,
    result: IngestResult,
    origin_tag: &str,
    original_url: &str,
    canonical_url: &str,
    content_hash: Option<String>,
    decision: IngestDecision,
) -> IngestOutcome {
    let frontmatter = RawFrontmatter::for_paste_with_identity(
        &result.source,
        Some(canonical_url.to_string()),
        Some(original_url.to_string()),
        content_hash,
    );
    let entry = match wiki_store::write_raw_entry(
        paths,
        &result.source,
        &result.title,
        &result.body,
        &frontmatter,
    ) {
        Ok(e) => e,
        Err(err) => {
            return IngestOutcome::FetchFailed {
                error: IngestError::Parse(format!("write_raw_entry failed: {err}")),
            };
        }
    };

    let inbox = match wiki_store::append_new_raw_task(paths, &entry, origin_tag) {
        Ok(e) => e,
        Err(err) => {
            // Raw *was* written, but the inbox append failed. Treat
            // as FetchFailed so the caller knows something is off —
            // though in practice this only happens on disk IO errors
            // that would have failed the raw write first.
            return IngestOutcome::FetchFailed {
                error: IngestError::Parse(format!("append_new_raw_task failed: {err}")),
            };
        }
    };

    let _ = entry.filename;
    IngestOutcome::Ingested {
        title: result.title,
        body: result.body,
        entry,
        inbox,
        decision,
    }
}

fn write_fallback(fallback: &TextFallback, origin_tag: &str, reason: String) -> IngestOutcome {
    let paths = match resolve_wiki_paths() {
        Ok(p) => p,
        Err(err) => {
            return IngestOutcome::FetchFailed {
                error: IngestError::Parse(format!("wiki paths resolve failed: {err}")),
            };
        }
    };

    // Fallback source is always `wechat-text` — this orchestrator
    // path is only reached from the WeChat iLink flow today, and
    // `wechat-text` is exempted from `write_raw_entry`'s secondary
    // validator (`RAW_CONTENT_VALIDATION_EXEMPT` in wiki_store).
    let source = "wechat-text";
    let frontmatter = RawFrontmatter::for_paste(source, None);
    let entry = match wiki_store::write_raw_entry(
        &paths,
        source,
        &fallback.slug_seed,
        &fallback.fallback_body,
        &frontmatter,
    ) {
        Ok(e) => e,
        Err(err) => {
            return IngestOutcome::FetchFailed {
                error: IngestError::Parse(format!("fallback write_raw_entry failed: {err}")),
            };
        }
    };

    let inbox = match wiki_store::append_new_raw_task(&paths, &entry, origin_tag) {
        Ok(e) => e,
        Err(err) => {
            return IngestOutcome::FetchFailed {
                error: IngestError::Parse(format!("fallback append_new_raw_task failed: {err}")),
            };
        }
    };

    IngestOutcome::FallbackToText {
        entry,
        inbox,
        reason,
    }
}

/// Load a previously-ingested raw body so the Ask enrich path can
/// seed the LLM prompt with the existing article text on a dedupe
/// hit. Returns `None` when the raw can't be loaded (deleted,
/// corrupted frontmatter) — callers then fall back to the plain
/// user message.
///
/// Separate from `wiki_store::read_raw_entry` so the enrich path
/// doesn't need to marshal its own `WikiPaths` + error conversion
/// boilerplate, and so the "what counts as the title" logic
/// (first-line heuristic) stays co-located with the other enrich
/// helpers.
#[must_use]
pub fn load_reused_body(raw_id: u32) -> Option<(String, String)> {
    let paths = resolve_wiki_paths().ok()?;
    let (entry, body) = wiki_store::read_raw_entry(&paths, raw_id).ok()?;
    // Derive a title from the body's first non-empty line, falling
    // back to the slug. Mirrors the quick-and-dirty heuristic the
    // inbox path uses so the "Reused" chip carries a meaningful
    // label rather than a raw file slug.
    let title = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| entry.slug.clone());
    Some((title, body))
}

/// M3 observability hook — funnels every orchestrator terminal
/// outcome into the `url_ingest::recent` ring buffer. The buffer
/// feeds `desktop-server`'s
/// `GET /api/desktop/url-ingest/recent` endpoint, which in turn
/// powers the Raw Library's "Recent ingests" diagnostic panel.
///
/// The push is best-effort and synchronous — the mutex inside
/// `recent::log()` is never held across an `await`, so this stays
/// cheap even on the hot path.
fn push_recent_log(
    original_url: &str,
    canonical_url: &str,
    origin_tag: &str,
    outcome: &IngestOutcome,
    content_hash: Option<String>,
) {
    // Derive the stable short outcome-kind string the endpoint serves
    // — keeps the wire format decoupled from the internal enum so a
    // future `IngestOutcome` refactor doesn't break the frontend.
    let outcome_kind = match outcome {
        IngestOutcome::Ingested { .. } => "ingested",
        IngestOutcome::IngestedInboxSuppressed { .. } => "inbox_suppressed",
        IngestOutcome::ReusedExisting { .. } => "reused_existing",
        IngestOutcome::FallbackToText { .. } => "fallback_to_text",
        IngestOutcome::RejectedQuality { .. } => "rejected_quality",
        IngestOutcome::FetchFailed { .. } => "fetch_failed",
        IngestOutcome::PrerequisiteMissing { .. } => "prerequisite_missing",
        IngestOutcome::InvalidUrl { .. } => "invalid_url",
    };

    // Only the three "persisted"-ish variants carry a decision.
    // Serialize via `serde_json::to_value` so `RecentIngestEntry`
    // can keep its generic payload type — Worker B's module stores
    // `Option<serde_json::Value>` and the frontend treats it as
    // opaque JSON keyed by the `kind` tag.
    let decision_json = match outcome {
        IngestOutcome::Ingested { decision, .. }
        | IngestOutcome::ReusedExisting { decision, .. } => serde_json::to_value(decision).ok(),
        _ => None,
    };

    // M4: surface the decision's reason string as its own column so
    // diagnostics can filter without decoding the `decision` JSON.
    let decision_reason = match outcome {
        IngestOutcome::Ingested { decision, .. }
        | IngestOutcome::ReusedExisting { decision, .. } => Some(decision.reason()),
        _ => None,
    };

    // M4: `content_hash_hit` flags decisions that were driven by the
    // content-identity lookup — both the cross-URL dedupe
    // (ContentDuplicate) and the same-URL drift detection
    // (RefreshedContent).
    let content_hash_hit = match outcome {
        IngestOutcome::ReusedExisting {
            decision: IngestDecision::ContentDuplicate { .. },
            ..
        }
        | IngestOutcome::Ingested {
            decision: IngestDecision::RefreshedContent { .. },
            ..
        } => Some(true),
        _ => None,
    };

    let raw_id = match outcome {
        IngestOutcome::Ingested { entry, .. }
        | IngestOutcome::IngestedInboxSuppressed { entry, .. }
        | IngestOutcome::ReusedExisting { entry, .. }
        | IngestOutcome::FallbackToText { entry, .. } => Some(entry.id),
        _ => None,
    };

    let inbox_id = match outcome {
        IngestOutcome::Ingested { inbox, .. } | IngestOutcome::FallbackToText { inbox, .. } => {
            Some(inbox.id)
        }
        IngestOutcome::IngestedInboxSuppressed { existing_inbox, .. } => Some(existing_inbox.id),
        IngestOutcome::ReusedExisting { existing_inbox, .. } => {
            existing_inbox.as_ref().map(|i| i.id)
        }
        _ => None,
    };

    let entry = recent::RecentIngestEntry {
        timestamp_ms: recent::now_millis(),
        canonical_url: canonical_url.to_string(),
        // M4: `original_url` is now the real pre-canonical input,
        // threaded from the orchestrator's caller. Useful for
        // distinguishing "utm_source variant" vs "already canonical".
        original_url: original_url.to_string(),
        entry_point: origin_tag.to_string(),
        outcome_kind: outcome_kind.to_string(),
        decision: decision_json,
        raw_id,
        inbox_id,
        adapter: None,
        duration_ms: None,
        summary: outcome.as_display(),
        decision_reason,
        content_hash,
        content_hash_hit,
    };
    recent::push(entry);
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────
//
// Integration-level tests for the orchestrator (real fetch, real
// Playwright) belong at the crate root's end-to-end layer; here we
// only cover the pure helpers that don't touch the network.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_url_empty() {
        // We call the public entry synchronously via a throwaway
        // runtime to check the fast-path branch that never awaits.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let out = rt.block_on(ingest_url(IngestRequest {
            url: "   ",
            origin_tag: "test".into(),
            prefer_playwright: Some(false),
            fetch_timeout: Duration::from_millis(10),
            allow_text_fallback: None,
            force: false,
        }));
        assert!(matches!(out, IngestOutcome::InvalidUrl { .. }));
    }

    #[test]
    fn invalid_url_no_scheme() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let out = rt.block_on(ingest_url(IngestRequest {
            url: "example.com/foo",
            origin_tag: "test".into(),
            prefer_playwright: Some(false),
            fetch_timeout: Duration::from_millis(10),
            allow_text_fallback: None,
            force: false,
        }));
        assert!(matches!(out, IngestOutcome::InvalidUrl { .. }));
    }

    #[test]
    fn detect_playwright_prereq_matches_module_error() {
        let err = IngestError::Parse(
            "WeChat fetch failed: ModuleNotFoundError: No module named 'playwright'".into(),
        );
        let hit = detect_playwright_prereq(&err);
        assert!(
            matches!(hit, Some(IngestOutcome::PrerequisiteMissing { ref dep, .. }) if dep == "playwright"),
            "expected playwright prereq, got {hit:?}"
        );
    }

    #[test]
    fn detect_playwright_prereq_matches_install_hint() {
        let err = IngestError::Parse(
            "WeChat fetch failed: Playwright not installed. Run: pip install playwright".into(),
        );
        let hit = detect_playwright_prereq(&err);
        assert!(matches!(
            hit,
            Some(IngestOutcome::PrerequisiteMissing { .. })
        ));
    }

    #[test]
    fn detect_playwright_prereq_matches_python_missing() {
        let err = IngestError::Parse("Failed to spawn Python: ...".into());
        let hit = detect_playwright_prereq(&err);
        assert!(
            matches!(hit, Some(IngestOutcome::PrerequisiteMissing { ref dep, .. }) if dep == "python"),
            "expected python prereq, got {hit:?}"
        );
    }

    #[test]
    fn detect_playwright_prereq_none_for_generic_error() {
        let err = IngestError::HttpStatus {
            status: 500,
            url: "https://example.com".into(),
        };
        assert!(detect_playwright_prereq(&err).is_none());
    }

    #[test]
    fn outcome_is_persisted_classifies_correctly() {
        let err = IngestError::Network("boom".into());
        assert!(!IngestOutcome::FetchFailed { error: err }.is_persisted());
        assert!(!IngestOutcome::RejectedQuality { reason: "x".into() }.is_persisted());
        assert!(!IngestOutcome::InvalidUrl { reason: "y".into() }.is_persisted());
    }

    #[test]
    fn outcome_as_display_is_nonempty() {
        let reason = "too short".to_string();
        let out = IngestOutcome::RejectedQuality { reason };
        assert!(!out.as_display().is_empty());
    }

    #[test]
    fn reused_existing_counts_as_persisted() {
        // M3 adds a new persisted variant; pin the classifier so a
        // future enum rename doesn't drop it from the "persisted" set.
        let paths = WikiPaths::resolve(std::path::Path::new("/does/not/matter"));
        let _ = paths;
        // Construct a minimal RawEntry for the test.
        let entry = RawEntry {
            id: 1,
            filename: "00001_paste_x_2026-04-17.md".to_string(),
            source: "paste".to_string(),
            slug: "x".to_string(),
            date: "2026-04-17".to_string(),
            source_url: Some("https://example.com/".into()),
            ingested_at: "2026-04-17T00:00:00Z".to_string(),
            byte_size: 10,
            content_hash: None,
            original_url: None,
        };
        let out = IngestOutcome::ReusedExisting {
            entry,
            existing_inbox: None,
            decision: IngestDecision::ReusedSilent {
                reason: "silent_test".into(),
            },
        };
        assert!(
            out.is_persisted(),
            "ReusedExisting should count as persisted"
        );
        assert!(!out.as_display().is_empty());
    }
}

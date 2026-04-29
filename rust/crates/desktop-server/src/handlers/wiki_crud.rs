use super::super::*;

// ── ClawWiki S1: wiki/raw layer HTTP handlers ─────────────────────
//
// These three handlers wrap `wiki_store::{write_raw_entry,
// list_raw_entries, read_raw_entry}`. They resolve the wiki root via
// `wiki_store::default_root()` (env override `CLAWWIKI_HOME` →
// `$HOME/.clawwiki/`) and call `init_wiki()` once on every request to
// keep them stateless and crash-safe.
//
// S5 will add the WeChat ingest path that flows through `ingest_wiki_raw`
// when a microWeChat message comes in via the wechat_ilink monitor.
// For S1 the only producer is the manual paste form on the Raw Library
// page (frontend `features/ingest/persist.ts`).

#[derive(Debug, Deserialize)]
pub(crate) struct IngestRawRequest {
    /// Source identifier: `paste`, `wechat-text`, etc. Stored in the
    /// frontmatter and used as part of the filename.
    source: String,
    /// Free-form title used to derive the slug. May contain any
    /// characters; `wiki_store::slugify` sanitizes it.
    #[serde(default)]
    title: String,
    /// Markdown body. Written to disk verbatim under the frontmatter.
    #[serde(default)]
    body: String,
    /// Optional source URL. When present, recorded in the frontmatter.
    #[serde(default)]
    source_url: Option<String>,
    /// M4: when `true` and `source == "url"` (fast-path branch), bypass
    /// the orchestrator's canonical-URL dedupe and always run a fresh
    /// fetch+write. Surfaces through the orchestrator's
    /// `IngestDecision::ExplicitReingest` variant so the frontend can
    /// render a "re-ingest of #NNNNN" banner. No effect on the legacy
    /// paste/body branch. Defaults to `false`.
    #[serde(default)]
    force: Option<bool>,
}

pub(crate) fn resolve_wiki_root_for_handler() -> Result<wiki_store::WikiPaths, ApiError> {
    let root = wiki_store::default_root();
    wiki_store::init_wiki(&root).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to init wiki root: {e}"),
            }),
        )
    })?;
    Ok(wiki_store::WikiPaths::resolve(&root))
}

/// `POST /api/wiki/raw`
///
/// Ingest a single raw entry. Body shape:
/// ```json
/// {
///   "source": "paste",
///   "title": "Hello world",
///   "body": "## hi\n",
///   "source_url": "https://example.com/article"   // optional
/// }
/// ```
///
/// Returns the resulting `RawEntry` so the caller can render an
/// optimistic row in the Raw Library list.
pub(crate) async fn ingest_wiki_raw_handler(
    Json(body): Json<IngestRawRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.source.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "source must not be empty".to_string(),
            }),
        ));
    }

    // ── M4 Worker B: URL fast-path routed through unified orchestrator ─
    //
    // When `source == "url"` and no body is supplied, defer to
    // `desktop_core::url_ingest::ingest_url`. This replaces the old
    // one-shot `wiki_ingest::url::fetch_and_body` + manual
    // `write_raw_entry` + `append_new_raw_task` sequence with the
    // orchestrator that every other URL ingest site already uses
    // (Ask enrich, WeChat iLink, wechat-fetch). Benefits:
    //
    //   * Canonical-URL dedupe: repeated paste of the same URL short-
    //     circuits to the existing raw (`ReusedExisting`).
    //   * M4 content-hash dedupe: even on a fresh URL, identical body
    //     hits `ContentDuplicate` (surfaced via `decision.kind`).
    //   * `force=true` supports the Raw Library "re-ingest" button.
    //   * Playwright auto-selection for `weixin.qq.com` hosts.
    //
    // The non-URL / body-supplied branch below preserves the S1
    // semantics verbatim so paste / wechat-text / file ingest keep
    // writing directly via `wiki_store` without a fetch round-trip.
    if body.source == "url" && body.body.is_empty() {
        let url = body
            .source_url
            .clone()
            .unwrap_or_else(|| body.title.clone())
            .trim()
            .to_string();
        if url.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "url source requires either `body`, `source_url`, or a non-empty title"
                        .to_string(),
                }),
            ));
        }

        let outcome =
            desktop_core::url_ingest::ingest_url(desktop_core::url_ingest::IngestRequest {
                url: &url,
                origin_tag: "raw-library-url".to_string(),
                prefer_playwright: None, // orchestrator auto-routes weixin.qq.com to Playwright
                fetch_timeout: std::time::Duration::from_secs(30),
                allow_text_fallback: None,
                force: body.force.unwrap_or(false),
            })
            .await;
        eprintln!("[raw-library-url] outcome: {}", outcome.as_display());

        return match outcome {
            desktop_core::url_ingest::IngestOutcome::Ingested {
                entry,
                inbox,
                decision,
                ..
            } => {
                // Orchestrator wrote raw + inbox; broadcast the WS
                // notification so the Inbox page repaints immediately.
                // `fire_inbox_notify` is a best-effort broadcast — a
                // double-fire would be silently coalesced on the client,
                // so we err on the side of notifying even if a future
                // orchestrator version calls it itself.
                fire_inbox_notify();
                Ok(Json(serde_json::json!({
                    "raw_entry": raw_entry_to_json(&entry),
                    "inbox_entry": inbox,
                    "decision": decision,
                    "content_hash": serde_json::to_value(&entry).ok()
                        .and_then(|v| v.get("content_hash").cloned()),
                })))
            }
            desktop_core::url_ingest::IngestOutcome::ReusedExisting {
                entry,
                existing_inbox,
                decision,
            } => Ok(Json(serde_json::json!({
                "raw_entry": raw_entry_to_json(&entry),
                "inbox_entry": existing_inbox,
                "decision": decision,
                "dedupe": true,
                "content_hash": serde_json::to_value(&entry).ok()
                    .and_then(|v| v.get("content_hash").cloned()),
            }))),
            desktop_core::url_ingest::IngestOutcome::IngestedInboxSuppressed {
                entry,
                existing_inbox,
            } => {
                fire_inbox_notify();
                Ok(Json(serde_json::json!({
                    "raw_entry": raw_entry_to_json(&entry),
                    "inbox_entry": existing_inbox,
                    "decision": { "kind": "inbox_suppressed" },
                    "content_hash": serde_json::to_value(&entry).ok()
                        .and_then(|v| v.get("content_hash").cloned()),
                })))
            }
            desktop_core::url_ingest::IngestOutcome::FallbackToText {
                entry,
                inbox,
                reason,
            } => {
                fire_inbox_notify();
                Ok(Json(serde_json::json!({
                    "raw_entry": raw_entry_to_json(&entry),
                    "inbox_entry": inbox,
                    "decision": { "kind": "fallback_to_text", "reason": reason },
                })))
            }
            desktop_core::url_ingest::IngestOutcome::RejectedQuality { reason } => Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse { error: reason }),
            )),
            desktop_core::url_ingest::IngestOutcome::FetchFailed { error } => Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: error.to_string(),
                }),
            )),
            desktop_core::url_ingest::IngestOutcome::PrerequisiteMissing { dep, hint } => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("缺少依赖 {dep}: {hint}"),
                }),
            )),
            desktop_core::url_ingest::IngestOutcome::InvalidUrl { reason } => Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: reason }),
            )),
        };
    }

    // ── Legacy branch: body-supplied ingest (paste / wechat-text / file) ─
    //
    // Non-url sources still require a body. The url fast-path above is
    // the ONLY case where body may legitimately be empty. This branch
    // preserves the S1 write semantics untouched so paste / CLI tests
    // / integration fixtures continue to work without a network call.
    if body.body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "body must not be empty".to_string(),
            }),
        ));
    }
    let effective_title = body.title.clone();
    let effective_body = body.body.clone();
    let effective_source_url = body.source_url.clone();

    let paths = resolve_wiki_root_for_handler()?;
    let frontmatter = wiki_store::RawFrontmatter::for_paste(&body.source, effective_source_url);
    let entry = wiki_store::write_raw_entry(
        &paths,
        &body.source,
        &effective_title,
        &effective_body,
        &frontmatter,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("write_raw_entry failed: {e}"),
            }),
        )
    })?;

    // S4 side-channel: every successful raw write appends a pending
    // `new-raw` task to the inbox so the maintainer Inbox page has
    // something to display. We swallow errors from the inbox append
    // because inbox bookkeeping should NEVER block a successful
    // ingest — losing one inbox task is recoverable, losing a raw
    // entry is not. Formatting lives inside `append_new_raw_task`
    // (review nit #15) so the wechat path stays in lockstep.
    let origin = format!("source `{}`", body.source);
    if let Err(err) = wiki_store::append_new_raw_task(&paths, &entry, &origin) {
        eprintln!(
            "[warn] raw entry {} written but inbox append failed: {err}",
            entry.id
        );
    } else {
        fire_inbox_notify(); // feat(O): instant WS push
    }

    Ok(Json(raw_entry_to_json(&entry)))
}

/// `POST /api/wiki/fetch` (canonical §9.3 · feat N)
///
/// Preview a URL by running the same `wiki_ingest::url::fetch_and_body`
/// pipeline that `POST /api/wiki/raw` uses, but **without** writing
/// to disk or appending an Inbox task. Returns the extracted title,
/// markdown body, source URL, and source tag in a JSON envelope.
///
/// This is the "preview before commit" surface for a future two-step
/// UI flow: paste URL → click Preview → see extracted markdown →
/// click Commit (which then hits `POST /api/wiki/raw`). MVP frontend
/// doesn't have that two-step flow yet, but the route exists so the
/// UI can be built without server-side changes later.
///
/// Body shape:
/// ```json
/// { "url": "https://mp.weixin.qq.com/s/..." }
/// ```
///
/// Returns:
/// ```json
/// {
///   "title": "...",
///   "body": "# ...\n\n_Source: <...>_\n\n...",
///   "source_url": "...",
///   "source": "url"
/// }
/// ```
///
/// Errors:
/// * 400 — empty/invalid url
/// * 502 — upstream fetch failed (network, non-2xx, oversize)
pub(crate) async fn preview_wiki_fetch_handler(
    Json(body): Json<PreviewFetchRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let url = body.url.trim();
    if url.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "url must not be empty".to_string(),
            }),
        ));
    }

    let result = wiki_ingest::url::fetch_and_body(url).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: format!("url fetch failed: {e}"),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "title": result.title,
        "body": result.body,
        "source_url": result.source_url,
        "source": result.source,
    })))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PreviewFetchRequest {
    url: String,
}

/// `GET /api/wiki/raw`
///
/// List every raw entry, sorted by id ascending. Empty wiki returns
/// `{ entries: [] }` (never errors when the directory is missing).
pub(crate) async fn list_wiki_raw_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let entries = wiki_store::list_raw_entries(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_raw_entries failed: {e}"),
            }),
        )
    })?;
    let json: Vec<serde_json::Value> = entries.iter().map(raw_entry_to_json).collect();
    Ok(Json(serde_json::json!({ "entries": json })))
}

/// `GET /api/wiki/raw/:id`
///
/// Read one raw entry by numeric id. Returns the metadata block plus
/// the body text (`{ entry: ..., body: "..." }`). 404 when the id is
/// not present in the directory.
pub(crate) async fn get_wiki_raw_handler(
    Path(id): Path<u32>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    match wiki_store::read_raw_entry(&paths, id) {
        Ok((entry, body)) => Ok(Json(serde_json::json!({
            "entry": raw_entry_to_json(&entry),
            "body": body,
        }))),
        Err(wiki_store::WikiStoreError::NotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("raw entry not found: {id}"),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("read_raw_entry failed: {e}"),
            }),
        )),
    }
}

// ── ClawWiki S4: inbox HTTP handlers ─────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct ResolveInboxRequest {
    /// Either `"approve"` or `"reject"`. Anything else returns 400.
    action: String,
}

pub(crate) async fn list_wiki_inbox_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let entries = wiki_store::list_inbox_entries(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_inbox_entries failed: {e}"),
            }),
        )
    })?;
    let pending = entries
        .iter()
        .filter(|e| e.status == wiki_store::InboxStatus::Pending)
        .count();
    Ok(Json(serde_json::json!({
        "entries": entries,
        "pending_count": pending,
        "total_count": entries.len(),
    })))
}

// ── Q2 Target Resolver: GET /api/wiki/inbox/{id}/candidates ────────
//
// Idempotent read route. Loads the target inbox entry and the full
// wiki page list, then delegates to
// `wiki_maintainer::resolve_target_candidates` for the pure scoring
// pass. Errors are surfaced with strict HTTP status codes so the UI
// can disambiguate "bad id" from "disk read failed":
//   * 404 — inbox entry not found
//   * 500 — wiki_store I/O error
//
// Optional `?with_graph=true` triggers the graph-signal second pass.
// We build the graph ONLY for the top-3 preliminary hits, which
// bounds the extra disk cost at 3 × (outgoing + backlinks + related)
// calls regardless of how large the wiki grows. The `with_graph`
// flag defaults to false so the fast path stays fast.

/// Query parameters for `GET /api/wiki/inbox/{id}/candidates`.
#[derive(Debug, Deserialize)]
pub(crate) struct InboxCandidatesQuery {
    /// When `true`, run the graph-signal enrichment pass after the
    /// preliminary top-3 is chosen. Defaults to `false`.
    #[serde(default)]
    with_graph: bool,
}

pub(crate) async fn list_inbox_candidates_handler(
    Path(id): Path<u32>,
    Query(query): Query<InboxCandidatesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;

    // Step 1: locate the inbox entry. `wiki_store` exposes
    // `list_inbox_entries` but no single-entry getter; scanning the
    // list is O(inbox_size) which is bounded at a few hundred by
    // design (see canonical §7.4 on inbox churn).
    let entries = wiki_store::list_inbox_entries(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_inbox_entries failed: {e}"),
            }),
        )
    })?;
    let entry = entries
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("inbox entry not found: {id}"),
                }),
            )
        })?;

    // Step 2: snapshot the wiki pages for the scorer. The helper
    // trims the full `WikiPageSummary` down to the four fields the
    // scorer consumes (slug / title / source_raw_id / category).
    let pages = wiki_store::list_page_summaries_for_resolver(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_page_summaries_for_resolver failed: {e}"),
            }),
        )
    })?;

    // Step 3: first-pass scoring. No graph signals yet.
    let preliminary = wiki_maintainer::resolve_target_candidates(&entry, &pages, None);

    // Step 4 (optional): second pass with graph signals. Build a
    // per-slug graph map ONLY for the preliminary hits so the cost
    // is bounded at the top-3 × O(page_read + backlinks).
    let candidates = if query.with_graph && !preliminary.is_empty() {
        let mut graphs: std::collections::HashMap<String, wiki_store::PageGraph> =
            std::collections::HashMap::new();
        for c in &preliminary {
            match wiki_store::get_page_graph(&paths, &c.slug) {
                Ok(g) => {
                    graphs.insert(c.slug.clone(), g);
                }
                Err(e) => {
                    // Soft-fail: graph enrichment is a best-effort
                    // boost. If one slug can't be read we drop its
                    // graph and keep the preliminary score. Logging
                    // rather than erroring matches the pattern of
                    // other "nice-to-have" Wiki reads.
                    eprintln!(
                        "list_inbox_candidates_handler: get_page_graph({}) failed: {e}",
                        c.slug
                    );
                }
            }
        }
        // Re-run the scorer with graph map so graph_* signals fold
        // into the final scores. `resolve_target_candidates` handles
        // re-sorting internally.
        wiki_maintainer::resolve_target_candidates(&entry, &pages, Some(&graphs))
    } else {
        preliminary
    };

    Ok(Json(serde_json::json!({
        "inbox_id": id,
        "candidates": candidates,
    })))
}

/// `GET /api/wiki/schema`
///
/// Return the current `schema/CLAUDE.md` content. The handler uses
/// `tokio::fs::read_to_string` (review nit #3) rather than blocking
/// `std::fs` to avoid stalling the axum executor thread on a
/// particularly slow disk.
///
/// `resolve_wiki_root_for_handler` already calls `init_wiki`, which
/// seeds `schema/CLAUDE.md` from the canonical template on fresh
/// installs — so the "file missing" branch that the S6 commit
/// originally carried was unreachable (review nit #4) and has been
/// removed. If a user deliberately `rm`s the file between `init_wiki`
/// and the handler run, the read fails and the caller sees a 500
/// with a clear "read CLAUDE.md failed" message, which is the
/// correct behavior.
pub(crate) async fn get_wiki_schema_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let claude_md = paths.schema_claude_md.clone();
    let content = tokio::fs::read_to_string(&claude_md).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("read CLAUDE.md failed: {e}"),
            }),
        )
    })?;
    let byte_size = content.len();
    Ok(Json(serde_json::json!({
        "path": claude_md.display().to_string(),
        "content": content,
        "source": "disk",
        "byte_size": byte_size,
    })))
}

/// `PUT /api/wiki/schema` (canonical §9.3 · feat M)
///
/// Overwrite `schema/CLAUDE.md` with new content. Canonical §8 says
/// "schema/ is human-only — the maintainer agent may PROPOSE changes
/// via Inbox but never writes here directly". This handler is the
/// HUMAN write path: the user opens SchemaEditor, edits, clicks
/// Save, frontend POSTs the new content here. The `Inbox proposal`
/// alternative path comes later via R + a future S2 sprint.
///
/// Body shape:
/// ```json
/// { "content": "# CLAUDE.md\n\n## Role\n..." }
/// ```
///
/// Behavior:
/// * Validates that content is non-empty (refuses to truncate
///   the schema with a blank PUT — that would orphan the maintainer
///   agent).
/// * Atomic write: tmp + rename via wiki_store::overwrite_schema.
/// * Logs to log.md as "edit-schema | CLAUDE.md".
/// * Returns the new byte size for client confirmation.
///
/// Errors:
/// * 400 — empty content
/// * 500 — disk write failure
pub(crate) async fn put_wiki_schema_handler(
    Json(body): Json<PutSchemaRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let trimmed = body.content.trim();
    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "schema content must not be empty".to_string(),
            }),
        ));
    }

    let paths = resolve_wiki_root_for_handler()?;
    wiki_store::overwrite_schema_claude_md(&paths, &body.content).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("schema write failed: {e}"),
            }),
        )
    })?;

    // Soft-fail audit log entry. Canonical §8 wants the schema
    // edits in the timeline alongside maintainer writes.
    if let Err(e) = wiki_store::append_wiki_log(&paths, "edit-schema", "CLAUDE.md") {
        eprintln!("put_wiki_schema: schema written but log append failed: {e}");
    }

    Ok(Json(serde_json::json!({
        "path": paths.schema_claude_md.display().to_string(),
        "byte_size": body.content.len(),
        "ok": true,
    })))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PutSchemaRequest {
    content: String,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct RulesFileQuery {
    path: String,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PutRulesFileRequest {
    path: String,
    content: String,
}

pub(crate) async fn get_rules_file_handler(
    Query(query): Query<RulesFileQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let file = wiki_store::read_rules_file_content(&paths, &query.path).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("rules file read failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(&file).unwrap_or_else(|_| serde_json::json!({ "ok": false })),
    ))
}

pub(crate) async fn put_rules_file_handler(
    Json(body): Json<PutRulesFileRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let file = wiki_store::overwrite_rules_file_content(&paths, &body.path, &body.content)
        .map_err(|e| {
            let status = match e {
                wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("rules file write failed: {e}"),
                }),
            )
        })?;

    if let Err(e) = wiki_store::append_wiki_log(&paths, "edit-rules-file", &file.relative_path) {
        eprintln!("put_rules_file: file written but log append failed: {e}");
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "relative_path": file.relative_path,
        "file_path": file.file_path,
        "content": file.content,
        "byte_size": file.byte_size,
    })))
}

/// `GET /api/wiki/pages/{slug}/backlinks` (feat Q)
///
/// Return every concept page that contains a markdown link to
/// `concepts/{slug}.md` in its body. This is the reverse lookup for
/// the bidirectional backlinks system required by canonical §8
/// Triggers row 3 ("A→B implies B→A"). Self-references excluded.
///
/// Returns `{ pages: [...WikiPageSummary] }`.
pub(crate) async fn get_wiki_backlinks_handler(
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let pages = wiki_store::list_backlinks(&paths, &slug).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_backlinks failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "pages": pages,
        "count": pages.len(),
    })))
}

/// `GET /api/wiki/graph` (canonical §9.3 · feat T)
///
/// Return the wiki graph: nodes (raw + concept) and edges
/// (`derived-from` for now; future feat(Q) adds backlink edges).
/// The Graph page consumes this to render a cognitive web with
/// raw entries on one layer and concept pages on the other,
/// connected by derivation arrows.
///
/// Empty wiki returns `{ nodes: [], edges: [], raw_count: 0,
/// concept_count: 0, edge_count: 0 }` so the frontend can render
/// an explicit "no data yet" state instead of an error.
pub(crate) async fn get_wiki_graph_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let graph = wiki_store::build_wiki_graph(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("build_wiki_graph failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(&graph).unwrap_or(serde_json::Value::Null),
    ))
}

/// `GET /api/wiki/pages/{slug}/graph` (G1)
///
/// Return the page-level graph for `slug`: the target page's own
/// header fields (slug/title/category/summary), its outgoing links,
/// its backlinks, and algorithmically-related pages (via shared
/// outgoing links + shared source_raw_id). All in one payload so
/// the frontend's per-page "Connections" panel renders with a
/// single request instead of three.
///
/// Response shape (serde-serialized [`wiki_store::PageGraph`]):
///
/// ```json
/// {
///   "slug": "hub",
///   "title": "Hub",
///   "category": "concept",
///   "summary": "one-line summary or null",
///   "outgoing": [{"slug": "...", "title": "...", "category": "..."}, ...],
///   "backlinks": [{"slug": "...", "title": "...", "category": "..."}, ...],
///   "related": [
///     {
///       "slug": "...",
///       "title": "...",
///       "category": "...",
///       "summary": "... or null",
///       "reasons": ["共享来源: raw #00042", "共同链接: spoke-a"],
///       "score": 5
///     },
///     ...
///   ]
/// }
/// ```
///
/// Errors:
///   * `404 Not Found` — slug validates but no such wiki page.
///   * `400 Bad Request` — slug fails validation (empty, too long,
///                         invalid chars).
///   * `500 Internal Server Error` — I/O failure mid-walk.
pub(crate) async fn get_page_graph_handler(
    Path(slug): Path<String>,
) -> Result<Json<wiki_store::PageGraph>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let graph = wiki_store::get_page_graph(&paths, &slug).map_err(|e| match e {
        wiki_store::WikiStoreError::Invalid(msg) => {
            // `get_page_graph` uses `Invalid` for both "slug failed
            // validation" and "no such page". Surface as 404 when the
            // message clearly points at a missing page; otherwise 400.
            let is_missing = msg.starts_with("wiki page not found");
            let status = if is_missing {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("page graph: {msg}"),
                }),
            )
        }
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("get_page_graph failed: {other}"),
            }),
        ),
    })?;
    Ok(Json(graph))
}

/// Query parameters for `GET /api/lineage/wiki/:slug`.
///
/// Pagination is server-side: the scanner collects every matching
/// event, sorts descending, then slices. Defaults mirror the
/// Lineage tab's initial render (10 rows, offset 0).
#[derive(Debug, Deserialize)]
pub(crate) struct WikiLineageQuery {
    #[serde(default = "default_lineage_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_lineage_limit() -> usize {
    10
}

/// `GET /api/lineage/wiki/:slug?limit=10&offset=0`
///
/// Returns every lineage event touching the given wiki slug
/// (upstream or downstream), sorted newest-first, sliced by
/// `offset` + `limit`. Used by the Wiki page's Lineage tab to
/// render "what happened to this page" as a timeline.
pub(crate) async fn get_wiki_lineage_handler(
    Path(slug): Path<String>,
    Query(query): Query<WikiLineageQuery>,
) -> Result<Json<wiki_store::provenance::WikiLineageResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let resp =
        wiki_store::provenance::read_lineage_for_wiki(&paths, &slug, query.limit, query.offset);
    Ok(Json(resp))
}

/// `GET /api/lineage/inbox/:id`
///
/// Returns two lineage buckets for the given inbox id:
///   * `upstream_events` — events where `Inbox{id}` appears as a
///     downstream ref, i.e. "what produced this inbox task"
///     (typically the `inbox_appended` and one `raw_written`).
///   * `downstream_events` — events where `Inbox{id}` appears as an
///     upstream ref, i.e. "what did this inbox drive"
///     (proposal_generated / wiki_page_applied / inbox_rejected).
pub(crate) async fn get_inbox_lineage_handler(
    Path(id): Path<u32>,
) -> Result<Json<wiki_store::provenance::InboxLineageResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let resp = wiki_store::provenance::read_lineage_for_inbox(&paths, id);
    Ok(Json(resp))
}

/// `GET /api/lineage/raw/:id`
///
/// Returns every lineage event whose upstream or downstream
/// mentions the given raw id. Flat list sorted newest-first —
/// a raw's lineage is naturally short (write → inbox → proposal →
/// apply) so pagination isn't needed for the MVP.
pub(crate) async fn get_raw_lineage_handler(
    Path(id): Path<u32>,
) -> Result<Json<wiki_store::provenance::RawLineageResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let resp = wiki_store::provenance::read_lineage_for_raw(&paths, id);
    Ok(Json(resp))
}

pub(crate) async fn resolve_wiki_inbox_handler(
    Path(id): Path<u32>,
    Json(body): Json<ResolveInboxRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let updated =
        wiki_store::resolve_inbox_entry(&paths, id, &body.action).map_err(|e| match e {
            wiki_store::WikiStoreError::NotFound(_) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("inbox entry not found: {id}"),
                }),
            ),
            wiki_store::WikiStoreError::Invalid(msg) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid inbox action: {msg}"),
                }),
            ),
            other => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("resolve_inbox_entry failed: {other}"),
                }),
            ),
        })?;
    fire_inbox_notify(); // feat(O): instant WS push
    Ok(Json(serde_json::json!({ "entry": updated })))
}

// ── Q1: Batch resolve for Inbox Queue Intelligence ───────────────
//
// `POST /api/wiki/inbox/batch/resolve` — resolve many inbox entries
// in one HTTP round trip. Motivation per Q1 contract: the frontend's
// Batch Triage mode multi-selects pending tasks and applies the same
// action (reject for MVP; approve reserved for a future sprint). A
// naive per-id fan-out would issue N HTTP calls from the browser;
// this endpoint collapses that into a single request that loops over
// `wiki_store::resolve_inbox_entry` internally.
//
// Design notes:
// * Partial success is allowed: each id is resolved independently,
//   failures go to `failed[]`, successes to `success[]`. No
//   transaction. This mirrors the UI expectation — if id #3 is a
//   stale reference, ids #1/#2/#4 should still land.
// * `action` is a string for forward compatibility. Q1 MVP only
//   accepts `"reject"`; `"approve"` is reserved and returns 400
//   "not supported in Q1" because the approve path has non-trivial
//   write side effects (wiki page creation) that can't be pipelined
//   safely in a batch loop. A later sprint can relax this.
// * `reason` is required when `action == "reject"` and must be at
//   least 4 chars — keeps the audit log useful. `approve` ignores
//   the field.
// * Locking: `wiki_store::resolve_inbox_entry` already serializes
//   on `INBOX_WRITE_GUARD`, so we just call it in a loop. Each
//   iteration acquires / releases the guard; we don't hold it across
//   the whole batch to avoid starving single-id resolves that race
//   against a long batch. The brief gap between iterations is fine
//   because each id resolution is self-contained.

#[derive(Debug, Deserialize)]
pub(crate) struct BatchResolveInboxRequest {
    /// Inbox entry ids to resolve. Empty list → 400.
    ids: Vec<u32>,
    /// `"reject"` (Q1 MVP) or `"approve"` (reserved, returns 400).
    action: String,
    /// Rejection reason, required for `action == "reject"` with
    /// `len >= 4`. Ignored for `approve`.
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BatchResolveInboxResponse {
    /// Ids that resolved successfully.
    success: Vec<u32>,
    /// Ids that failed, with a per-id error message.
    failed: Vec<BatchFailedItem>,
    /// Total ids submitted (== `success.len() + failed.len()`).
    total: u32,
    /// Count of successes (mirrors `success.len()`) — convenience for
    /// the Inbox toast "已处理 N/M" summary.
    processed: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct BatchFailedItem {
    id: u32,
    error: String,
}

pub(crate) async fn batch_resolve_wiki_inbox_handler(
    Json(body): Json<BatchResolveInboxRequest>,
) -> Result<Json<BatchResolveInboxResponse>, ApiError> {
    // Step 1 — request validation. Empty ids would silently return
    // `total=0` which is almost certainly a bug on the caller side;
    // fail loudly instead.
    if body.ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ids must not be empty".to_string(),
            }),
        ));
    }

    // Step 2 — action whitelist. Q1 MVP: only `reject` is pipelined.
    match body.action.as_str() {
        "reject" => {}
        "approve" => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "batch approve is not supported in Q1".to_string(),
                }),
            ));
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("unknown inbox action: {other}"),
                }),
            ));
        }
    }

    // Step 3 — reason sanity. Reject path demands a >=4 char reason
    // so the audit log produced by `wiki_store::resolve_inbox_entry`
    // has something human-meaningful behind each rejection.
    if body.action == "reject" {
        let reason_ok = body
            .reason
            .as_deref()
            .map(|r| r.trim().chars().count() >= 4)
            .unwrap_or(false);
        if !reason_ok {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "reason is required (>=4 chars) when action=reject".to_string(),
                }),
            ));
        }
    }

    let paths = resolve_wiki_root_for_handler()?;
    let total = body.ids.len() as u32;

    // Step 4 — per-id loop. Each `resolve_inbox_entry` call takes
    // the shared inbox write guard internally, so parallel spawns
    // would just serialize on the lock without a throughput win;
    // keep it sequential and capture per-id errors for the response.
    let mut success: Vec<u32> = Vec::with_capacity(body.ids.len());
    let mut failed: Vec<BatchFailedItem> = Vec::new();

    for id in body.ids.iter().copied() {
        match wiki_store::resolve_inbox_entry(&paths, id, &body.action) {
            Ok(_entry) => success.push(id),
            Err(e) => {
                let msg = match &e {
                    wiki_store::WikiStoreError::NotFound(_) => {
                        format!("inbox entry not found: {id}")
                    }
                    wiki_store::WikiStoreError::Invalid(m) => {
                        format!("invalid inbox action: {m}")
                    }
                    other => format!("resolve_inbox_entry failed: {other}"),
                };
                failed.push(BatchFailedItem { id, error: msg });
            }
        }
    }

    // Fire a single WS notify after the batch settles rather than
    // once per id — clients only need one repaint to re-read the
    // inbox after this call returns.
    if !success.is_empty() {
        fire_inbox_notify();
    }

    let processed = success.len() as u32;
    Ok(Json(BatchResolveInboxResponse {
        success,
        failed,
        total,
        processed,
    }))
}

fn raw_entry_to_json(entry: &wiki_store::RawEntry) -> serde_json::Value {
    // M4 observability: surface canonical/original URL pair + content
    // hash on the wire so the Inbox Workbench Evidence section can
    // render URLTrackBadge / IngestDecisionBadge without a second API
    // call. `canonical_url` is the same string as `source_url` (M4's
    // convention: source_url in frontmatter is always canonical) but
    // surfacing it under a named field makes the frontend contract
    // explicit and self-documenting.
    serde_json::json!({
        "id": entry.id,
        "filename": entry.filename,
        "source": entry.source,
        "slug": entry.slug,
        "date": entry.date,
        "source_url": entry.source_url,
        "canonical_url": entry.source_url,
        "original_url": entry.original_url,
        "ingested_at": entry.ingested_at,
        "byte_size": entry.byte_size,
        "content_hash": entry.content_hash,
    })
}

// ── ClawWiki S4: maintainer MVP HTTP handlers ─────────────────────
//
// These handlers wrap `wiki_maintainer::propose_for_raw_entry` and
// `wiki_store::{write_wiki_page, list_wiki_pages, read_wiki_page}`
// — the engram-style MVP per canonical §4 blade 3.
//
// Design notes:
//
// * `propose` NEVER touches the filesystem. It reads the raw entry,
//   fires one chat_completion through the process-global broker via
//   `BrokerAdapter::from_global`, parses the JSON, returns. If the
//   pool is empty the handler returns 503 with a clear message so
//   the frontend can render an "add a Codex account" CTA.
// * `approve-with-write` takes the proposal *from the request body*,
//   not from any server-side cache. This is deliberate — we don't
//   want to hold LLM outputs in memory between requests. The
//   frontend keeps the proposal in its own state and re-sends on
//   approve. Write goes first, then `resolve_inbox_entry(approve)`.
//   Worst case on partial failure: page is on disk, inbox still
//   pending. The user can retry the approve and get a 200 from the
//   second resolve_inbox_entry call.
// * `list_wiki_pages` and `get_wiki_page` are plain read routes;
//   no auth, no permissions (ClawWiki's entire wiki/ is local and
//   user-owned — anyone with access to the desktop-server binding
//   has access to the wiki).

/// `POST /api/wiki/inbox/{id}/propose`
///
/// Produce a `WikiPageProposal` for the raw entry referenced by the
/// given inbox task. The inbox entry itself is NOT mutated — this
/// route only previews. A follow-up `approve-with-write` call is
/// required to persist anything.
///
/// Errors:
///   * 404 if the inbox entry doesn't exist or has no source_raw_id
///   * 404 if the raw entry is gone (stale inbox)
///   * 503 if the Codex broker has no usable account in the pool
///   * 502 if the LLM returns bad JSON
///   * 500 on unexpected I/O failure
pub(crate) async fn propose_wiki_inbox_handler(
    Path(id): Path<u32>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;

    // Step 1: find the inbox entry and pull its source_raw_id.
    let entries = wiki_store::list_inbox_entries(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_inbox_entries failed: {e}"),
            }),
        )
    })?;
    let entry = entries.iter().find(|e| e.id == id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("inbox entry not found: {id}"),
            }),
        )
    })?;
    let raw_id = entry.source_raw_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("inbox entry {id} has no source_raw_id"),
            }),
        )
    })?;

    // Step 2: build an auth adapter. In OSS builds this goes straight
    // to the providers.json fallback; in private-cloud builds it tries
    // the managed broker first and then falls back to providers.json.
    let adapter = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global();

    // Step 3: fire the proposal.
    let proposal = wiki_maintainer::propose_for_raw_entry(&paths, raw_id, &adapter)
        .await
        .map_err(|e| match e {
            wiki_maintainer::MaintainerError::RawNotAvailable(msg) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("raw entry not available: {msg}"),
                }),
            ),
            wiki_maintainer::MaintainerError::Broker(msg) => {
                // Empty-broker / no-provider cases land here via the
                // adapter's string flattening. Pin the 503 on anything
                // that looks like "no usable auth source"; everything
                // else is an upstream LLM error worth a 502.
                let is_empty_pool = msg.contains("no codex account")
                    || msg.contains("pool_size")
                    || msg.contains("no providers.json fallback");
                let code = if is_empty_pool {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::BAD_GATEWAY
                };
                (
                    code,
                    Json(ErrorResponse {
                        error: format!("broker error: {msg}"),
                    }),
                )
            }
            wiki_maintainer::MaintainerError::BadJson { reason, preview } => (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("LLM returned malformed JSON: {reason}; preview: {preview}"),
                }),
            ),
            wiki_maintainer::MaintainerError::InvalidProposal(msg) => (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("LLM proposal shape invalid: {msg}"),
                }),
            ),
            wiki_maintainer::MaintainerError::Store(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("wiki store error: {msg}"),
                }),
            ),
            wiki_maintainer::MaintainerError::Cancelled => (
                StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
                Json(ErrorResponse {
                    error: "absorb cancelled by user".to_string(),
                }),
            ),
        })?;

    Ok(Json(serde_json::json!({
        "proposal": wiki_page_proposal_to_json(&proposal),
        "inbox_id": id,
        "source_raw_id": raw_id,
    })))
}

/// Request body for `POST /api/wiki/inbox/{id}/approve-with-write`.
///
/// The frontend re-sends the full proposal object it received from
/// `propose` (the server doesn't cache proposals). The frontend is
/// allowed to edit the fields before approving — so the user can
/// fix a typo in the title or trim the body before persisting.
#[derive(Debug, Deserialize)]
pub(crate) struct ApproveWithWriteRequest {
    proposal: WikiPageProposalBody,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WikiPageProposalBody {
    slug: String,
    title: String,
    summary: String,
    body: String,
    #[serde(default)]
    source_raw_id: Option<u32>,
}

/// `POST /api/wiki/inbox/{id}/approve-with-write`
///
/// Persist the proposal as a wiki page and resolve the inbox entry
/// as `approved`. Two-step operation:
///   1. `wiki_store::write_wiki_page(slug, title, summary, body)`
///   2. `wiki_store::resolve_inbox_entry(id, "approve")`
///
/// Step 1 failures (invalid slug, I/O error) fail the whole request
/// with nothing written. Step 2 failures (inbox already resolved,
/// missing, etc.) are logged but do not fail the request — the
/// wiki page IS on disk at that point and the user can re-approve
/// from the Inbox UI to finish the bookkeeping. This is the "write
/// first, bookkeep second" pattern from the plan.
pub(crate) async fn approve_wiki_inbox_with_write_handler(
    Path(id): Path<u32>,
    Json(body): Json<ApproveWithWriteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let p = body.proposal;

    // Defense in depth: even though wiki_maintainer already
    // validated the slug in `parse_proposal`, the frontend might
    // have edited the proposal before re-sending. Let
    // wiki_store::write_wiki_page validate again.
    let written_path = wiki_store::write_wiki_page(
        &paths,
        &p.slug,
        &p.title,
        &p.summary,
        &p.body,
        p.source_raw_id,
    )
    .map_err(|e| match e {
        wiki_store::WikiStoreError::Invalid(msg) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid wiki page: {msg}"),
            }),
        ),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("write_wiki_page failed: {other}"),
            }),
        ),
    })?;

    // Step 1.5: Karpathy llm-wiki.md §"Indexing and logging" + canonical
    // §8 Triggers — after every wiki write, append to log.md and
    // rebuild index.md so the two special files stay current. Both
    // are soft-fail: the concept page is already persisted and the
    // user's approve succeeded; a missing log entry or stale index is
    // a maintenance problem the next write will fix on its own, NOT
    // a reason to fail the user's action.
    let log_title = if p.title.is_empty() {
        p.slug.clone()
    } else {
        p.title.clone()
    };
    if let Err(e) = wiki_store::append_wiki_log(&paths, "write-concept", &log_title) {
        eprintln!("approve-with-write: wiki page written but log append failed: {e}");
    }
    // feat(S): also append to per-day changelog file (canonical §8
    // Triggers row 5). Same soft-fail policy as the log: missing
    // entries are recoverable, the page is already persisted.
    if let Err(e) = wiki_store::append_changelog_entry(&paths, "write-concept", &log_title) {
        eprintln!("approve-with-write: wiki page written but changelog append failed: {e}");
    }
    if let Err(e) = wiki_store::rebuild_wiki_index(&paths) {
        eprintln!("approve-with-write: wiki page written but index rebuild failed: {e}");
    }
    // feat(P): scan existing concept pages for mentions of the newly
    // written page and create Stale inbox entries. Canonical §8
    // Triggers row 2: "update affected pages". This is the notification
    // half; the actual LLM re-write is future work.
    match wiki_store::notify_affected_pages(&paths, &p.slug, &p.title) {
        Ok(n) if n > 0 => {
            eprintln!(
                "approve-with-write: notified {n} affected page(s) about new `{}`",
                p.slug
            );
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("approve-with-write: notify_affected_pages failed (non-fatal): {e}");
        }
    }

    // Step 2: flip the inbox entry to approved. Soft-fail: we log
    // and keep going even if resolve fails, because the wiki page
    // is already persisted and re-running the approve from the
    // frontend will get a 200 next time.
    let inbox_result = wiki_store::resolve_inbox_entry(&paths, id, "approve");
    let inbox_entry_json = match inbox_result {
        Ok(updated) => Some(serde_json::to_value(&updated).unwrap_or(serde_json::Value::Null)),
        Err(e) => {
            eprintln!("approve-with-write: wiki page written but inbox resolve failed: {e}");
            None
        }
    };

    fire_inbox_notify(); // feat(O): instant WS push
    Ok(Json(serde_json::json!({
        "written_path": written_path.display().to_string(),
        "slug": p.slug,
        "inbox_entry": inbox_entry_json,
    })))
}

// ── W1 Maintainer Workbench: POST /api/wiki/inbox/{id}/maintain ─────
//
// Flat request body (aligned with the TS `MaintainRequest` shape):
//   { action: "create_new" | "update_existing" | "reject",
//     purpose_lenses?: string[],
//     target_page_slug?: string,
//     rejection_reason?: string }
//
// Flat response (aligned with TS `MaintainResponse`):
//   { outcome: "created" | "updated" | "rejected" | "failed",
//     target_page_slug?: string,
//     rejection_reason?: string,
//     error?: string }
//
// Validation: `update_existing` requires a non-empty `target_page_slug`;
// `reject` requires `rejection_reason` with length ≥ 4. Both failures
// return 400. Anything unexpected (LLM error, disk I/O) becomes 200
// with `outcome: "failed"` — the frontend renders that as an inline
// error banner instead of a retry-the-request flow, because the inbox
// entry has already received whatever partial state the backend could
// commit.

#[derive(Debug, Deserialize)]
pub(crate) struct InboxMaintainRequest {
    /// `"create_new"` | `"update_existing"` | `"reject"`. Kept as a
    /// free string here so an unknown action returns a friendly 400
    /// instead of a serde parse error.
    action: String,
    /// Optional when `action == "create_new"`; empty falls back to the
    /// Buddy default (`learning`) inside wiki_store.
    #[serde(default)]
    purpose_lenses: Vec<String>,
    /// Required when `action == "update_existing"`.
    #[serde(default)]
    target_page_slug: Option<String>,
    /// Required when `action == "reject"`; min 4 chars.
    #[serde(default)]
    rejection_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InboxMaintainResponse {
    /// `"created"` | `"updated"` | `"rejected"` | `"failed"`.
    outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_page_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rejection_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// `POST /api/wiki/inbox/{id}/maintain` — run a three-choice
/// maintainer action end-to-end.
///
/// The handler translates the flat frontend contract into the
/// tagged `wiki_maintainer::MaintainAction` enum, calls
/// `execute_maintain`, and flattens the resulting `MaintainOutcome`
/// back onto `InboxMaintainResponse`. The frontend's `maintainInboxEntry`
/// wrapper (in `apps/desktop-shell/src/lib/tauri.ts`) shapes the
/// request body; the frontend's `InboxEntry` rendering reads the
/// augmented fields (`maintain_action`, `target_page_slug`, etc.)
/// that `execute_maintain` wrote to disk.
pub(crate) async fn inbox_maintain_handler(
    Path(id): Path<u32>,
    Json(body): Json<InboxMaintainRequest>,
) -> Result<Json<InboxMaintainResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;

    // Step 1: translate the flat action into the tagged enum, with
    // strict validation of the per-variant required fields.
    let action = match body.action.as_str() {
        "create_new" => wiki_maintainer::MaintainAction::CreateNew {
            purpose_lenses: body.purpose_lenses.clone(),
        },
        "update_existing" => {
            let slug = body
                .target_page_slug
                .as_ref()
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if slug.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "action=update_existing requires a non-empty target_page_slug"
                            .to_string(),
                    }),
                ));
            }
            wiki_maintainer::MaintainAction::UpdateExisting {
                target_page_slug: slug,
                purpose_lenses: body.purpose_lenses.clone(),
            }
        }
        "reject" => {
            let reason = body
                .rejection_reason
                .as_ref()
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if reason.chars().count() < 4 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "action=reject requires rejection_reason of at least 4 chars"
                            .to_string(),
                    }),
                ));
            }
            wiki_maintainer::MaintainAction::Reject { reason }
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!(
                        "unknown maintain action `{other}` (expected create_new | update_existing | reject)"
                    ),
                }),
            ));
        }
    };

    // Step 2: fetch a broker adapter (only create_new consumes it, but
    // the enum dispatcher needs an instance in all cases).
    let adapter = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global();

    // Step 3: run the action. On error, flatten into a `Failed` outcome
    // rather than bubbling a 5xx — the frontend uses `error` as an
    // inline warning in the Workbench result pane.
    let outcome_result = wiki_maintainer::execute_maintain(&paths, id, action, &adapter).await;

    let response = match outcome_result {
        Ok(wiki_maintainer::MaintainOutcome::Created { target_page_slug }) => {
            fire_inbox_notify();
            InboxMaintainResponse {
                outcome: "created".to_string(),
                target_page_slug: Some(target_page_slug),
                rejection_reason: None,
                error: None,
            }
        }
        Ok(wiki_maintainer::MaintainOutcome::Updated { target_page_slug }) => {
            fire_inbox_notify();
            InboxMaintainResponse {
                outcome: "updated".to_string(),
                target_page_slug: Some(target_page_slug),
                rejection_reason: None,
                error: None,
            }
        }
        Ok(wiki_maintainer::MaintainOutcome::Rejected { reason }) => {
            fire_inbox_notify();
            InboxMaintainResponse {
                outcome: "rejected".to_string(),
                target_page_slug: None,
                rejection_reason: Some(reason),
                error: None,
            }
        }
        Ok(wiki_maintainer::MaintainOutcome::Failed { error }) => InboxMaintainResponse {
            outcome: "failed".to_string(),
            target_page_slug: None,
            rejection_reason: None,
            error: Some(error),
        },
        Err(e) => InboxMaintainResponse {
            outcome: "failed".to_string(),
            target_page_slug: None,
            rejection_reason: None,
            error: Some(format!("{e}")),
        },
    };

    Ok(Json(response))
}

// ── W2 Proposal/Apply: two-phase update_existing ─────────────────
//
// Three endpoints, one per phase of the proposal lifecycle:
//
//   POST /api/wiki/inbox/{id}/proposal         — create a proposal
//   POST /api/wiki/inbox/{id}/proposal/apply   — commit to disk
//   POST /api/wiki/inbox/{id}/proposal/cancel  — discard
//
// Request / response shapes are pinned here and mirrored in the TS
// contract Worker B owns. Body validation stays minimal — the heavy
// lifting happens in `wiki_maintainer::{propose_update,
// apply_update_proposal, cancel_update_proposal}`.

/// Request body for `POST /api/wiki/inbox/{id}/proposal`.
#[derive(Debug, Deserialize)]
pub(crate) struct CreateProposalRequest {
    /// Slug of the target wiki page to merge the raw body into.
    /// Required, must be non-empty after trim.
    target_slug: String,
}

/// Response body for `POST /api/wiki/inbox/{id}/proposal`.
///
/// Mirrors `wiki_maintainer::UpdateProposal` field-for-field. We use
/// a dedicated struct here (rather than forwarding the crate type)
/// so future wire-shape evolutions (e.g. add `conflicts`, `warning`)
/// can happen without coupling the domain type to HTTP.
#[derive(Debug, Serialize)]
pub(crate) struct ProposalResponse {
    target_slug: String,
    before_markdown: String,
    after_markdown: String,
    summary: String,
    generated_at: u64,
}

impl From<wiki_maintainer::UpdateProposal> for ProposalResponse {
    fn from(p: wiki_maintainer::UpdateProposal) -> Self {
        Self {
            target_slug: p.target_slug,
            before_markdown: p.before_markdown,
            after_markdown: p.after_markdown,
            summary: p.summary,
            generated_at: p.generated_at,
        }
    }
}

/// Response body for `POST /api/wiki/inbox/{id}/proposal/apply`.
///
/// Uses the same `outcome` flat shape as `InboxMaintainResponse` so
/// the frontend can dispatch on a consistent `outcome` field across
/// endpoints. `error` is populated on conflict / internal failure.
#[derive(Debug, Serialize)]
pub(crate) struct ApplyProposalResponse {
    outcome: String, // "updated" | "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    target_page_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Response body for `POST /api/wiki/inbox/{id}/proposal/cancel`.
/// Deliberately minimal — the UI just needs a "cancelled or error".
#[derive(Debug, Serialize)]
pub(crate) struct CancelProposalResponse {
    outcome: String, // "cancelled" | "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// `POST /api/wiki/inbox/{id}/proposal`
///
/// Phase 1 of W2's two-phase update. Fires one LLM merge call, stages
/// the result on the inbox entry, and returns the diff for review.
///
/// 400 if `target_slug` is missing / empty. 200 with a populated
/// response on success. Internal failures (broker down, parse error)
/// come back as 5xx because they're not user-recoverable — the UI
/// retries rather than rendering a partial diff.
pub(crate) async fn create_proposal_handler(
    Path(id): Path<u32>,
    Json(body): Json<CreateProposalRequest>,
) -> Result<Json<ProposalResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;

    let slug = body.target_slug.trim().to_string();
    if slug.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "target_slug is required and must be non-empty".to_string(),
            }),
        ));
    }

    let adapter = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global();
    let proposal = wiki_maintainer::propose_update(&paths, id, &slug, &adapter)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("propose_update failed: {e}"),
                }),
            )
        })?;

    fire_inbox_notify();
    Ok(Json(ProposalResponse::from(proposal)))
}

/// `POST /api/wiki/inbox/{id}/proposal/apply`
///
/// Phase 2 of W2. Commits the staged `proposed_after_markdown` to
/// disk, flips the inbox entry to `approved`, clears the staging.
/// Returns 200 with `outcome="failed"` on conflict (concurrent
/// external edit) so the UI can show an inline warning and offer a
/// "re-propose" button. A missing-proposal error becomes 400
/// because that's a state precondition the caller should know about.
pub(crate) async fn apply_proposal_handler(
    Path(id): Path<u32>,
) -> Result<Json<ApplyProposalResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    match wiki_maintainer::apply_update_proposal(&paths, id) {
        Ok(wiki_maintainer::MaintainOutcome::Updated { target_page_slug }) => {
            fire_inbox_notify();
            Ok(Json(ApplyProposalResponse {
                outcome: "updated".to_string(),
                target_page_slug: Some(target_page_slug),
                error: None,
            }))
        }
        // execute_maintain uses `Failed` variant; apply_update_proposal
        // doesn't produce it today but we fold it in defensively.
        Ok(other) => Ok(Json(ApplyProposalResponse {
            outcome: "failed".to_string(),
            target_page_slug: None,
            error: Some(format!("unexpected outcome: {other:?}")),
        })),
        Err(wiki_maintainer::MaintainerError::InvalidProposal(msg)) => {
            Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })))
        }
        Err(e) => {
            // Concurrent-edit conflicts surface as `Store` errors;
            // fold them into a structured 200 response so the UI can
            // render a warning card instead of a toast.
            Ok(Json(ApplyProposalResponse {
                outcome: "failed".to_string(),
                target_page_slug: None,
                error: Some(format!("{e}")),
            }))
        }
    }
}

/// `POST /api/wiki/inbox/{id}/proposal/cancel`
///
/// Discards the staged proposal. Returns 200 on success (including
/// the no-op case where there was nothing staged) and 4xx only if
/// the inbox id is unknown.
pub(crate) async fn cancel_proposal_handler(
    Path(id): Path<u32>,
) -> Result<Json<CancelProposalResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    match wiki_maintainer::cancel_update_proposal(&paths, id) {
        Ok(()) => {
            fire_inbox_notify();
            Ok(Json(CancelProposalResponse {
                outcome: "cancelled".to_string(),
                error: None,
            }))
        }
        Err(wiki_maintainer::MaintainerError::RawNotAvailable(msg)) => {
            Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: msg })))
        }
        Err(wiki_maintainer::MaintainerError::Store(msg))
            if msg.to_lowercase().contains("not found") =>
        {
            Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: msg })))
        }
        Err(e) => Ok(Json(CancelProposalResponse {
            outcome: "failed".to_string(),
            error: Some(format!("{e}")),
        })),
    }
}

// ── W3 Combined Proposal endpoints ──────────────────────────────────
//
//   POST /api/wiki/proposal/combined         — preview (no staging)
//   POST /api/wiki/proposal/combined/apply   — atomic write + N flip
//
// Request/response shapes are pinned here and the same struct names
// appear verbatim in `apps/desktop-shell/src/lib/protocol.generated.ts`
// after the codegen run. Unlike the single-source W2 path, the
// preview body carries the full `{target_slug, inbox_ids}` pair
// (no path-encoded id) because a combined preview has no natural
// single-inbox anchor.

/// Request body for `POST /api/wiki/proposal/combined`.
#[derive(Debug, Deserialize)]
pub(crate) struct CombinedProposalRequest {
    target_slug: String,
    inbox_ids: Vec<u32>,
}

/// HTTP-layer mirror of [`wiki_maintainer::CombinedProposalResponse`].
/// We duplicate the struct rather than forward the domain type so
/// future wire-shape evolutions (e.g. add `warnings: Vec<String>`)
/// don't couple the crate-internal type to HTTP.
#[derive(Debug, Serialize)]
pub(crate) struct CombinedProposalResponse {
    target_slug: String,
    inbox_ids: Vec<u32>,
    before_markdown: String,
    after_markdown: String,
    summary: String,
    before_hash: String,
    generated_at: i64,
    source_titles: Vec<CombinedProposalSource>,
}

/// Per-source description that rides alongside the combined preview.
/// Mirrors [`wiki_maintainer::CombinedProposalSource`] for the wire.
#[derive(Debug, Serialize)]
pub(crate) struct CombinedProposalSource {
    inbox_id: u32,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_raw_id: Option<u32>,
}

impl From<wiki_maintainer::CombinedProposalResponse> for CombinedProposalResponse {
    fn from(p: wiki_maintainer::CombinedProposalResponse) -> Self {
        Self {
            target_slug: p.target_slug,
            inbox_ids: p.inbox_ids,
            before_markdown: p.before_markdown,
            after_markdown: p.after_markdown,
            summary: p.summary,
            before_hash: p.before_hash,
            generated_at: p.generated_at,
            source_titles: p
                .source_titles
                .into_iter()
                .map(|s| CombinedProposalSource {
                    inbox_id: s.inbox_id,
                    title: s.title,
                    source_raw_id: s.source_raw_id,
                })
                .collect(),
        }
    }
}

/// Request body for `POST /api/wiki/proposal/combined/apply`.
#[derive(Debug, Deserialize)]
pub(crate) struct CombinedApplyRequest {
    target_slug: String,
    inbox_ids: Vec<u32>,
    expected_before_hash: String,
    after_markdown: String,
    summary: String,
}

/// Response body for `POST /api/wiki/proposal/combined/apply`. Thin
/// mirror of [`wiki_maintainer::CombinedApplyResult`] so future
/// response-shape evolutions (e.g. add warning strings) don't leak
/// into the maintainer crate.
#[derive(Debug, Serialize)]
pub(crate) struct CombinedApplyResponse {
    outcome: String,
    target_page_slug: String,
    applied_inbox_ids: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failed_inbox_ids: Vec<u32>,
    audit_entry: String,
}

impl From<wiki_maintainer::CombinedApplyResult> for CombinedApplyResponse {
    fn from(r: wiki_maintainer::CombinedApplyResult) -> Self {
        Self {
            outcome: r.outcome,
            target_page_slug: r.target_page_slug,
            applied_inbox_ids: r.applied_inbox_ids,
            failed_inbox_ids: r.failed_inbox_ids,
            audit_entry: r.audit_entry,
        }
    }
}

/// Translate a `MaintainerError` into a `StatusCode + ErrorResponse`
/// pair appropriate for the combined preview/apply handlers. Split
/// out so both handlers share the same mapping.
fn combined_error_to_api(e: wiki_maintainer::MaintainerError) -> ApiError {
    use wiki_maintainer::MaintainerError;
    match e {
        MaintainerError::InvalidProposal(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg }))
        }
        MaintainerError::RawNotAvailable(msg) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("raw entry unavailable: {msg}"),
            }),
        ),
        MaintainerError::Store(msg) => {
            let lower = msg.to_lowercase();
            if lower.contains("not found") {
                (StatusCode::NOT_FOUND, Json(ErrorResponse { error: msg }))
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: msg }),
                )
            }
        }
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{other}"),
            }),
        ),
    }
}

/// `POST /api/wiki/proposal/combined`
///
/// W3 Phase 1 — fold 2..=6 inbox entries into one diff for the target
/// page via a single LLM call. Does NOT write anything to the inbox
/// file; the response is ephemeral and the frontend echoes the
/// critical pieces (`after_markdown`, `summary`, `before_hash`) back
/// on apply.
///
/// Errors:
///   * 400 if `inbox_ids.len() ∉ 2..=6`, any id isn't Pending, any
///     entry lacks a `source_raw_id`, or a duplicate id is passed.
///   * 404 if the target page is missing or the raw entry behind an
///     inbox id is missing.
///   * 500 on broker / LLM parse failures.
pub(crate) async fn create_combined_proposal_handler(
    Json(body): Json<CombinedProposalRequest>,
) -> Result<Json<CombinedProposalResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let slug = body.target_slug.trim().to_string();

    let adapter = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global();
    let proposal =
        wiki_maintainer::propose_combined_update(&paths, &slug, &body.inbox_ids, &adapter)
            .await
            .map_err(combined_error_to_api)?;

    Ok(Json(CombinedProposalResponse::from(proposal)))
}

/// `POST /api/wiki/proposal/combined/apply`
///
/// W3 Phase 2 — atomic-ish apply. Writes `after_markdown` to the
/// target page first, then flips each of the N inbox entries to
/// Approved. Partial-flip failures do NOT roll back the wiki write;
/// instead the response carries `outcome: "partial_applied"` with
/// `failed_inbox_ids`.
///
/// `outcome` values the frontend must branch on:
///   * `"applied"` — full success.
///   * `"partial_applied"` — wiki write OK, at least one flip failed.
///   * `"concurrent_edit"` — the page changed between preview and
///     apply (detected via SHA-256 of the current body vs
///     `expected_before_hash`); NO write happened. 200 OK, UI should
///     re-preview.
///   * `"stale_inbox"` — one or more inbox ids are gone or no longer
///     Pending; NO write happened. 200 OK, UI should re-fetch.
///
/// Errors (4xx/5xx): validation failure, missing target page, LLM
/// parse failure on an upstream re-read, etc.
pub(crate) async fn apply_combined_proposal_handler(
    Json(body): Json<CombinedApplyRequest>,
) -> Result<Json<CombinedApplyResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let slug = body.target_slug.trim().to_string();

    let result = wiki_maintainer::apply_combined_proposal(
        &paths,
        &slug,
        &body.inbox_ids,
        &body.expected_before_hash,
        &body.after_markdown,
        &body.summary,
    )
    .map_err(combined_error_to_api)?;

    // Fire inbox notify only when we actually mutated inbox state.
    // concurrent_edit / stale_inbox bail out before any flip runs,
    // so avoid a spurious WS push that would trigger a needless
    // client-side refetch.
    if matches!(result.outcome.as_str(), "applied" | "partial_applied") {
        fire_inbox_notify();
    }

    Ok(Json(CombinedApplyResponse::from(result)))
}

/// `GET /api/wiki/pages`
///
/// List every concept page under `wiki/concepts/`. Returns summaries
/// (no body text) sorted by slug ascending.
pub(crate) async fn list_wiki_pages_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let pages = wiki_store::list_wiki_pages(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_wiki_pages failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "pages": pages,
        "total_count": pages.len(),
    })))
}

/// Query parameters for `GET /api/wiki/search`. `q` is the search
/// query (required non-empty), `limit` caps result count (default
/// 20, hard max 100 so a runaway frontend can't drag down the
/// server).
#[derive(Debug, Deserialize)]
pub(crate) struct WikiSearchQuery {
    q: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

/// `GET /api/wiki/search?q=&limit=`
///
/// Substring search over all concept pages with weighted field
/// scoring. Empty/missing `q` returns an empty result set (not
/// 400) so the frontend can debounce without error flicker.
///
/// Canonical §9.3 lists this route; Karpathy llm-wiki.md
/// §"Optional CLI tools" justifies the substring-first approach
/// at MVP scale.
pub(crate) async fn search_wiki_pages_handler(
    Query(params): Query<WikiSearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let query = params.q.unwrap_or_default();
    let limit = params.limit.unwrap_or(20).min(100);

    let mut hits = wiki_store::search_wiki_pages(&paths, &query).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("search_wiki_pages failed: {e}"),
            }),
        )
    })?;

    let total_matches = hits.len();
    hits.truncate(limit);
    Ok(Json(serde_json::json!({
        "query": query,
        "hits": hits,
        "total_matches": total_matches,
        "limit": limit,
    })))
}

/// `GET /api/wiki/pages/{slug}`
///
/// Fetch a single concept page by slug. Returns the parsed summary
/// plus the body text. 404 if the slug doesn't exist or is invalid.
pub(crate) async fn get_wiki_page_handler(
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let (summary, body) = wiki_store::read_wiki_page(&paths, &slug).map_err(|e| match e {
        wiki_store::WikiStoreError::Invalid(msg) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("wiki page: {msg}"),
            }),
        ),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("read_wiki_page failed: {other}"),
            }),
        ),
    })?;
    let content = wiki_store::read_wiki_page_content(&paths, &slug).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("read_wiki_page_content failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "summary": summary,
        "body": body,
        "content": content,
    })))
}

/// `PUT /api/wiki/pages/{slug}`
///
/// Human wiki edit path. Accepts the complete markdown file, including
/// YAML frontmatter, validates the minimal required frontmatter shape,
/// preserves all additional fields, writes atomically, and logs the edit.
pub(crate) async fn put_wiki_page_handler(
    Path(slug): Path<String>,
    Json(body): Json<PutWikiPageRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let written_path = wiki_store::overwrite_wiki_page_content(&paths, &slug, &body.content)
        .map_err(|e| {
            let status = match e {
                wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("wiki page write failed: {e}"),
                }),
            )
        })?;

    if let Err(e) = wiki_store::append_wiki_log(&paths, "human-edit-wiki-page", &slug) {
        eprintln!("put_wiki_page: page written but log append failed: {e}");
    }

    let (summary, body) = wiki_store::read_wiki_page(&paths, &slug).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("read_wiki_page after write failed: {e}"),
            }),
        )
    })?;
    let content = wiki_store::read_wiki_page_content(&paths, &slug).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("read_wiki_page_content after write failed: {e}"),
            }),
        )
    })?;
    let byte_size = content.len();

    Ok(Json(serde_json::json!({
        "ok": true,
        "path": written_path.display().to_string(),
        "byte_size": byte_size,
        "summary": summary,
        "body": body,
        "content": content,
    })))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PutWikiPageRequest {
    content: String,
}

pub(crate) async fn get_vault_git_status_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let status = wiki_store::vault_git_status(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("git status failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(status).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GitDiffQuery {
    #[serde(default)]
    staged: Option<bool>,
}

pub(crate) async fn get_vault_git_diff_handler(
    Query(query): Query<GitDiffQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let diff = wiki_store::vault_git_diff(&paths, query.staged.unwrap_or(false)).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git diff failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(diff).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GitAuditQuery {
    #[serde(default)]
    limit: Option<usize>,
}

pub(crate) async fn get_vault_git_audit_handler(
    Query(query): Query<GitAuditQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let audit = wiki_store::vault_git_audit_log(&paths, query.limit.unwrap_or(10)).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("git audit read failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(audit).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GitCommitRequest {
    message: String,
}

pub(crate) async fn commit_vault_git_handler(
    Json(body): Json<GitCommitRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let result = wiki_store::vault_git_commit(&paths, &body.message).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git commit failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

pub(crate) async fn pull_vault_git_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let result = wiki_store::vault_git_pull(&paths).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git pull failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

pub(crate) async fn push_vault_git_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let result = wiki_store::vault_git_push(&paths).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git push failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GitRemoteRequest {
    remote: Option<String>,
    url: String,
}

pub(crate) async fn set_vault_git_remote_handler(
    Json(body): Json<GitRemoteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let remote = body.remote.as_deref().unwrap_or("origin");
    let result = wiki_store::vault_git_set_remote(&paths, remote, &body.url).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git remote failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GitDiscardRequest {
    path: String,
}

pub(crate) async fn discard_vault_git_path_handler(
    Json(body): Json<GitDiscardRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let result = wiki_store::vault_git_discard_path(&paths, &body.path).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git discard failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GitDiscardHunkRequest {
    path: String,
    hunk_index: usize,
    hunk_header: Option<String>,
}

pub(crate) async fn discard_vault_git_hunk_handler(
    Json(body): Json<GitDiscardHunkRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let result = wiki_store::vault_git_discard_hunk(
        &paths,
        &body.path,
        body.hunk_index,
        body.hunk_header.as_deref(),
    )
    .map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git hunk discard failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GitDiscardLineRequest {
    path: String,
    hunk_index: usize,
    line_index: usize,
    hunk_header: Option<String>,
    line_text: Option<String>,
    new_line: Option<u32>,
}

pub(crate) async fn discard_vault_git_line_handler(
    Json(body): Json<GitDiscardLineRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let result = wiki_store::vault_git_discard_added_line(
        &paths,
        &body.path,
        body.hunk_index,
        body.line_index,
        body.hunk_header.as_deref(),
        body.line_text.as_deref(),
        body.new_line,
    )
    .map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git line discard failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

pub(crate) async fn discard_vault_git_change_block_handler(
    Json(body): Json<GitDiscardLineRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let result = wiki_store::vault_git_discard_change_block(
        &paths,
        &body.path,
        body.hunk_index,
        body.line_index,
        body.hunk_header.as_deref(),
        body.line_text.as_deref(),
        body.new_line,
    )
    .map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("git change-block discard failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
    ))
}

pub(crate) async fn get_external_ai_write_policy_handler(
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let policy = wiki_store::load_external_ai_write_policy(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("external AI policy read failed: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(policy).unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ExternalAiGrantRequest {
    level: String,
    scope: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

pub(crate) async fn add_external_ai_write_grant_handler(
    Json(body): Json<ExternalAiGrantRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let grant = wiki_store::add_external_ai_write_grant(
        &paths,
        &body.level,
        &body.scope,
        body.note,
        body.expires_at,
    )
    .map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("external AI grant failed: {e}"),
            }),
        )
    })?;
    if let Err(e) = wiki_store::append_wiki_log(
        &paths,
        "external-ai-write-grant",
        &format!("{} {}", grant.level, grant.scope),
    ) {
        eprintln!("add_external_ai_write_grant: grant saved but log append failed: {e}");
    }
    Ok(Json(serde_json::json!({
        "ok": true,
        "grant": grant,
        "policy": wiki_store::load_external_ai_write_policy(&paths).ok(),
    })))
}

pub(crate) async fn revoke_external_ai_write_grant_handler(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let policy = wiki_store::revoke_external_ai_write_grant(&paths, &id).map_err(|e| {
        let status = match e {
            wiki_store::WikiStoreError::Invalid(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorResponse {
                error: format!("external AI revoke failed: {e}"),
            }),
        )
    })?;
    if let Err(e) = wiki_store::append_wiki_log(&paths, "external-ai-write-revoke", &id) {
        eprintln!("revoke_external_ai_write_grant: revoke saved but log append failed: {e}");
    }
    Ok(Json(serde_json::json!({
        "ok": true,
        "policy": policy,
    })))
}

/// `WS /ws/wechat-inbox` (canonical §9.3 · feat O)
///
/// WebSocket endpoint that sends a JSON `{"event":"inbox_changed"}`
/// message whenever the inbox is mutated (new raw entry, proposal
/// approved, conflict marked, etc.). Frontend subscribes on mount
/// and invalidates the inbox query on each message, replacing the
/// 30s polling interval with sub-second reactivity.
///
/// The WS is read-only from the client side — any incoming client
/// message is ignored. The server holds the connection open and
/// streams notifications until the client disconnects.
pub(crate) async fn ws_wechat_inbox_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    ws.on_upgrade(move |mut socket| async move {
        let mut rx = state.inbox_notify.subscribe();
        loop {
            match rx.recv().await {
                Ok(()) => {
                    let msg =
                        axum::extract::ws::Message::Text("{\"event\":\"inbox_changed\"}".into());
                    if socket.send(msg).await.is_err() {
                        break; // client disconnected
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("[ws/wechat-inbox] lagged {n} messages — sending catchup");
                    let msg = axum::extract::ws::Message::Text(
                        "{\"event\":\"inbox_changed\",\"lagged\":true}".into(),
                    );
                    if socket.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break; // sender dropped — server shutting down
                }
            }
        }
    })
}

/// Process-global inbox notification channel. Initialized once by
/// `AppState::new` / `AppState::default` via `install_inbox_notify`.
/// Handlers call `fire_inbox_notify()` without needing a `State`
/// extractor, same pattern as `codex_broker::install_global`.
static INBOX_NOTIFY: std::sync::OnceLock<tokio::sync::broadcast::Sender<()>> =
    std::sync::OnceLock::new();

pub(crate) fn install_inbox_notify(tx: tokio::sync::broadcast::Sender<()>) {
    let _ = INBOX_NOTIFY.set(tx);
}

/// Fire the inbox_notify broadcast so all WS subscribers get an
/// instant notification. Best-effort: if no subscribers exist the
/// send is silently dropped.
pub(crate) fn fire_inbox_notify() {
    if let Some(tx) = INBOX_NOTIFY.get() {
        let _ = tx.send(());
    }
}

fn wiki_page_proposal_to_json(p: &wiki_maintainer::WikiPageProposal) -> serde_json::Value {
    serde_json::json!({
        "slug": p.slug,
        "title": p.title,
        "summary": p.summary,
        "body": p.body,
        "source_raw_id": p.source_raw_id,
        "conflict_with": &p.conflict_with,
        "conflict_reason": &p.conflict_reason,
    })
}

/// `GET /api/wiki/index`
///
/// Read `wiki/index.md`. Canonical §10 + Karpathy llm-wiki.md: this
/// is the content-oriented catalog auto-maintained by the
/// `approve-with-write` handler. Returns 200 with empty content when
/// the file doesn't exist yet (a fresh wiki has never been written to).
pub(crate) async fn get_wiki_index_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let path = wiki_store::wiki_index_path(&paths);
    let content = if path.is_file() {
        tokio::fs::read_to_string(&path).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("read wiki index failed: {e}"),
                }),
            )
        })?
    } else {
        String::new()
    };
    let byte_size = content.len();
    Ok(Json(serde_json::json!({
        "path": path.display().to_string(),
        "content": content,
        "byte_size": byte_size,
        "exists": path.is_file(),
    })))
}

/// `GET /api/wiki/log`
///
/// Read `wiki/log.md`. Append-only audit trail of maintainer writes
/// and inbox resolutions. Returns 200 with empty content for a fresh
/// wiki that has never been written to.
pub(crate) async fn get_wiki_log_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let path = wiki_store::wiki_log_path(&paths);
    let content = if path.is_file() {
        tokio::fs::read_to_string(&path).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("read wiki log failed: {e}"),
                }),
            )
        })?
    } else {
        String::new()
    };
    let byte_size = content.len();
    Ok(Json(serde_json::json!({
        "path": path.display().to_string(),
        "content": content,
        "byte_size": byte_size,
        "exists": path.is_file(),
    })))
}

pub(crate) async fn delete_wiki_raw_handler(
    Path(id): Path<u32>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    wiki_store::delete_raw_entry(&paths, id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("{e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({ "ok": true, "deleted": id })))
}

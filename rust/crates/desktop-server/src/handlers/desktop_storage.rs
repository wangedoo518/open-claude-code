use super::super::*;

// Storage migration handler
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub(crate) struct MigrateStorageRequest {
    new_path: String,
}

/// `POST /api/desktop/storage/migrate`
///
/// Copies the entire wiki directory tree from the current location to
/// `new_path`, then writes a `.clawwiki-redirect` marker so the next
/// startup can auto-detect the new location.
///
/// This is a best-effort copy — if the target already exists and is
/// non-empty, the handler returns 409 Conflict.
pub(crate) async fn migrate_storage_handler(
    Json(body): Json<MigrateStorageRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let new_path = body.new_path.trim().to_string();
    if new_path.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "new_path must not be empty".to_string(),
            }),
        ));
    }

    let current_root = wiki_store::default_root();
    let target = std::path::PathBuf::from(&new_path);

    // Don't overwrite an existing non-empty directory
    if target.exists()
        && target
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: format!("目标目录 {} 已存在且非空", new_path),
            }),
        ));
    }

    // Recursive copy
    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<u64> {
        let mut count = 0u64;
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if ft.is_dir() {
                count += copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
                count += 1;
            }
        }
        Ok(count)
    }

    let file_count = copy_dir_recursive(&current_root, &target).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("迁移失败: {e}"),
            }),
        )
    })?;

    // Write a redirect marker in the OLD location so future code can
    // detect the move (optional — main mechanism is CLAWWIKI_HOME env).
    let marker = current_root.join(".clawwiki-redirect");
    let _ = std::fs::write(&marker, &new_path);

    Ok(Json(serde_json::json!({
        "ok": true,
        "files_copied": file_count,
        "old_path": current_root.to_string_lossy(),
        "new_path": new_path,
    })))
}

// ═══════════════════════════════════════════════════════════════
// MarkItDown handlers
// ═══════════════════════════════════════════════════════════════

/// `GET /api/desktop/markitdown/check`
///
/// Check if Python + markitdown are available on this machine.
pub(crate) async fn markitdown_check_handler() -> Json<serde_json::Value> {
    match wiki_ingest::markitdown::check_environment().await {
        Ok(version) => Json(serde_json::json!({
            "available": true,
            "version": version,
            "supported_formats": wiki_ingest::markitdown::supported_extensions(),
        })),
        Err(error) => Json(serde_json::json!({
            "available": false,
            "error": error,
        })),
    }
}

#[derive(Deserialize)]
pub(crate) struct MarkItDownConvertRequest {
    /// Absolute path to the file to convert.
    path: String,
    /// If true, also ingest the result into Raw Library.
    #[serde(default)]
    ingest: bool,
}

/// `POST /api/desktop/markitdown/convert`
///
/// Convert a local file to Markdown using MarkItDown.
/// Optionally ingests the result into Raw Library.
pub(crate) async fn markitdown_convert_handler(
    Json(body): Json<MarkItDownConvertRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = std::path::PathBuf::from(&body.path);

    let result = wiki_ingest::markitdown::extract_via_markitdown(&path)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("{e}"),
                }),
            )
        })?;

    // Optionally ingest into Raw Library
    let raw_id = if body.ingest {
        let paths = resolve_wiki_root_for_handler()?;
        let frontmatter =
            wiki_store::RawFrontmatter::for_paste(&result.source, result.source_url.clone());
        match wiki_store::write_raw_entry(
            &paths,
            &result.source,
            &result.title,
            &result.body,
            &frontmatter,
        ) {
            Ok(entry) => Some(entry.id),
            Err(e) => {
                eprintln!("[markitdown] ingest failed: {e}");
                None
            }
        }
    } else {
        None
    };

    Ok(Json(serde_json::json!({
        "ok": true,
        "title": result.title,
        "markdown": result.body,
        "source": result.source,
        "raw_id": raw_id,
    })))
}

// ═══════════════════════════════════════════════════════════════
// WeChat article fetch handler
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub(crate) struct WechatFetchRequest {
    url: String,
    #[serde(default = "default_true")]
    ingest: bool,
    /// M3: when `true`, bypass canonical-URL dedupe and write a
    /// fresh raw entry even if the URL was ingested before. Used by
    /// the Raw Library's "re-ingest" button and future admin tools.
    /// Defaults to `false` so the common fetch path keeps its M3
    /// dedupe behavior.
    #[serde(default)]
    force: bool,
}
fn default_true() -> bool {
    true
}

/// `POST /api/desktop/wechat-fetch`
///
/// Fetch a WeChat article using Playwright and optionally ingest it.
///
/// M2: core logic now funnels through
/// `desktop_core::url_ingest::ingest_url` when `ingest=true`, so the
/// write + inbox queue + dedupe all match the shared orchestrator
/// semantics. When `ingest=false` we preserve the old "fetch, validate,
/// return markdown" contract by calling the Playwright adapter
/// directly — the orchestrator intentionally doesn't have a "fetch
/// only" mode because every other caller always wants to persist.
pub(crate) async fn wechat_fetch_handler(
    Json(body): Json<WechatFetchRequest>,
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

    // `ingest=false` path: one-shot preview, no persistence. Kept
    // outside the orchestrator because orchestrator always writes.
    if !body.ingest {
        let result = wiki_ingest::wechat_fetch::fetch_wechat_article(url)
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("{e}"),
                    }),
                )
            })?;

        if let Err(reason) = wiki_ingest::validate_fetched_content(&result.body) {
            eprintln!("[wechat-fetch] rejected by quality check: {reason}");
            return Ok(Json(serde_json::json!({
                "ok": false,
                "error": reason,
                "title": result.title,
            })));
        }

        return Ok(Json(serde_json::json!({
            "ok": true,
            "title": result.title,
            "markdown": result.body,
            "source": result.source,
            "raw_id": serde_json::Value::Null,
        })));
    }

    // `ingest=true` (default) → funnel through the orchestrator so
    // the write + inbox + dedupe + prerequisite detection all behave
    // identically to every other URL ingest site.
    let outcome = desktop_core::url_ingest::ingest_url(desktop_core::url_ingest::IngestRequest {
        url,
        origin_tag: "wechat-fetch".into(),
        prefer_playwright: Some(true),
        fetch_timeout: std::time::Duration::from_secs(60),
        allow_text_fallback: None,
        force: body.force,
    })
    .await;
    eprintln!("[wechat-fetch] outcome: {}", outcome.as_display());

    match outcome {
        desktop_core::url_ingest::IngestOutcome::Ingested {
            entry,
            title,
            body,
            decision,
            ..
        } => Ok(Json(serde_json::json!({
            "ok": true,
            "title": title,
            "markdown": body,
            "source": entry.source,
            "raw_id": entry.id,
            "decision": decision.tag(),
        }))),
        desktop_core::url_ingest::IngestOutcome::IngestedInboxSuppressed {
            entry,
            existing_inbox,
        } => Ok(Json(serde_json::json!({
            "ok": true,
            "title": String::new(),
            "markdown": String::new(),
            "source": entry.source,
            "raw_id": entry.id,
            "inbox_id": existing_inbox.id,
            "dedupe": true,
        }))),
        desktop_core::url_ingest::IngestOutcome::ReusedExisting {
            entry,
            decision,
            existing_inbox,
        } => Ok(Json(serde_json::json!({
            "ok": true,
            "title": entry.slug,
            "markdown": String::new(),
            "source": entry.source,
            "raw_id": entry.id,
            "inbox_id": existing_inbox.as_ref().map(|i| i.id),
            "dedupe": true,
            "decision": decision.tag(),
            "reason": decision.reason(),
        }))),
        desktop_core::url_ingest::IngestOutcome::RejectedQuality { reason } => {
            Ok(Json(serde_json::json!({
                "ok": false,
                "error": reason,
            })))
        }
        desktop_core::url_ingest::IngestOutcome::PrerequisiteMissing { dep, hint } => {
            Ok(Json(serde_json::json!({
                "ok": false,
                "error": hint,
                "missing_prerequisite": dep,
            })))
        }
        desktop_core::url_ingest::IngestOutcome::FetchFailed { error } => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("{error}"),
            }),
        )),
        desktop_core::url_ingest::IngestOutcome::InvalidUrl { reason } => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: reason }),
        )),
        desktop_core::url_ingest::IngestOutcome::FallbackToText { .. } => {
            // `wechat-fetch` never opts into text fallback, so this
            // variant should be unreachable. Treat as a 500 since it
            // indicates a logic error in the orchestrator or this
            // handler.
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "unexpected FallbackToText without fallback request".to_string(),
                }),
            ))
        }
    }
}

/// `GET /api/desktop/wechat-fetch/check`
///
/// Report whether the Playwright-based WeChat fetcher is available
/// on this machine. Mirrors `markitdown_check_handler` so the
/// Environment Doctor panel can render a uniform status row for
/// either sidecar. Delegates to
/// `wiki_ingest::wechat_fetch::check_environment`, which already
/// knows how to distinguish "Python missing" from "Playwright not
/// installed".
pub(crate) async fn wechat_fetch_check_handler() -> Json<serde_json::Value> {
    match wiki_ingest::wechat_fetch::check_environment().await {
        Ok(message) => Json(serde_json::json!({
            "available": true,
            "message": message,
        })),
        Err(error) => Json(serde_json::json!({
            "available": false,
            "error": error,
        })),
    }
}

// ═══════════════════════════════════════════════════════════════
// URL ingest observability (M3 Worker B)
// ═══════════════════════════════════════════════════════════════
//
// Backed by `desktop_core::url_ingest::recent`, an in-memory ring
// buffer populated by the orchestrator after every terminal outcome.
// Read-only endpoint — the buffer clears on restart by design (it is
// diagnostics, not persistence).

#[derive(Deserialize)]
pub(crate) struct RecentIngestQuery {
    /// Cap on rows returned (newest-first). Defaults to the buffer
    /// capacity when omitted.
    #[serde(default)]
    limit: Option<usize>,
    /// Optional substring filter against `entry_point`. Matches via
    /// `str::contains` so `ep=ilink` catches `"ilink"`, `"wechat-ilink"`,
    /// etc.
    #[serde(default)]
    entry_point: Option<String>,
    /// Only return decisions at or after this epoch-millis timestamp.
    #[serde(default)]
    since_ms: Option<u64>,
    /// M4: filter by `decision.kind` (the serde tag of
    /// `IngestDecision`, e.g. `"created_new"`, `"reused_with_pending_inbox"`,
    /// `"explicit_reingest"`, `"content_duplicate"`, `"refreshed_content"`).
    /// When a decision row has no structured decision payload
    /// (e.g. `fetch_failed`, `invalid_url`), this also matches
    /// `outcome_kind` as a fallback so the diagnostics panel can
    /// filter terminal errors the same way.
    #[serde(default)]
    decision_kind: Option<String>,
}

/// `GET /api/desktop/url-ingest/recent`
///
/// Newest-first snapshot of recent URL ingest decisions. Supports
/// `?limit=N`, `?entry_point=substr`, `?since_ms=epoch_ms`, and
/// (M4) `?decision_kind=kind` for filtering. Response shape:
///
/// ```json
/// {
///   "decisions": [ RecentIngestEntry, ... ],
///   "total":     <filtered count>,
///   "capacity":  <ring buffer capacity>,
///   "stats": {
///     "by_kind":        { "<decision-kind-or-outcome>": <count>, ... },
///     "by_entry_point": { "<entry-point>": <count>, ... }
///   }
/// }
/// ```
///
/// The `stats` object is computed against the *filtered* set so the
/// frontend can render decision-distribution histograms without a
/// second round-trip. Counts aggregate on `decision.kind` when present
/// and fall back to `outcome_kind` (e.g. `"fetch_failed"`) otherwise —
/// the same rule used by the `decision_kind` filter so a chart click
/// round-trips cleanly into a drill-down query.
pub(crate) async fn recent_ingest_handler(
    Query(params): Query<RecentIngestQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Take a snapshot first, then filter — the mutex is released before
    // any string comparison runs, so concurrent `push`es from the
    // orchestrator never block on a slow query.
    let snap = desktop_core::url_ingest::recent::snapshot(params.limit);

    let filtered: Vec<_> = snap
        .into_iter()
        .filter(|e| {
            if let Some(ep) = &params.entry_point {
                if !e.entry_point.contains(ep) {
                    return false;
                }
            }
            if let Some(since) = params.since_ms {
                if e.timestamp_ms < since {
                    return false;
                }
            }
            if let Some(dk) = &params.decision_kind {
                // Match against `decision.kind` first (structured payload),
                // fall back to `outcome_kind` so failure variants without
                // a decision (fetch_failed, invalid_url, etc.) remain
                // filterable by the same query parameter.
                let from_decision = e
                    .decision
                    .as_ref()
                    .and_then(|v| v.get("kind"))
                    .and_then(|k| k.as_str())
                    .map(|k| k == dk.as_str())
                    .unwrap_or(false);
                let from_outcome = e.outcome_kind == *dk;
                if !(from_decision || from_outcome) {
                    return false;
                }
            }
            true
        })
        .collect();

    // M4: aggregate stats by decision.kind (with outcome_kind fallback)
    // and by entry_point. BTreeMap keeps the JSON key order stable so
    // the frontend chart doesn't flicker between requests with identical
    // data.
    let mut stats_by_kind: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut stats_by_entry: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for e in &filtered {
        let kind_key = e
            .decision
            .as_ref()
            .and_then(|v| v.get("kind"))
            .and_then(|k| k.as_str())
            .unwrap_or(e.outcome_kind.as_str())
            .to_string();
        *stats_by_kind.entry(kind_key).or_insert(0) += 1;
        *stats_by_entry.entry(e.entry_point.clone()).or_insert(0) += 1;
    }

    let total = filtered.len();
    Ok(Json(serde_json::json!({
        "decisions": filtered,
        "total": total,
        "capacity": desktop_core::url_ingest::recent::RECENT_LOG_CAPACITY,
        "stats": {
            "by_kind": stats_by_kind,
            "by_entry_point": stats_by_entry,
        },
    })))
}

// ═══════════════════════════════════════════════════════════════
// Environment Doctor prerequisite probes (M2.1 Worker A Task A-3)
// ═══════════════════════════════════════════════════════════════
//
// These endpoints mirror `markitdown_check_handler` /
// `wechat_fetch_check_handler` so the frontend doctor panel can
// render every row with the uniform `{available, message?, error?}`
// shape. Each probe blocks on a tiny subprocess spawn via
// `spawn_blocking` (the underlying `deployer::check_prerequisites`
// uses sync `std::process::Command`) so we don't stall the tokio
// reactor.

/// `GET /api/desktop/node/check`
///
/// Report whether Node.js + `npx` are available. Delegates to
/// `wechat_kefu::deployer::WranglerDeployer::check_prerequisites`,
/// the same probe the one-scan pipeline runs before attempting to
/// deploy the Cloudflare Worker relay. Treats "either missing" as
/// unavailable so the frontend shows a single install CTA.
pub(crate) async fn node_check_handler() -> Json<serde_json::Value> {
    // `check_prerequisites` is a sync function that shells out twice;
    // hand it to a blocking pool so tokio keeps spinning.
    let status = tokio::task::spawn_blocking(
        desktop_core::wechat_kefu::deployer::WranglerDeployer::check_prerequisites,
    )
    .await;

    let status = match status {
        Ok(s) => s,
        Err(e) => {
            return Json(serde_json::json!({
                "available": false,
                "error": format!("node check join failed: {e}"),
            }));
        }
    };

    if status.node_ok && status.npx_ok {
        Json(serde_json::json!({
            "available": true,
            "message": status
                .node_version
                .unwrap_or_else(|| "node available".to_string()),
        }))
    } else if !status.node_ok {
        Json(serde_json::json!({
            "available": false,
            "error": "Node.js not found. Install from https://nodejs.org or via your package manager.",
        }))
    } else {
        Json(serde_json::json!({
            "available": false,
            "error": "npx not found. Reinstall Node.js to ensure npx is on PATH.",
        }))
    }
}

/// `GET /api/desktop/opencli/check`
///
/// Report whether OpenCLI (`@jackwener/opencli`) is reachable either
/// as a global binary or via `npx --yes @jackwener/opencli`. Mirrors
/// the version probe in `KefuPipeline::resolve_opencli_command`
/// (inlined here so this endpoint can call it without constructing a
/// full pipeline instance + cancellation token).
pub(crate) async fn opencli_check_handler() -> Json<serde_json::Value> {
    let result = tokio::task::spawn_blocking(|| -> Result<String, String> {
        // Try global `opencli` first; if it's on PATH we prefer it
        // because `npx --yes` can spend a few seconds resolving.
        let direct = std::process::Command::new("opencli")
            .arg("--version")
            .output();
        if let Ok(output) = direct {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string();
                return Ok(format!("opencli (global) {version}"));
            }
        }

        let npx = desktop_core::wechat_kefu::deployer::run_node_tool(
            "npx",
            &["--yes", "@jackwener/opencli", "--version"],
        );
        match npx {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string();
                Ok(format!("opencli (npx) {version}"))
            }
            Ok(output) => {
                let npx_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let npm_exec = desktop_core::wechat_kefu::deployer::run_node_tool(
                    "npm",
                    &[
                        "exec",
                        "--yes",
                        "--package",
                        "@jackwener/opencli",
                        "opencli",
                        "--",
                        "--version",
                    ],
                );
                match npm_exec {
                    Ok(output) if output.status.success() => {
                        let version = String::from_utf8_lossy(&output.stdout)
                            .trim()
                            .to_string();
                        Ok(format!("opencli (npm exec) {version}"))
                    }
                    Ok(output) => Err(format!(
                        "opencli probe failed via npx ({npx_error}) and npm exec ({}).",
                        String::from_utf8_lossy(&output.stderr).trim(),
                    )),
                    Err(e) => Err(format!(
                        "opencli probe failed via npx ({npx_error}) and npm exec ({e}). Install it globally (`npm i -g @jackwener/opencli`) or ensure Node package runners are on PATH."
                    )),
                }
            }
            Err(e) => {
                let npm_exec = desktop_core::wechat_kefu::deployer::run_node_tool(
                    "npm",
                    &[
                        "exec",
                        "--yes",
                        "--package",
                        "@jackwener/opencli",
                        "opencli",
                        "--",
                        "--version",
                    ],
                );
                match npm_exec {
                    Ok(output) if output.status.success() => {
                        let version = String::from_utf8_lossy(&output.stdout)
                            .trim()
                            .to_string();
                        Ok(format!("opencli (npm exec) {version}"))
                    }
                    Ok(output) => Err(format!(
                        "opencli not reachable via npx ({e}) and npm exec ({}). Install it globally (`npm i -g @jackwener/opencli`) or ensure Node package runners are on PATH.",
                        String::from_utf8_lossy(&output.stderr).trim(),
                    )),
                    Err(npm_err) => Err(format!(
                        "opencli not reachable. Install it globally (`npm i -g @jackwener/opencli`) or ensure `npx` / `npm exec` is on PATH: npx={e}; npm={npm_err}"
                    )),
                }
            }
        }
    })
    .await;

    match result {
        Ok(Ok(message)) => Json(serde_json::json!({
            "available": true,
            "message": message,
        })),
        Ok(Err(error)) => Json(serde_json::json!({
            "available": false,
            "error": error,
        })),
        Err(e) => Json(serde_json::json!({
            "available": false,
            "error": format!("opencli check join failed: {e}"),
        })),
    }
}

/// `GET /api/desktop/chromium/check`
///
/// Report whether Playwright + its bundled Chromium driver are
/// importable. A green result here implies Chromium is reachable —
/// Playwright's sync import exercises the browser binary path
/// internally. Reuses `wiki_ingest::wechat_fetch::check_environment`
/// instead of rolling a second Python probe; the surface keeps the
/// same `{available, message? | error?}` shape every other doctor
/// row uses.
pub(crate) async fn chromium_check_handler() -> Json<serde_json::Value> {
    match wiki_ingest::wechat_fetch::check_environment().await {
        Ok(message) => Json(serde_json::json!({
            "available": true,
            "message": format!("Chromium reachable via Playwright: {message}"),
        })),
        Err(error) => Json(serde_json::json!({
            "available": false,
            "error": error,
        })),
    }
}

// ═══════════════════════════════════════════════════════════════
// Python dependency auto-installer
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub(crate) struct InstallDepsRequest {
    #[serde(default = "default_pkg_all")]
    package: String,
}
fn default_pkg_all() -> String {
    "all".to_string()
}

pub(crate) async fn install_python_deps_handler(
    Json(body): Json<InstallDepsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut steps: Vec<serde_json::Value> = Vec::new();
    let mut all_ok = true;

    let py = tokio::process::Command::new("python")
        .args(["--version"])
        .output()
        .await;
    match py {
        Ok(o) if o.status.success() => {
            steps.push(serde_json::json!({"step":"python","ok":true,"output":String::from_utf8_lossy(&o.stdout).trim().to_string()}));
        }
        _ => {
            steps.push(serde_json::json!({"step":"python","ok":false,"output":"Python not found"}));
            return Ok(Json(serde_json::json!({"ok":false,"steps":steps})));
        }
    }

    if body.package == "markitdown" || body.package == "all" {
        let o = tokio::process::Command::new("python")
            .args(["-m", "pip", "install", "--upgrade", "markitdown[all]"])
            .output()
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("{e}"),
                    }),
                )
            })?;
        let ok = o.status.success();
        if !ok {
            all_ok = false;
        }
        steps.push(serde_json::json!({"step":"markitdown","ok":ok,"output":format!("{}\n{}",String::from_utf8_lossy(&o.stdout),String::from_utf8_lossy(&o.stderr)).trim().to_string()}));
    }

    if body.package == "playwright" || body.package == "all" {
        let o1 = tokio::process::Command::new("python")
            .args(["-m", "pip", "install", "--upgrade", "playwright"])
            .output()
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("{e}"),
                    }),
                )
            })?;
        let o2 = tokio::process::Command::new("python")
            .args(["-m", "playwright", "install", "chromium"])
            .output()
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("{e}"),
                    }),
                )
            })?;
        let ok = o1.status.success() && o2.status.success();
        if !ok {
            all_ok = false;
        }
        steps.push(serde_json::json!({"step":"playwright","ok":ok}));
    }

    Ok(Json(serde_json::json!({"ok":all_ok,"steps":steps})))
}

use super::super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct CleanupQuery {
    /// When true (default), persist the patrol report and create idempotent
    /// Inbox tasks. `apply=false` gives the frontend a dry-run proposal view.
    #[serde(default)]
    apply: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CleanupProposal {
    issue_kind: String,
    page_slug: String,
    title: String,
    description: String,
    suggested_action: String,
    inbox_action: String,
}

/// POST /api/wiki/cleanup - patrol-backed cleanup proposal flow.
///
/// Backwards-compatible response: the patrol report still sits at the top
/// level (`issues`, `summary`, `checked_at`) while Phase 4 adds
/// `cleanup_proposals`, `inbox_created`, and `applied`.
pub(crate) async fn cleanup_handler(
    Query(params): Query<CleanupQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let config = wiki_patrol::PatrolConfig {
        stale_threshold_days: 30,
        min_page_words: 50,
        max_page_words: 3000,
    };
    let report = wiki_patrol::run_full_patrol(&paths, &config);
    let proposals = cleanup_proposals_for_report(&paths, &report);
    let apply = params.apply.unwrap_or(true);
    let inbox_created = if apply {
        persist_patrol_outputs(&paths, &report)?
    } else {
        0
    };
    let mut value = serde_json::to_value(&report).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "cleanup_proposals".to_string(),
            serde_json::to_value(proposals).unwrap_or_else(|_| serde_json::json!([])),
        );
        obj.insert(
            "inbox_created".to_string(),
            serde_json::json!(inbox_created),
        );
        obj.insert("applied".to_string(), serde_json::json!(apply));
    }
    Ok(Json(value))
}

/// POST /api/wiki/patrol - run full patrol and return report.
pub(crate) async fn patrol_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let config = wiki_patrol::PatrolConfig::default();
    let report = wiki_patrol::run_full_patrol(&paths, &config);
    persist_patrol_outputs(&paths, &report)?;
    Ok(Json(
        serde_json::to_value(&report).unwrap_or(serde_json::Value::Null),
    ))
}

fn persist_patrol_outputs(
    paths: &wiki_store::WikiPaths,
    report: &wiki_store::PatrolReport,
) -> Result<usize, ApiError> {
    wiki_store::save_patrol_report(paths, report).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("PATROL_REPORT_SAVE_FAILED: {e}"),
            }),
        )
    })?;
    let created =
        wiki_store::append_patrol_issue_inbox_tasks(paths, &report.issues).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("PATROL_INBOX_TASK_CREATE_FAILED: {e}"),
                }),
            )
        })?;
    Ok(created)
}

fn cleanup_proposals_for_report(
    paths: &wiki_store::WikiPaths,
    report: &wiki_store::PatrolReport,
) -> Vec<CleanupProposal> {
    report
        .issues
        .iter()
        .map(|issue| {
            let title = wiki_store::read_wiki_page(paths, &issue.page_slug)
                .map(|(summary, _)| summary.title)
                .unwrap_or_else(|_| issue.page_slug.clone());
            CleanupProposal {
                issue_kind: format!("{:?}", issue.kind),
                page_slug: issue.page_slug.clone(),
                title,
                description: issue.description.clone(),
                suggested_action: issue.suggested_action.clone(),
                inbox_action: "create_review_task".to_string(),
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub(crate) struct BreakdownRequest {
    slug: String,
    #[serde(default)]
    apply: Option<bool>,
    #[serde(default)]
    max_targets: Option<usize>,
}

#[derive(Debug, Serialize, Clone)]
struct BreakdownTarget {
    slug: String,
    title: String,
    summary: String,
    body: String,
    word_count: usize,
}

#[derive(Debug, Serialize)]
struct BreakdownResponse {
    source_slug: String,
    source_title: String,
    source_word_count: usize,
    reason: String,
    targets: Vec<BreakdownTarget>,
    applied: bool,
    written_paths: Vec<String>,
}

/// POST /api/wiki/breakdown - preview/apply deterministic page split.
///
/// This is intentionally local for Phase 4: it splits by `##` headings when
/// possible and otherwise chunks paragraphs. It never deletes or mutates the
/// source page; applying writes new concept pages and refreshes index/log.
pub(crate) async fn breakdown_handler(
    Json(body): Json<BreakdownRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let slug = body.slug.trim();
    if slug.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "slug must not be empty".to_string(),
            }),
        ));
    }
    let (summary, page_body) = wiki_store::read_wiki_page(&paths, slug).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("wiki page not found: {e}"),
            }),
        )
    })?;
    let source_word_count = count_words_for_breakdown(&page_body);
    let max_targets = body.max_targets.unwrap_or(6).clamp(2, 12);
    let targets = propose_breakdown_targets(&summary, &page_body, max_targets);
    let reason = if targets.is_empty() {
        "page has no clear split points".to_string()
    } else if source_word_count > 3000 {
        format!("source page is oversized ({source_word_count} words)")
    } else {
        "source page has multiple maintainable sections".to_string()
    };

    let mut written_paths = Vec::new();
    let apply = body.apply.unwrap_or(false);
    if apply {
        for target in &targets {
            let path = wiki_store::write_wiki_page(
                &paths,
                &target.slug,
                &target.title,
                &target.summary,
                &target.body,
                summary.source_raw_id,
            )
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("write breakdown target failed: {e}"),
                    }),
                )
            })?;
            written_paths.push(path.display().to_string());
            let _ = wiki_store::append_wiki_log(&paths, "breakdown-write", &target.title);
        }
        let _ = wiki_store::rebuild_wiki_index(&paths);
    }

    let response = BreakdownResponse {
        source_slug: summary.slug,
        source_title: summary.title,
        source_word_count,
        reason,
        targets,
        applied: apply,
        written_paths,
    };
    Ok(Json(
        serde_json::to_value(response).unwrap_or(serde_json::Value::Null),
    ))
}

fn propose_breakdown_targets(
    source: &wiki_store::WikiPageSummary,
    body: &str,
    max_targets: usize,
) -> Vec<BreakdownTarget> {
    let sections = split_by_h2(body);
    let chunks = if sections.len() >= 2 {
        sections
    } else {
        split_by_paragraph_chunks(body, 700)
    };

    chunks
        .into_iter()
        .filter(|section| count_words_for_breakdown(&section.body) >= 20)
        .take(max_targets)
        .enumerate()
        .map(|(index, section)| {
            let title = if section.title.trim().is_empty() {
                format!("{} part {}", source.title, index + 1)
            } else {
                section.title
            };
            let slug_tail = wiki_store::slugify(&title);
            let slug = wiki_store::slugify(&format!("{} {} {}", source.slug, index + 1, slug_tail));
            let body = format!(
                "> Split from [{}](concepts/{}.md).\n\n{}",
                source.title, source.slug, section.body
            );
            let word_count = count_words_for_breakdown(&body);
            BreakdownTarget {
                slug,
                title,
                summary: format!("Split from {}.", source.title),
                body,
                word_count,
            }
        })
        .collect()
}

#[derive(Debug)]
struct BreakdownSection {
    title: String,
    body: String,
}

fn split_by_h2(body: &str) -> Vec<BreakdownSection> {
    let mut sections = Vec::new();
    let mut current_title = String::new();
    let mut current_lines: Vec<String> = Vec::new();

    for line in body.lines() {
        if line.starts_with("## ") && !line.starts_with("### ") {
            if !current_lines.is_empty() {
                sections.push(BreakdownSection {
                    title: current_title.clone(),
                    body: current_lines.join("\n").trim().to_string(),
                });
                current_lines.clear();
            }
            current_title = line.trim_start_matches('#').trim().to_string();
            current_lines.push(line.to_string());
        } else {
            current_lines.push(line.to_string());
        }
    }
    if !current_lines.is_empty() {
        sections.push(BreakdownSection {
            title: current_title,
            body: current_lines.join("\n").trim().to_string(),
        });
    }
    sections
}

fn split_by_paragraph_chunks(body: &str, target_words: usize) -> Vec<BreakdownSection> {
    let mut sections = Vec::new();
    let mut current = Vec::new();
    let mut count = 0usize;
    for paragraph in body.split("\n\n") {
        let words = count_words_for_breakdown(paragraph);
        current.push(paragraph.trim().to_string());
        count += words;
        if count >= target_words {
            let index = sections.len() + 1;
            sections.push(BreakdownSection {
                title: format!("Part {index}"),
                body: current.join("\n\n"),
            });
            current.clear();
            count = 0;
        }
    }
    if !current.is_empty() {
        let index = sections.len() + 1;
        sections.push(BreakdownSection {
            title: format!("Part {index}"),
            body: current.join("\n\n"),
        });
    }
    sections
}

fn count_words_for_breakdown(body: &str) -> usize {
    let ascii = body
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .count();
    let cjk = body.chars().filter(|ch| (*ch as u32) > 0x2e7f).count();
    ascii + cjk
}

#[derive(Deserialize)]
pub(crate) struct AbsorbLogQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

/// GET /api/wiki/absorb-log - paginated absorb log.
pub(crate) async fn get_absorb_log_handler(
    Query(params): Query<AbsorbLogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let all = wiki_store::list_absorb_log(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("LOG_READ_FAILED: {e}"),
            }),
        )
    })?;

    let total = all.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).min(1000).max(1);
    let entries: Vec<_> = all.into_iter().skip(offset).take(limit).collect();

    Ok(Json(serde_json::json!({
        "entries": entries,
        "total": total,
    })))
}

#[derive(Deserialize)]
pub(crate) struct BacklinksQuery {
    slug: Option<String>,
    format: Option<String>,
}

/// GET /api/wiki/backlinks - full backlinks index or single slug.
pub(crate) async fn get_backlinks_index_handler(
    Query(params): Query<BacklinksQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;

    let mut index = wiki_store::load_backlinks_index(&paths).unwrap_or_default();
    if index.is_empty() {
        index = wiki_store::build_backlinks_index(&paths).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("INDEX_BUILD_FAILED: {e}"),
                }),
            )
        })?;
        let _ = wiki_store::save_backlinks_index(&paths, &index);
    }

    match params.slug {
        Some(slug) => {
            let backlinks = index.get(&slug).cloned().unwrap_or_default();
            let enriched: Vec<serde_json::Value> = backlinks
                .iter()
                .filter_map(|s| {
                    wiki_store::read_wiki_page(&paths, s)
                        .ok()
                        .map(|(summary, _)| {
                            serde_json::json!({
                                "slug": s,
                                "title": summary.title,
                                "category": summary.category,
                            })
                        })
                })
                .collect();
            Ok(Json(serde_json::json!({
                "slug": slug,
                "backlinks": enriched,
                "count": enriched.len(),
            })))
        }
        None => {
            if params.format.as_deref() == Some("raw") {
                return Ok(Json(
                    serde_json::to_value(&index).unwrap_or(serde_json::json!({})),
                ));
            }
            let total_pages = wiki_store::list_all_wiki_pages(&paths)
                .map(|p| p.len())
                .unwrap_or(0);
            let total_backlinks: usize = index.values().map(|v| v.len()).sum();
            Ok(Json(serde_json::json!({
                "index": index,
                "total_pages": total_pages,
                "total_backlinks": total_backlinks,
            })))
        }
    }
}

/// GET /api/wiki/stats - aggregated wiki statistics.
pub(crate) async fn get_stats_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let stats = wiki_store::wiki_stats(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("STATS_COMPUTE_FAILED: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null),
    ))
}

/// GET /api/wiki/patrol/report - latest persisted patrol report.
pub(crate) async fn get_patrol_report_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let report = wiki_store::load_patrol_report(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("load patrol report: {e}"),
            }),
        )
    })?;
    match report {
        Some(r) => Ok(Json(
            serde_json::to_value(&r).unwrap_or(serde_json::Value::Null),
        )),
        None => Ok(Json(serde_json::Value::Null)),
    }
}

/// GET /api/wiki/schema/templates - list all schema templates.
pub(crate) async fn get_schema_templates_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let infos = wiki_store::load_schema_template_infos(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("TEMPLATE_PARSE_FAILED: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(&infos).unwrap_or(serde_json::json!([])),
    ))
}

/// GET /api/wiki/guidance - root/schema guidance file status for Rules Studio.
pub(crate) async fn get_guidance_files_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let infos = wiki_store::load_guidance_file_infos(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("GUIDANCE_STATUS_FAILED: {e}"),
            }),
        )
    })?;
    Ok(Json(
        serde_json::to_value(&infos).unwrap_or_else(|_| serde_json::json!({ "files": [] })),
    ))
}

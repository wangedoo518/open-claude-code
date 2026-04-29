use super::super::*;

#[derive(Deserialize)]
pub(crate) struct AbsorbRequest {
    entry_ids: Option<Vec<u32>>,
    date_range: Option<AbsorbDateRange>,
}

#[derive(Deserialize)]
pub(crate) struct AbsorbDateRange {
    from: String,
    to: String,
}

/// POST /api/wiki/absorb - trigger batch absorb as a background task.
pub(crate) async fn absorb_handler(
    State(state): State<AppState>,
    Json(body): Json<AbsorbRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let paths = resolve_wiki_root_for_handler()?;

    if let Some(range) = &body.date_range {
        if !is_valid_iso_date(&range.from) || !is_valid_iso_date(&range.to) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_DATE_RANGE".to_string(),
                }),
            ));
        }
        if range.from.as_str() > range.to.as_str() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_DATE_RANGE".to_string(),
                }),
            ));
        }
    }

    let raws = wiki_store::list_raw_entries(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list_raw_entries failed: {e}"),
            }),
        )
    })?;

    let entry_ids: Vec<u32> = if let Some(ids) = body.entry_ids {
        let existing: std::collections::HashSet<u32> = raws.iter().map(|r| r.id).collect();
        let missing: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|id| !existing.contains(id))
            .collect();
        if !missing.is_empty() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("ENTRIES_NOT_FOUND: {missing:?}"),
                }),
            ));
        }
        ids
    } else if let Some(range) = &body.date_range {
        raws.iter()
            .filter(|r| {
                r.date.as_str() >= range.from.as_str() && r.date.as_str() <= range.to.as_str()
            })
            .filter(|r| !wiki_store::is_entry_absorbed(&paths, r.id))
            .map(|r| r.id)
            .collect()
    } else {
        raws.iter()
            .filter(|r| !wiki_store::is_entry_absorbed(&paths, r.id))
            .map(|r| r.id)
            .collect()
    };

    let total = entry_ids.len();
    if total > 0 {
        desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global()
            .runtime_auth_health()
            .map_err(|e| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ErrorResponse {
                        error: format!("BROKER_UNAVAILABLE: {e}"),
                    }),
                )
            })?;
    }

    let adapter = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global();
    let desktop_state = state.desktop().clone();
    let (task_id, cancel_token) = match desktop_state.task_manager().register("absorb").await {
        Ok(pair) => pair,
        Err(_) => {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "ABSORB_IN_PROGRESS".to_string(),
                }),
            ));
        }
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<wiki_maintainer::AbsorbProgressEvent>(64);
    let bridge_state = desktop_state.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            bridge_state
                .broadcast_session_event(desktop_core::DesktopSessionEvent::AbsorbProgress(ev))
                .await;
        }
    });

    let task_id_for_batch = task_id.clone();
    let task_id_for_complete = task_id.clone();
    let task_id_for_response = task_id.clone();
    let complete_state = desktop_state.clone();
    let paths_for_spawn = paths.clone();
    tokio::spawn(async move {
        let result = wiki_maintainer::absorb_batch(
            &paths_for_spawn,
            entry_ids,
            &adapter,
            tx,
            task_id_for_batch,
            cancel_token,
        )
        .await;

        match result {
            Ok(r) => {
                complete_state
                    .broadcast_session_event(desktop_core::DesktopSessionEvent::AbsorbComplete {
                        task_id: task_id_for_complete.clone(),
                        created: r.created,
                        updated: r.updated,
                        skipped: r.skipped,
                        failed: r.failed,
                        duration_ms: r.duration_ms,
                    })
                    .await;
                log::info!(
                    "[absorb] task {} completed (created={} updated={} skipped={} failed={} ms={})",
                    task_id_for_complete,
                    r.created,
                    r.updated,
                    r.skipped,
                    r.failed,
                    r.duration_ms,
                );
            }
            Err(e) => {
                log::warn!("[absorb] task {} errored: {e}", task_id_for_complete);
                complete_state
                    .broadcast_session_event(desktop_core::DesktopSessionEvent::AbsorbComplete {
                        task_id: task_id_for_complete.clone(),
                        created: 0,
                        updated: 0,
                        skipped: 0,
                        failed: 1,
                        duration_ms: 0,
                    })
                    .await;
            }
        }

        complete_state
            .task_manager()
            .complete(&task_id_for_complete)
            .await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "task_id": task_id_for_response,
            "status": "started",
            "total_entries": total,
        })),
    ))
}

/// GET /api/wiki/absorb/events - stream global absorb progress events.
pub(crate) async fn stream_absorb_events_handler(
    State(state): State<AppState>,
) -> ApiResult<impl IntoResponse> {
    let mut receiver = state.desktop().subscribe_skill_events();

    let stream = async_stream::stream! {
        loop {
            match receiver.recv().await {
                Ok(event) => match event {
                    desktop_core::DesktopSessionEvent::AbsorbProgress(_)
                    | desktop_core::DesktopSessionEvent::AbsorbComplete { .. } => {
                        if let Ok(sse_event) = to_sse_event(&event) {
                            yield Ok::<axum::response::sse::Event, std::convert::Infallible>(sse_event);
                        }
                    }
                    _ => {}
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(axum::response::sse::Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    ))
}

fn is_valid_iso_date(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 10 {
        return false;
    }
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    for &b in &bytes[..4] {
        if !b.is_ascii_digit() {
            return false;
        }
    }
    for &b in &bytes[5..7] {
        if !b.is_ascii_digit() {
            return false;
        }
    }
    for &b in &bytes[8..10] {
        if !b.is_ascii_digit() {
            return false;
        }
    }
    true
}

#[derive(Deserialize)]
pub(crate) struct QueryWikiRequest {
    question: String,
    max_sources: Option<usize>,
}

/// POST /api/wiki/query - Wiki-grounded Q&A as an SSE stream.
pub(crate) async fn query_wiki_handler(
    State(_state): State<AppState>,
    Json(body): Json<QueryWikiRequest>,
) -> Result<
    axum::response::sse::Sse<
        impl futures::stream::Stream<
            Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    >,
    ApiError,
> {
    let question = body.question.trim().to_string();
    if question.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "EMPTY_QUESTION".to_string(),
            }),
        ));
    }
    if question.len() > 2000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "QUESTION_TOO_LONG".to_string(),
            }),
        ));
    }

    let paths = resolve_wiki_root_for_handler()?;
    let max_sources = body.max_sources.unwrap_or(5).min(20).max(1);
    let adapter = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global();

    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let paths_clone = paths.clone();
    let question_clone = question.clone();
    let query_task = tokio::spawn(async move {
        wiki_maintainer::query_wiki(&paths_clone, &question_clone, max_sources, &adapter, tx).await
    });

    let sse_stream = async_stream::stream! {
        while let Some(chunk) = rx.recv().await {
            let data = serde_json::json!({
                "type": "query_chunk",
                "delta": chunk.delta,
                "source_refs": chunk.source_refs,
            });
            yield Ok(axum::response::sse::Event::default().event("skill").data(data.to_string()));
        }

        let final_payload = match query_task.await {
            Ok(Ok(result)) => make_query_done_payload(&result),
            Ok(Err(maintainer_err)) => make_query_error_payload(&maintainer_err.to_string()),
            Err(join_err) => make_query_error_payload(&format!("query task failed: {join_err}")),
        };
        yield Ok(axum::response::sse::Event::default().event("skill").data(final_payload.to_string()));
    };

    Ok(axum::response::sse::Sse::new(sse_stream))
}

pub(crate) fn make_query_done_payload(result: &wiki_maintainer::QueryResult) -> serde_json::Value {
    serde_json::json!({
        "type": "query_done",
        "sources": result.sources,
        "total_tokens": result.total_tokens,
        "crystallized": result.crystallized,
    })
}

pub(crate) fn make_query_error_payload(error: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "query_error",
        "error": error,
    })
}

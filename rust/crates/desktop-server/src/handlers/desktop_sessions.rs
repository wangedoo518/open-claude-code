use super::super::*;
use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use desktop_core::DesktopSessionEvent;
use std::convert::Infallible;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub(crate) struct SearchQuery {
    q: Option<String>,
}

pub(crate) async fn list_sessions(State(state): State<AppState>) -> Json<DesktopSessionsResponse> {
    Json(DesktopSessionsResponse {
        sessions: state.desktop().list_sessions().await,
    })
}

pub(crate) async fn search_sessions(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<SearchDesktopSessionsResponse> {
    Json(SearchDesktopSessionsResponse {
        results: state
            .desktop()
            .search_sessions(query.q.as_deref().unwrap_or_default())
            .await,
    })
}

pub(crate) async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopSessionRequest>,
) -> ApiResult<(StatusCode, Json<CreateDesktopSessionResponse>)> {
    validate_optional_project_path(&payload.project_path)?;

    let session = state.desktop().create_session(payload).await;
    Ok((
        StatusCode::CREATED,
        Json(CreateDesktopSessionResponse { session }),
    ))
}

pub(crate) async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopSessionDetail>> {
    let session = state
        .desktop()
        .get_session(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(session))
}

pub(crate) async fn append_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<AppendDesktopMessageRequest>,
) -> ApiResult<Json<AppendDesktopMessageResponse>> {
    let session = state
        .desktop()
        .append_user_message(&id, payload.message, payload.mode, payload.purpose)
        .await
        .map_err(into_api_error)?;
    Ok(Json(AppendDesktopMessageResponse { session }))
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BindSourceBody {
    source: desktop_core::SourceRef,
    #[serde(default)]
    reason: Option<String>,
}

pub(crate) async fn bind_source_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<BindSourceBody>,
) -> ApiResult<Json<DesktopSessionDetail>> {
    let session = state
        .desktop()
        .bind_source(&id, body.source, body.reason)
        .await
        .map_err(into_api_error)?;
    Ok(Json(session))
}

pub(crate) async fn clear_source_binding_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopSessionDetail>> {
    let session = state
        .desktop()
        .clear_source_binding(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(session))
}

pub(crate) async fn stream_session_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let (snapshot, mut receiver) = state
        .desktop()
        .subscribe(&id)
        .await
        .map_err(into_api_error)?;

    let stream = stream! {
        if let Ok(event) = to_sse_event(&snapshot) {
            yield Ok::<Event, Infallible>(event);
        }

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let Ok(sse_event) = to_sse_event(&event) {
                        yield Ok::<Event, Infallible>(sse_event);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

pub(crate) fn to_sse_event(event: &DesktopSessionEvent) -> Result<Event, serde_json::Error> {
    Ok(Event::default()
        .event(event.event_name())
        .data(serde_json::to_string(event)?))
}

pub(crate) async fn delete_session_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .desktop
        .delete_session(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

#[derive(Deserialize, Default)]
pub(crate) struct CleanupEmptyRequest {
    #[serde(default)]
    except: Option<String>,
}

pub(crate) async fn cleanup_empty_sessions_handler(
    State(state): State<AppState>,
    body: Option<Json<CleanupEmptyRequest>>,
) -> Json<serde_json::Value> {
    let req = body.map(|b| b.0).unwrap_or_default();
    let deleted = state
        .desktop
        .cleanup_empty_sessions(req.except.as_deref())
        .await;
    let count = deleted.len();
    Json(serde_json::json!({
        "deleted_ids": deleted,
        "deleted_count": count,
    }))
}

pub(crate) async fn rename_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let title = body["title"].as_str().unwrap_or("Untitled");
    state
        .desktop
        .rename_session(&id, title)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn cancel_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .desktop
        .cancel_session(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn resume_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state
        .desktop
        .resume_session(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "session": session })))
}

pub(crate) async fn fork_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let message_index = body
        .get("message_index")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let session = state
        .desktop
        .fork_session(&id, message_index)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "session": session })))
}

pub(crate) async fn set_session_lifecycle_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status_str = body.get("status").and_then(|v| v.as_str()).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "missing status field".to_string(),
            }),
        )
    })?;
    let status = match status_str {
        "todo" => desktop_core::DesktopLifecycleStatus::Todo,
        "in_progress" => desktop_core::DesktopLifecycleStatus::InProgress,
        "needs_review" => desktop_core::DesktopLifecycleStatus::NeedsReview,
        "done" => desktop_core::DesktopLifecycleStatus::Done,
        "archived" => desktop_core::DesktopLifecycleStatus::Archived,
        other => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid status value: {other}"),
                }),
            ));
        }
    };
    let session = state
        .desktop
        .set_session_lifecycle_status(&id, status)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "session": session })))
}

pub(crate) async fn set_session_flag_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flagged = body
        .get("flagged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let session = state
        .desktop
        .set_session_flagged(&id, flagged)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "session": session })))
}

pub(crate) async fn compact_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state
        .desktop
        .compact_session_messages(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(
        serde_json::json!({ "compacted": true, "session": session }),
    ))
}

pub(crate) async fn forward_permission(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let request_id = body
        .get("requestId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "missing or empty requestId".to_string(),
                }),
            )
        })?;

    let decision = body
        .get("decision")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "missing decision".to_string(),
                }),
            )
        })?;

    if !matches!(decision, "allow" | "deny") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid decision: {decision} (expected: allow | deny)"),
            }),
        ));
    }

    state
        .desktop
        .forward_permission_decision(&id, request_id, decision)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "forwarded": true })))
}

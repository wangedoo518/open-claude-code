use super::super::*;

pub(crate) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

pub(crate) async fn bootstrap(State(state): State<AppState>) -> Json<DesktopBootstrap> {
    Json(state.desktop().bootstrap())
}

pub(crate) async fn workbench(State(state): State<AppState>) -> Json<DesktopWorkbench> {
    Json(state.desktop().workbench().await)
}

pub(crate) async fn customize(State(state): State<AppState>) -> Json<DesktopCustomizeResponse> {
    Json(DesktopCustomizeResponse {
        customize: state.desktop().customize().await,
    })
}

pub(crate) async fn dispatch(State(state): State<AppState>) -> Json<DesktopDispatchResponse> {
    Json(DesktopDispatchResponse {
        dispatch: state.desktop().dispatch().await,
    })
}

pub(crate) async fn scheduled(State(state): State<AppState>) -> Json<DesktopScheduledResponse> {
    Json(DesktopScheduledResponse {
        scheduled: state.desktop().scheduled().await,
    })
}

pub(crate) async fn settings(State(state): State<AppState>) -> Json<DesktopSettingsResponse> {
    Json(DesktopSettingsResponse {
        settings: state.desktop().settings().await,
    })
}

pub(crate) async fn create_scheduled_task(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopScheduledTaskRequest>,
) -> ApiResult<(StatusCode, Json<DesktopScheduledTaskResponse>)> {
    validate_optional_project_path(&payload.project_path)?;

    let task = state
        .desktop()
        .create_scheduled_task(payload)
        .await
        .map_err(into_api_error)?;
    Ok((
        StatusCode::CREATED,
        Json(DesktopScheduledTaskResponse { task }),
    ))
}

pub(crate) async fn create_dispatch_item(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopDispatchItemRequest>,
) -> ApiResult<(StatusCode, Json<DesktopDispatchItemResponse>)> {
    let item = state
        .desktop()
        .create_dispatch_item(payload)
        .await
        .map_err(into_api_error)?;
    Ok((
        StatusCode::CREATED,
        Json(DesktopDispatchItemResponse { item }),
    ))
}

pub(crate) async fn update_dispatch_item_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateDesktopDispatchItemStatusRequest>,
) -> ApiResult<Json<DesktopDispatchItemResponse>> {
    let item = state
        .desktop()
        .update_dispatch_item_status(&id, payload.status)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopDispatchItemResponse { item }))
}

pub(crate) async fn deliver_dispatch_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopDispatchItemResponse>> {
    let item = state
        .desktop()
        .deliver_dispatch_item(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopDispatchItemResponse { item }))
}

pub(crate) async fn update_scheduled_task_enabled(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateDesktopScheduledTaskRequest>,
) -> ApiResult<Json<DesktopScheduledTaskResponse>> {
    let task = state
        .desktop()
        .update_scheduled_task_enabled(&id, payload.enabled)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopScheduledTaskResponse { task }))
}

pub(crate) async fn run_scheduled_task_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopScheduledTaskResponse>> {
    let task = state
        .desktop()
        .run_scheduled_task_now(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopScheduledTaskResponse { task }))
}

pub(crate) async fn list_workspace_skills_handler(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_path = params.get("project_path").ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "missing project_path query param".to_string(),
            }),
        )
    })?;
    let validated = desktop_core::validate_project_path(project_path).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: e }),
        )
    })?;
    let skills = desktop_core::system_prompt::find_workspace_skills(&validated);
    let summary: Vec<serde_json::Value> = skills
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "description": s.description,
                "source": s.source.display().to_string(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "count": skills.len(),
        "skills": summary,
    })))
}

pub(crate) async fn process_attachment_handler(
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use base64::Engine;
    use desktop_core::attachments::{process_attachment, AttachmentKind};

    let filename = body
        .get("filename")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "missing filename".to_string(),
                }),
            )
        })?;

    let base64_data = body.get("base64").and_then(|v| v.as_str()).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "missing base64 payload".to_string(),
            }),
        )
    })?;

    const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
    const MAX_BASE64_CHARS: usize = (MAX_ATTACHMENT_BYTES * 4) / 3 + 4;
    if base64_data.len() > MAX_BASE64_CHARS {
        return Err((
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse {
                error: format!(
                    "attachment base64 too large: {} chars (max {})",
                    base64_data.len(),
                    MAX_BASE64_CHARS
                ),
            }),
        ));
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid base64 payload: {e}"),
                }),
            )
        })?;

    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err((
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse {
                error: format!(
                    "attachment decoded too large: {} bytes (max {})",
                    bytes.len(),
                    MAX_ATTACHMENT_BYTES
                ),
            }),
        ));
    }

    let result = process_attachment(filename, &bytes);
    let kind_str = match result.kind {
        AttachmentKind::Text => "text",
        AttachmentKind::ImageBase64 => "image_base64",
        AttachmentKind::BinaryStub => "binary_stub",
    };

    Ok(Json(serde_json::json!({
        "filename": result.filename,
        "content": result.content,
        "truncated": result.truncated,
        "kind": kind_str,
        "byte_size": bytes.len(),
    })))
}

pub(crate) async fn debug_mcp_probe_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_path = body
        .get("project_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "missing project_path".to_string(),
                }),
            )
        })?;
    let validated = desktop_core::validate_project_path(project_path).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: e }),
        )
    })?;
    let tools = state.desktop.ensure_mcp_initialized(&validated).await;
    let summary: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "server": t.server_name,
                "qualified_name": t.qualified_name,
                "raw_name": t.raw_name,
                "description": t.tool.description,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "tool_count": tools.len(),
        "tools": summary,
    })))
}

pub(crate) async fn debug_mcp_call_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let qualified_name = body
        .get("qualified_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "missing qualified_name".to_string(),
                }),
            )
        })?;
    let arguments = body
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    match state.desktop.mcp_call_tool(qualified_name, arguments).await {
        Ok(result) => Ok(Json(serde_json::json!({
            "ok": true,
            "result": result,
        }))),
        Err(err) => Ok(Json(serde_json::json!({
            "ok": false,
            "error": err,
        }))),
    }
}

pub(crate) async fn set_permission_mode_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_path = body
        .get("project_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "missing project_path".to_string(),
                }),
            )
        })?;
    let validated = desktop_core::validate_project_path(project_path).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: e }),
        )
    })?;
    let mode = body.get("mode").and_then(|v| v.as_str()).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "missing mode".to_string(),
            }),
        )
    })?;
    state
        .desktop
        .set_permission_mode(&validated.display().to_string(), mode)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "ok": true, "mode": mode })))
}

pub(crate) async fn get_permission_mode_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_path = params.get("project_path").cloned().unwrap_or_default();
    if !project_path.is_empty() {
        desktop_core::validate_project_path(&project_path).map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: e }),
            )
        })?;
    }
    let mode = state
        .desktop
        .get_permission_mode(&project_path)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "mode": mode })))
}

pub(crate) async fn delete_scheduled_task_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .desktop
        .delete_scheduled_task(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

pub(crate) async fn update_scheduled_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<DesktopScheduledTaskResponse>, ApiError> {
    let task = state
        .desktop
        .update_scheduled_task(
            &id,
            body["title"].as_str().map(|s| s.to_string()),
            body["prompt"].as_str().map(|s| s.to_string()),
            body["enabled"].as_bool(),
        )
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopScheduledTaskResponse { task }))
}

pub(crate) async fn delete_dispatch_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .desktop
        .delete_dispatch_item(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

pub(crate) async fn update_dispatch_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<DesktopDispatchItemResponse>, ApiError> {
    let priority = body["priority"]
        .as_str()
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok());
    let item = state
        .desktop
        .update_dispatch_item(
            &id,
            body["title"].as_str().map(|s| s.to_string()),
            body["body"].as_str().map(|s| s.to_string()),
            priority,
        )
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopDispatchItemResponse { item }))
}

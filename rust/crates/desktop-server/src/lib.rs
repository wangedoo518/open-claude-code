use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::http::{Method, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use desktop_core::{
    AppendDesktopMessageRequest, CreateDesktopDispatchItemRequest,
    CreateDesktopScheduledTaskRequest, CreateDesktopSessionRequest, DesktopBootstrap,
    DesktopCodeToolLaunchProfile, DesktopCodexAuthOverview, DesktopCodexLoginSessionSnapshot,
    DesktopCodexRuntimeState, DesktopCustomizeState, DesktopDispatchItem, DesktopDispatchState,
    DesktopManagedAuthAccount, DesktopManagedAuthLoginSessionSnapshot, DesktopManagedAuthProvider,
    DesktopScheduledState, DesktopScheduledTask, DesktopSearchHit, DesktopSessionDetail,
    DesktopSessionEvent, DesktopSessionSummary, DesktopSettingsState, DesktopState,
    DesktopStateError, DesktopWorkbench, UpdateDesktopDispatchItemStatusRequest,
    UpdateDesktopScheduledTaskRequest,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

mod code_tools_bridge;

#[derive(Clone, Default)]
pub struct AppState {
    desktop: DesktopState,
}

impl AppState {
    #[must_use]
    pub fn new(desktop: DesktopState) -> Self {
        Self { desktop }
    }

    #[must_use]
    pub fn desktop(&self) -> &DesktopState {
        &self.desktop
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionsResponse {
    pub sessions: Vec<DesktopSessionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchDesktopSessionsResponse {
    pub results: Vec<DesktopSearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCustomizeResponse {
    pub customize: DesktopCustomizeState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopScheduledResponse {
    pub scheduled: DesktopScheduledState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopScheduledTaskResponse {
    pub task: DesktopScheduledTask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDispatchResponse {
    pub dispatch: DesktopDispatchState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDispatchItemResponse {
    pub item: DesktopDispatchItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopSettingsResponse {
    pub settings: DesktopSettingsState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthProvidersResponse {
    pub providers: Vec<DesktopManagedAuthProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthAccountsResponse {
    pub provider: DesktopManagedAuthProvider,
    pub accounts: Vec<DesktopManagedAuthAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedAuthLoginSessionResponse {
    pub session: DesktopManagedAuthLoginSessionSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCodeToolLaunchProfileRequest {
    pub cli_tool: String,
    pub provider_id: String,
    pub model_id: String,
    pub desktop_api_base: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCodeToolLaunchProfileResponse {
    pub launch_profile: DesktopCodeToolLaunchProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexRuntimeResponse {
    pub runtime: DesktopCodexRuntimeState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexAuthOverviewResponse {
    pub overview: DesktopCodexAuthOverview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopCodexLoginSessionResponse {
    pub session: DesktopCodexLoginSessionSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateDesktopSessionResponse {
    pub session: DesktopSessionDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppendDesktopMessageResponse {
    pub session: DesktopSessionDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

type ApiError = (StatusCode, Json<ErrorResponse>);
type ApiResult<T> = Result<T, ApiError>;

#[must_use]
pub fn app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS]);

    Router::new()
        .route("/healthz", get(health))
        .route("/api/desktop/bootstrap", get(bootstrap))
        .route("/api/desktop/workbench", get(workbench))
        .route("/api/desktop/customize", get(customize))
        .route("/api/desktop/codex/runtime", get(codex_runtime))
        .route("/api/desktop/codex/auth", get(codex_auth_overview))
        .route(
            "/api/desktop/codex/auth/import",
            post(import_codex_auth_profile),
        )
        .route("/api/desktop/codex/auth/login", post(begin_codex_login))
        .route("/api/desktop/codex/auth/login/{id}", get(poll_codex_login))
        .route(
            "/api/desktop/codex/auth/profiles/{id}/activate",
            post(activate_codex_auth_profile),
        )
        .route(
            "/api/desktop/codex/auth/profiles/{id}/refresh",
            post(refresh_codex_auth_profile),
        )
        .route(
            "/api/desktop/codex/auth/profiles/{id}",
            delete(remove_codex_auth_profile),
        )
        .route("/api/desktop/auth/providers", get(managed_auth_providers))
        .route(
            "/api/desktop/auth/providers/{provider}/accounts",
            get(managed_auth_accounts),
        )
        .route(
            "/api/desktop/code-tools/launch-profile",
            post(code_tool_launch_profile),
        )
        .route(
            "/api/desktop/code-tools/claude-bridge/{provider}",
            get(code_tools_bridge::ready),
        )
        .route(
            "/api/desktop/code-tools/claude-bridge/{provider}/v1/messages",
            post(code_tools_bridge::handle_messages),
        )
        .route(
            "/api/desktop/auth/providers/{provider}/import",
            post(import_managed_auth_accounts),
        )
        .route(
            "/api/desktop/auth/providers/{provider}/login",
            post(begin_managed_auth_login),
        )
        .route(
            "/api/desktop/auth/providers/{provider}/login/{id}",
            get(poll_managed_auth_login),
        )
        .route(
            "/api/desktop/auth/providers/{provider}/accounts/{id}/default",
            post(set_managed_auth_default_account),
        )
        .route(
            "/api/desktop/auth/providers/{provider}/accounts/{id}/refresh",
            post(refresh_managed_auth_account),
        )
        .route(
            "/api/desktop/auth/providers/{provider}/accounts/{id}",
            delete(remove_managed_auth_account),
        )
        .route(
            "/api/desktop/dispatch",
            get(dispatch).post(create_dispatch_item),
        )
        .route(
            "/api/desktop/dispatch/items/{id}/status",
            post(update_dispatch_item_status),
        )
        .route(
            "/api/desktop/dispatch/items/{id}/deliver",
            post(deliver_dispatch_item),
        )
        .route(
            "/api/desktop/scheduled",
            get(scheduled).post(create_scheduled_task),
        )
        .route("/api/desktop/settings", get(settings))
        .route("/api/desktop/search", get(search_sessions))
        .route(
            "/api/desktop/sessions",
            get(list_sessions).post(create_session),
        )
        .route(
            "/api/desktop/scheduled/{id}/enabled",
            post(update_scheduled_task_enabled),
        )
        .route(
            "/api/desktop/scheduled/{id}/run",
            post(run_scheduled_task_now),
        )
        .route("/api/desktop/sessions/{id}", get(get_session).delete(delete_session_handler))
        .route("/api/desktop/sessions/{id}/messages", post(append_message))
        .route("/api/desktop/sessions/{id}/title", post(rename_session))
        .route("/api/desktop/sessions/{id}/cancel", post(cancel_session))
        .route("/api/desktop/sessions/{id}/resume", post(resume_session))
        .route("/api/desktop/sessions/{id}/permission", post(forward_permission))
        .route(
            "/api/desktop/sessions/{id}/events",
            get(stream_session_events),
        )
        .route(
            "/api/desktop/scheduled/{id}",
            delete(delete_scheduled_task_handler).post(update_scheduled_task),
        )
        .route(
            "/api/desktop/dispatch/items/{id}",
            delete(delete_dispatch_item_handler).post(update_dispatch_item),
        )
        .layer(cors)
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn bootstrap(State(state): State<AppState>) -> Json<DesktopBootstrap> {
    Json(state.desktop().bootstrap())
}

async fn workbench(State(state): State<AppState>) -> Json<DesktopWorkbench> {
    Json(state.desktop().workbench().await)
}

async fn customize(State(state): State<AppState>) -> Json<DesktopCustomizeResponse> {
    Json(DesktopCustomizeResponse {
        customize: state.desktop().customize().await,
    })
}

async fn codex_runtime(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexRuntimeResponse>> {
    let runtime = state
        .desktop()
        .codex_runtime_state()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexRuntimeResponse { runtime }))
}

async fn codex_auth_overview(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexAuthOverviewResponse>> {
    let overview = state
        .desktop()
        .codex_auth_overview()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexAuthOverviewResponse { overview }))
}

async fn import_codex_auth_profile(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexAuthOverviewResponse>> {
    let overview = state
        .desktop()
        .import_codex_auth_profile()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexAuthOverviewResponse { overview }))
}

async fn begin_codex_login(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexLoginSessionResponse>> {
    let session = state
        .desktop()
        .begin_codex_login()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexLoginSessionResponse { session }))
}

async fn poll_codex_login(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopCodexLoginSessionResponse>> {
    let session = state
        .desktop()
        .poll_codex_login(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexLoginSessionResponse { session }))
}

async fn activate_codex_auth_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopCodexAuthOverviewResponse>> {
    let overview = state
        .desktop()
        .activate_codex_auth_profile(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexAuthOverviewResponse { overview }))
}

async fn refresh_codex_auth_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopCodexAuthOverviewResponse>> {
    let overview = state
        .desktop()
        .refresh_codex_auth_profile(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexAuthOverviewResponse { overview }))
}

async fn remove_codex_auth_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopCodexAuthOverviewResponse>> {
    let overview = state
        .desktop()
        .remove_codex_auth_profile(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexAuthOverviewResponse { overview }))
}

async fn managed_auth_providers(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopManagedAuthProvidersResponse>> {
    let providers = state
        .desktop()
        .managed_auth_providers()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthProvidersResponse { providers }))
}

async fn managed_auth_accounts(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> ApiResult<Json<DesktopManagedAuthAccountsResponse>> {
    let provider_state = state
        .desktop()
        .managed_auth_provider(&provider)
        .await
        .map_err(into_api_error)?;
    let accounts = state
        .desktop()
        .managed_auth_accounts(&provider)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthAccountsResponse {
        provider: provider_state,
        accounts,
    }))
}

async fn import_managed_auth_accounts(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> ApiResult<Json<DesktopManagedAuthAccountsResponse>> {
    let accounts = state
        .desktop()
        .import_managed_auth_accounts(&provider)
        .await
        .map_err(into_api_error)?;
    let provider_state = state
        .desktop()
        .managed_auth_provider(&provider)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthAccountsResponse {
        provider: provider_state,
        accounts,
    }))
}

async fn begin_managed_auth_login(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> ApiResult<Json<DesktopManagedAuthLoginSessionResponse>> {
    let session = state
        .desktop()
        .begin_managed_auth_login(&provider)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthLoginSessionResponse { session }))
}

async fn poll_managed_auth_login(
    State(state): State<AppState>,
    Path((provider, id)): Path<(String, String)>,
) -> ApiResult<Json<DesktopManagedAuthLoginSessionResponse>> {
    let session = state
        .desktop()
        .poll_managed_auth_login(&provider, &id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthLoginSessionResponse { session }))
}

async fn set_managed_auth_default_account(
    State(state): State<AppState>,
    Path((provider, id)): Path<(String, String)>,
) -> ApiResult<Json<DesktopManagedAuthAccountsResponse>> {
    let accounts = state
        .desktop()
        .set_managed_auth_default_account(&provider, &id)
        .await
        .map_err(into_api_error)?;
    let provider_state = state
        .desktop()
        .managed_auth_provider(&provider)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthAccountsResponse {
        provider: provider_state,
        accounts,
    }))
}

async fn refresh_managed_auth_account(
    State(state): State<AppState>,
    Path((provider, id)): Path<(String, String)>,
) -> ApiResult<Json<DesktopManagedAuthAccountsResponse>> {
    let accounts = state
        .desktop()
        .refresh_managed_auth_account(&provider, &id)
        .await
        .map_err(into_api_error)?;
    let provider_state = state
        .desktop()
        .managed_auth_provider(&provider)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthAccountsResponse {
        provider: provider_state,
        accounts,
    }))
}

async fn remove_managed_auth_account(
    State(state): State<AppState>,
    Path((provider, id)): Path<(String, String)>,
) -> ApiResult<Json<DesktopManagedAuthAccountsResponse>> {
    let accounts = state
        .desktop()
        .remove_managed_auth_account(&provider, &id)
        .await
        .map_err(into_api_error)?;
    let provider_state = state
        .desktop()
        .managed_auth_provider(&provider)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthAccountsResponse {
        provider: provider_state,
        accounts,
    }))
}

async fn code_tool_launch_profile(
    State(state): State<AppState>,
    Json(request): Json<DesktopCodeToolLaunchProfileRequest>,
) -> ApiResult<Json<DesktopCodeToolLaunchProfileResponse>> {
    let launch_profile = state
        .desktop()
        .code_tool_launch_profile(
            &request.cli_tool,
            &request.provider_id,
            &request.model_id,
            &request.desktop_api_base,
        )
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodeToolLaunchProfileResponse {
        launch_profile,
    }))
}

async fn dispatch(State(state): State<AppState>) -> Json<DesktopDispatchResponse> {
    Json(DesktopDispatchResponse {
        dispatch: state.desktop().dispatch().await,
    })
}

async fn scheduled(State(state): State<AppState>) -> Json<DesktopScheduledResponse> {
    Json(DesktopScheduledResponse {
        scheduled: state.desktop().scheduled().await,
    })
}

async fn settings(State(state): State<AppState>) -> Json<DesktopSettingsResponse> {
    Json(DesktopSettingsResponse {
        settings: state.desktop().settings().await,
    })
}

async fn list_sessions(State(state): State<AppState>) -> Json<DesktopSessionsResponse> {
    Json(DesktopSessionsResponse {
        sessions: state.desktop().list_sessions().await,
    })
}

async fn search_sessions(
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

async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopSessionRequest>,
) -> (StatusCode, Json<CreateDesktopSessionResponse>) {
    let session = state.desktop().create_session(payload).await;
    (
        StatusCode::CREATED,
        Json(CreateDesktopSessionResponse { session }),
    )
}

async fn create_scheduled_task(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopScheduledTaskRequest>,
) -> ApiResult<(StatusCode, Json<DesktopScheduledTaskResponse>)> {
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

async fn create_dispatch_item(
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

async fn update_dispatch_item_status(
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

async fn deliver_dispatch_item(
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

async fn update_scheduled_task_enabled(
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

async fn run_scheduled_task_now(
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

async fn get_session(
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

async fn append_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<AppendDesktopMessageRequest>,
) -> ApiResult<Json<AppendDesktopMessageResponse>> {
    let session = state
        .desktop()
        .append_user_message(&id, payload.message)
        .await
        .map_err(into_api_error)?;
    Ok(Json(AppendDesktopMessageResponse { session }))
}

async fn stream_session_events(
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

fn to_sse_event(event: &DesktopSessionEvent) -> Result<Event, serde_json::Error> {
    Ok(Event::default()
        .event(event.event_name())
        .data(serde_json::to_string(event)?))
}

fn into_api_error(error: DesktopStateError) -> ApiError {
    let status = match error {
        DesktopStateError::SessionNotFound(_) => StatusCode::NOT_FOUND,
        DesktopStateError::SessionBusy(_) => StatusCode::CONFLICT,
        DesktopStateError::ScheduledTaskNotFound(_) => StatusCode::NOT_FOUND,
        DesktopStateError::ScheduledTaskBusy(_) => StatusCode::CONFLICT,
        DesktopStateError::InvalidScheduledTask(_) => StatusCode::BAD_REQUEST,
        DesktopStateError::DispatchItemNotFound(_) => StatusCode::NOT_FOUND,
        DesktopStateError::InvalidDispatchItem(_) => StatusCode::BAD_REQUEST,
        DesktopStateError::ProviderNotFound(_) => StatusCode::NOT_FOUND,
        DesktopStateError::InvalidProvider(_) => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

// ── Session manipulation handlers ──────────────────────────────────

async fn delete_session_handler(
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

async fn rename_session(
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

async fn cancel_session(
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

async fn resume_session(
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

async fn forward_permission(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let request_id = body["requestId"].as_str().unwrap_or("");
    let decision = body["decision"].as_str().unwrap_or("deny");
    state
        .desktop
        .forward_permission_decision(&id, request_id, decision)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "forwarded": true })))
}

// ── Scheduled task extended CRUD handlers ──────────────────────────

async fn delete_scheduled_task_handler(
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

async fn update_scheduled_task(
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

// ── Dispatch item extended CRUD handlers ──────────────────────────

async fn delete_dispatch_item_handler(
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

async fn update_dispatch_item(
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

pub async fn serve(state: AppState, address: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, app(state)).await
}

#[cfg(test)]
mod tests {
    use super::{
        app, AppState, CreateDesktopSessionResponse, DesktopDispatchItemResponse,
        DesktopDispatchResponse, DesktopScheduledResponse, DesktopScheduledTaskResponse,
        DesktopSessionsResponse, HealthResponse,
    };
    use reqwest::Client;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    struct TestServer {
        address: SocketAddr,
        handle: JoinHandle<()>,
    }

    impl TestServer {
        async fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("test listener should bind");
            let address = listener
                .local_addr()
                .expect("listener should report local address");
            let handle = tokio::spawn(async move {
                axum::serve(listener, app(AppState::default()))
                    .await
                    .expect("desktop server should run");
            });

            Self { address, handle }
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}{}", self.address, path)
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    #[tokio::test]
    async fn bootstrap_and_session_routes_work() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        let health = client
            .get(server.url("/healthz"))
            .send()
            .await
            .expect("health request should succeed")
            .json::<HealthResponse>()
            .await
            .expect("health payload should decode");
        assert_eq!(health.status, "ok");

        let sessions = client
            .get(server.url("/api/desktop/sessions"))
            .send()
            .await
            .expect("list sessions request should succeed")
            .json::<DesktopSessionsResponse>()
            .await
            .expect("sessions response should decode");
        assert!(!sessions.sessions.is_empty());

        let created = client
            .post(server.url("/api/desktop/sessions"))
            .json(&serde_json::json!({ "title": "Desktop Session" }))
            .send()
            .await
            .expect("create session request should succeed")
            .json::<CreateDesktopSessionResponse>()
            .await
            .expect("create session response should decode");
        assert_eq!(created.session.title, "Desktop Session");

        let scheduled = client
            .get(server.url("/api/desktop/scheduled"))
            .send()
            .await
            .expect("scheduled request should succeed")
            .json::<DesktopScheduledResponse>()
            .await
            .expect("scheduled response should decode");
        assert_eq!(scheduled.scheduled.summary.total_task_count, 0);

        let created_task = client
            .post(server.url("/api/desktop/scheduled"))
            .json(&serde_json::json!({
                "title": "Morning sweep",
                "prompt": "Review the workspace and continue from the highest-value next step.",
                "schedule": {
                    "kind": "hourly",
                    "interval_hours": 4
                }
            }))
            .send()
            .await
            .expect("create scheduled task request should succeed")
            .json::<DesktopScheduledTaskResponse>()
            .await
            .expect("create scheduled task response should decode");
        assert_eq!(created_task.task.title, "Morning sweep");

        let dispatch = client
            .get(server.url("/api/desktop/dispatch"))
            .send()
            .await
            .expect("dispatch request should succeed")
            .json::<DesktopDispatchResponse>()
            .await
            .expect("dispatch response should decode");
        assert_eq!(dispatch.dispatch.summary.total_item_count, 0);

        let created_dispatch_item = client
            .post(server.url("/api/desktop/dispatch"))
            .json(&serde_json::json!({
                "title": "Inbox follow-up",
                "body": "Continue the implementation from the dispatch queue.",
                "priority": "high"
            }))
            .send()
            .await
            .expect("create dispatch item request should succeed")
            .json::<DesktopDispatchItemResponse>()
            .await
            .expect("create dispatch item response should decode");
        assert_eq!(created_dispatch_item.item.title, "Inbox follow-up");
    }
}

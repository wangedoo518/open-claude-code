use std::collections::BTreeMap;
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
    DesktopCodexAuthOverview, DesktopCodexLoginSessionSnapshot, DesktopCodexRuntimeState,
    DesktopCustomizeState, DesktopDispatchItem, DesktopDispatchState, DesktopManagedProvider,
    DesktopManagedProviderUpsertInput, DesktopOpenClawConfigWriteResult,
    DesktopOpenClawRuntimeState, DesktopProviderConnectionTestInput,
    DesktopProviderConnectionTestResult, DesktopProviderDeleteResult, DesktopProviderPreset,
    DesktopProviderSyncResult, DesktopScheduledState, DesktopScheduledTask, DesktopSearchHit,
    DesktopSessionDetail, DesktopSessionEvent, DesktopSessionSummary, DesktopSettingsState,
    DesktopState, DesktopStateError, DesktopWorkbench, UpdateDesktopDispatchItemStatusRequest,
    UpdateDesktopScheduledTaskRequest,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

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
pub struct DesktopProviderPresetsResponse {
    pub presets: Vec<DesktopProviderPreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedProvidersResponse {
    pub providers: Vec<DesktopManagedProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopManagedProviderResponse {
    pub provider: DesktopManagedProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderImportResponse {
    pub providers: Vec<DesktopManagedProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderDeleteResponse {
    pub result: DesktopProviderDeleteResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderSyncResponse {
    pub result: DesktopProviderSyncResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConnectionTestResponse {
    pub result: DesktopProviderConnectionTestResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopOpenClawRuntimeResponse {
    pub runtime: DesktopOpenClawRuntimeState,
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
pub struct DesktopOpenClawConfigWriteResponse {
    pub result: DesktopOpenClawConfigWriteResult,
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

#[derive(Debug, Clone, Deserialize)]
struct ProviderImportRequest {
    provider_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderSyncRequest {
    set_primary: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenclawEnvUpdateRequest {
    env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenclawToolsUpdateRequest {
    tools: serde_json::Value,
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
        .route("/api/desktop/openclaw/runtime", get(openclaw_runtime))
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
        .route(
            "/api/desktop/codex/import-live",
            post(import_codex_live_providers),
        )
        .route("/api/desktop/openclaw/env", post(update_openclaw_env))
        .route("/api/desktop/openclaw/tools", post(update_openclaw_tools))
        .route("/api/desktop/providers/presets", get(provider_presets))
        .route(
            "/api/desktop/providers",
            get(managed_providers).post(upsert_managed_provider),
        )
        .route(
            "/api/desktop/providers/test",
            post(test_provider_connection),
        )
        .route(
            "/api/desktop/providers/import-live",
            post(import_live_providers),
        )
        .route(
            "/api/desktop/providers/{id}/sync",
            post(sync_managed_provider),
        )
        .route(
            "/api/desktop/providers/{id}",
            delete(delete_managed_provider),
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
        .route("/api/desktop/sessions/{id}", get(get_session))
        .route("/api/desktop/sessions/{id}/messages", post(append_message))
        .route(
            "/api/desktop/sessions/{id}/events",
            get(stream_session_events),
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

async fn openclaw_runtime(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopOpenClawRuntimeResponse>> {
    let runtime = state
        .desktop()
        .openclaw_runtime_state()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopOpenClawRuntimeResponse { runtime }))
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

async fn update_openclaw_env(
    State(state): State<AppState>,
    Json(payload): Json<OpenclawEnvUpdateRequest>,
) -> ApiResult<Json<DesktopOpenClawConfigWriteResponse>> {
    let result = state
        .desktop()
        .set_openclaw_env(payload.env)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopOpenClawConfigWriteResponse { result }))
}

async fn update_openclaw_tools(
    State(state): State<AppState>,
    Json(payload): Json<OpenclawToolsUpdateRequest>,
) -> ApiResult<Json<DesktopOpenClawConfigWriteResponse>> {
    let result = state
        .desktop()
        .set_openclaw_tools(payload.tools)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopOpenClawConfigWriteResponse { result }))
}

async fn provider_presets(State(state): State<AppState>) -> Json<DesktopProviderPresetsResponse> {
    Json(DesktopProviderPresetsResponse {
        presets: state.desktop().provider_presets().await,
    })
}

async fn managed_providers(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopManagedProvidersResponse>> {
    let providers = state
        .desktop()
        .managed_providers()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedProvidersResponse { providers }))
}

async fn upsert_managed_provider(
    State(state): State<AppState>,
    Json(payload): Json<DesktopManagedProviderUpsertInput>,
) -> ApiResult<Json<DesktopManagedProviderResponse>> {
    let provider = state
        .desktop()
        .upsert_managed_provider(payload)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedProviderResponse { provider }))
}

async fn test_provider_connection(
    State(state): State<AppState>,
    Json(payload): Json<DesktopProviderConnectionTestInput>,
) -> ApiResult<Json<DesktopProviderConnectionTestResponse>> {
    let result = state
        .desktop()
        .test_provider_connection(payload)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopProviderConnectionTestResponse { result }))
}

async fn import_live_providers(
    State(state): State<AppState>,
    Json(payload): Json<ProviderImportRequest>,
) -> ApiResult<Json<DesktopProviderImportResponse>> {
    let providers = state
        .desktop()
        .import_managed_providers_from_openclaw_live(payload.provider_ids)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopProviderImportResponse { providers }))
}

async fn import_codex_live_providers(
    State(state): State<AppState>,
    Json(payload): Json<ProviderImportRequest>,
) -> ApiResult<Json<DesktopProviderImportResponse>> {
    let providers = state
        .desktop()
        .import_managed_providers_from_codex_live(payload.provider_ids)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopProviderImportResponse { providers }))
}

async fn sync_managed_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ProviderSyncRequest>,
) -> ApiResult<Json<DesktopProviderSyncResponse>> {
    let result = state
        .desktop()
        .sync_managed_provider_to_openclaw(&id, payload.set_primary.unwrap_or(false))
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopProviderSyncResponse { result }))
}

async fn delete_managed_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DesktopProviderDeleteResponse>> {
    let result = state
        .desktop()
        .delete_managed_provider(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopProviderDeleteResponse { result }))
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

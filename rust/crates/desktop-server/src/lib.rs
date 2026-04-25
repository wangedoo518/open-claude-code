use std::net::SocketAddr;

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
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
    DesktopSessionSummary, DesktopSettingsState, DesktopState, DesktopStateError, DesktopWorkbench,
    UpdateDesktopDispatchItemStatusRequest, UpdateDesktopScheduledTaskRequest,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};

mod handlers;
mod routes;
pub(crate) use handlers::desktop_sessions::{
    append_message, bind_source_handler, cancel_session, cleanup_empty_sessions_handler,
    clear_source_binding_handler, compact_session, create_session, delete_session_handler,
    fork_session, forward_permission, get_session, list_sessions, rename_session, resume_session,
    search_sessions, set_session_flag_handler, set_session_lifecycle_handler,
    stream_session_events, to_sse_event,
};
pub(crate) use handlers::provider_runtime::{
    activate_codex_auth_profile, activate_provider_handler, begin_codex_login,
    begin_managed_auth_login, codex_auth_overview, codex_runtime, delete_provider_handler,
    import_codex_auth_profile, import_managed_auth_accounts, list_provider_templates_handler,
    list_providers_handler, managed_auth_accounts, managed_auth_providers, poll_codex_login,
    poll_managed_auth_login, refresh_codex_auth_profile, refresh_managed_auth_account,
    remove_codex_auth_profile, remove_managed_auth_account, set_managed_auth_default_account,
    test_provider_handler, upsert_provider_handler,
};
pub(crate) use handlers::wiki_reports::{
    cleanup_handler, get_absorb_log_handler, get_backlinks_index_handler,
    get_patrol_report_handler, get_schema_templates_handler, get_stats_handler, patrol_handler,
};
pub(crate) use handlers::wiki_tasks::{
    absorb_handler, query_wiki_handler, stream_absorb_events_handler,
};
#[cfg(test)]
pub(crate) use handlers::wiki_tasks::{make_query_done_payload, make_query_error_payload};

// S0.4 cut day: `mod code_tools_bridge` is gone along with the
// `/api/desktop/code-tools/*` routes. ClawWiki canonical §11.1 cut #3
// — there is no /code page, no CLI launcher, no claude-bridge proxy.

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    desktop: DesktopState,
    // Optional private-cloud account pool. OSS builds keep this off
    // and rely on the generic provider registry instead.
    #[cfg(feature = "private-cloud")]
    broker: Arc<desktop_core::codex_broker::CodexBroker>,
    // feat(O): broadcast channel for inbox change notifications. Every
    // time a raw entry lands, an inbox task is appended, or a proposal
    // is approved, the handler sends a unit `()` through this channel.
    // WS clients subscribed via `/ws/wechat-inbox` receive the signal
    // and can trigger an immediate refetch, replacing the 30s polling
    // interval with sub-second reactivity (canonical §2: "3 秒内").
    inbox_notify: tokio::sync::broadcast::Sender<()>,
    // Graceful-shutdown plumbing. The /internal/shutdown route verifies
    // the header token matches `shutdown_token` and, on success, calls
    // `shutdown_cancel.cancel()` which trips the `with_graceful_shutdown`
    // future inside `serve_with_shutdown`, letting axum finish in-flight
    // requests and drop tasks (so `SessionCleanupGuard::drop` actually
    // runs instead of the old "window close = force-kill child" path).
    //
    // Both fields are `Arc`-wrapped because AppState is `Clone` and we
    // want every cloned copy to see the same token + cancel source.
    shutdown_token: Arc<String>,
    shutdown_cancel: CancellationToken,
}

#[cfg(feature = "private-cloud")]
fn build_default_broker() -> Arc<desktop_core::codex_broker::CodexBroker> {
    let fallback = std::env::temp_dir().join(format!("warwolf-broker-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&fallback);
    let broker = desktop_core::codex_broker::CodexBroker::new(&fallback).unwrap_or_else(|_| {
        let alt = std::env::temp_dir();
        desktop_core::codex_broker::CodexBroker::new(&alt)
            .expect("broker must construct from tempdir fallback")
    });
    Arc::new(broker)
}

#[cfg(feature = "private-cloud")]
fn build_wiki_broker() -> Arc<desktop_core::codex_broker::CodexBroker> {
    let wiki_root = wiki_store::default_root();
    let _ = wiki_store::init_wiki(&wiki_root);
    let paths = wiki_store::WikiPaths::resolve(&wiki_root);
    let broker = desktop_core::codex_broker::CodexBroker::new(&paths.meta).unwrap_or_else(|err| {
        eprintln!(
            "private-cloud broker: failed to init from {:?}: {err}",
            paths.meta
        );
        eprintln!("private-cloud broker: falling back to empty in-memory pool");
        desktop_core::codex_broker::CodexBroker::new(&paths.meta)
            .expect("broker second-try must succeed")
    });
    Arc::new(broker)
}

impl Default for AppState {
    fn default() -> Self {
        let (inbox_notify, _) = tokio::sync::broadcast::channel(16);
        install_inbox_notify(inbox_notify.clone());
        Self {
            desktop: DesktopState::default(),
            #[cfg(feature = "private-cloud")]
            broker: build_default_broker(),
            inbox_notify,
            // Default token: every Default instance gets a fresh random
            // value. Primarily used by the test TestServer; production
            // callers go through `new()` or `new_with_shutdown()`.
            shutdown_token: Arc::new(uuid::Uuid::new_v4().to_string()),
            shutdown_cancel: CancellationToken::new(),
        }
    }
}

impl AppState {
    #[must_use]
    pub fn new(desktop: DesktopState) -> Self {
        Self::new_with_shutdown(
            desktop,
            uuid::Uuid::new_v4().to_string(),
            CancellationToken::new(),
        )
    }

    /// Build an `AppState` that shares an externally-owned cancellation
    /// token + shutdown token. The caller keeps a clone of both: the
    /// token so it can `cancel()` on signal reception, and the secret
    /// string so the Tauri shell can authenticate its own shutdown
    /// POST without leaking the credential through a config file.
    #[must_use]
    pub fn new_with_shutdown(
        desktop: DesktopState,
        shutdown_token: String,
        shutdown_cancel: CancellationToken,
    ) -> Self {
        let (inbox_notify, _) = tokio::sync::broadcast::channel(16);
        install_inbox_notify(inbox_notify.clone());
        #[cfg(feature = "private-cloud")]
        let broker = build_wiki_broker();
        #[cfg(feature = "private-cloud")]
        desktop_core::codex_broker::install_global(Arc::clone(&broker));
        Self {
            desktop,
            #[cfg(feature = "private-cloud")]
            broker,
            inbox_notify,
            shutdown_token: Arc::new(shutdown_token),
            shutdown_cancel,
        }
    }

    #[must_use]
    pub fn desktop(&self) -> &DesktopState {
        &self.desktop
    }

    /// Expose the cancel token so the binary can wire up OS-signal
    /// handlers (Ctrl-C / SIGTERM) without duplicating the plumbing.
    #[must_use]
    pub fn shutdown_cancel(&self) -> CancellationToken {
        self.shutdown_cancel.clone()
    }

    #[cfg(feature = "private-cloud")]
    #[must_use]
    pub fn broker(&self) -> &desktop_core::codex_broker::CodexBroker {
        &self.broker
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

type ApiError = (StatusCode, Json<ErrorResponse>);
type ApiResult<T> = Result<T, ApiError>;

// ── Handler naming convention (SG-09) ─────────────────────────────────
//
// Axum handler functions in this file follow two conventions:
//
//  1. Historical handlers (`health`, `bootstrap`, `workbench`, `create_session`,
//     `get_session`, etc.) use the bare noun/verb matching their route.
//     They predate the `_handler` suffix convention.
//
//  2. Newer handlers added after 2025-Q4 use the `_handler` suffix
//     (`delete_session_handler`, `set_permission_mode_handler`,
//     `process_attachment_handler`, ...) to disambiguate them from
//     same-named types in the codebase.
//
// Both styles are considered valid. A mechanical rename of the older
// handlers would touch 30+ files and pollute git blame for no functional
// benefit. New handlers should use the `_handler` suffix.

/// Global body-size ceiling for all HTTP endpoints.
///
/// Set to 15 MiB (slightly above the 10 MiB frontend attachment cap) to
/// accommodate base64-encoded uploads, which inflate payloads by ~33 %.
/// Requests larger than this return 413 without reaching handler code.
const MAX_REQUEST_BODY_BYTES: usize = 15 * 1024 * 1024;

#[cfg(feature = "private-cloud")]
fn install_private_cloud_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/desktop/cloud/codex-accounts",
            get(list_cloud_codex_accounts_handler).post(sync_cloud_codex_accounts_handler),
        )
        .route(
            "/api/desktop/cloud/codex-accounts/clear",
            post(clear_cloud_codex_accounts_handler),
        )
        .route("/api/broker/status", get(broker_status_handler))
}

#[cfg(not(feature = "private-cloud"))]
fn install_private_cloud_routes(router: Router<AppState>) -> Router<AppState> {
    router
}

#[must_use]
pub fn app(state: AppState) -> Router {
    // CORS policy: permissive by design.
    //
    // The desktop server binds to 127.0.0.1 only (see `main.rs` DEFAULT_ADDRESS),
    // so it is not reachable from other hosts. The Tauri shell loads the
    // frontend from a `tauri://` or `http://` origin that varies by platform,
    // so we accept any origin and headers. Do NOT relax this further (e.g.
    // by binding to 0.0.0.0) without re-reviewing this policy.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS]);

    let router = routes::desktop::install(Router::new());
    let router = install_private_cloud_routes(router);
    let router = routes::wiki::install(router);
    let router = routes::wechat::install(router);
    let router = routes::internal::install(router);

    router
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(cors)
        .with_state(state)
}
/// Internal handler for `POST /internal/shutdown`. Auth-gated by a
/// per-process secret (the `X-Shutdown-Token` HTTP header) that the
/// Tauri parent supplies when spawning the child. Success flips the
/// cancel token; `axum::serve()` then drains in-flight requests and
/// returns, which lets Drop impls (incl. `SessionCleanupGuard::drop`)
/// run as the runtime tears down spawned tasks.
///
/// We treat an already-cancelled token as idempotent success — racing
/// a window-close against a Ctrl-C must not panic.
async fn shutdown_handler(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let provided = headers
        .get("x-shutdown-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Constant-time-ish comparison: both strings short and lengths
    // cheap to mismatch on, so byte-wise eq is fine here.
    if provided.is_empty() || provided != state.shutdown_token.as_str() {
        return (
            StatusCode::UNAUTHORIZED,
            "shutdown token missing or invalid",
        )
            .into_response();
    }
    state.shutdown_cancel.cancel();
    (StatusCode::ACCEPTED, "shutdown signalled").into_response()
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

// S0.4 cut day: `code_tool_launch_profile` handler removed along with
// the /api/desktop/code-tools/* routes.

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

/// Helper: validate an optional `project_path` field from a request body.
///
/// Accepts `None` and empty strings (both fall through unchanged). Non-empty
/// values are sent to `desktop_core::validate_project_path` which checks
/// for `..` traversal, canonicalizes, and verifies the path is an existing
/// directory. See S-02 for the original audit motivation.
fn validate_optional_project_path(path: &Option<String>) -> Result<(), ApiError> {
    if let Some(p) = path.as_ref() {
        if !p.is_empty() {
            desktop_core::validate_project_path(p)
                .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;
        }
    }
    Ok(())
}

async fn create_scheduled_task(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopScheduledTaskRequest>,
) -> ApiResult<(StatusCode, Json<DesktopScheduledTaskResponse>)> {
    // IM-02: same validation as create_session. Scheduled tasks carry a
    // project_path that is later used when the task fires, so bad paths
    // would manifest as scheduled-execution errors rather than API errors.
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

/// List workspace skills discovered under `<project_path>/.claude/skills/`.
///
/// Query: `?project_path=<path>`. Returns the same shape the agentic
/// loop uses internally so the frontend can display which skills the
/// LLM has access to in its system prompt.
async fn list_workspace_skills_handler(
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
    // S-02: validate path (no traversal, must exist, must be a directory).
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

/// Process a file attachment (drag-drop upload from the frontend).
///
/// Body schema:
/// ```json
/// {
///   "filename": "report.pdf",
///   "base64": "JVBERi0xLjcK..."
/// }
/// ```
///
/// The file is decoded from base64, dispatched by extension (PDF
/// extraction, image encoding, plain-text decode), and the result
/// is returned for the frontend to inject as message prefix context
/// or a vision block.
async fn process_attachment_handler(
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

    // CR-04: Enforce a strict 10 MiB cap on the *decoded* payload.
    //
    // The global DefaultBodyLimit (15 MiB in `app()`) blocks obvious DoS
    // attempts, but base64 inflates binary content by ~33 %, so a 10 MiB
    // real file comes in as ~13.4 MiB on the wire. Checking the pre-decode
    // string length first lets us reject early without allocating the
    // decoded buffer.
    //
    // Frontend (`InputBar.tsx:validateAttachment`) also enforces a 10 MiB
    // cap on raw file size, so this is a defense-in-depth check for any
    // caller that bypasses the UI (CLI, curl, third-party integrations).
    const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
    // Base64 expands bytes by 4/3, round up: max_b64 = ceil(max * 4 / 3) + pad
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

    // Re-check after decode. A crafted base64 with lots of padding could
    // technically produce more bytes than the char-length check predicted.
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

/// Debug endpoint: initialize MCP from `.claw/settings.json` at the given
/// project path and return the list of discovered tools. Intended for E2E
/// verification that our MCP manager actually connects to configured servers.
async fn debug_mcp_probe_handler(
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
    // S-02: validate path
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

/// Debug endpoint: call an MCP tool by qualified name.
async fn debug_mcp_call_handler(
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

async fn set_permission_mode_handler(
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
    // S-02: validate path
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

async fn get_permission_mode_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // GET is read-only and uses an empty default if no path. Validation is
    // skipped here because the empty case is benign — get_permission_mode
    // returns "default" without touching disk in that case.
    let project_path = params.get("project_path").cloned().unwrap_or_default();
    if !project_path.is_empty() {
        // Validate non-empty paths to fail fast on typos / traversal attempts.
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

// ── Phase 6C: WeChat account management HTTP handlers ──────────────
//
// Lets the frontend drive a full QR-login → monitor-spawn flow from
// the Settings UI without requiring the `desktop-server wechat-login`
// CLI. All real work happens inside DesktopState methods; these
// handlers are thin JSON wrappers.

/// `GET /api/desktop/wechat/accounts` — list persisted WeChat bots
/// with their connection status (connected / disconnected / expired).
async fn list_wechat_accounts_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let accounts = state.desktop.list_wechat_accounts_summary().await;
    let items: Vec<serde_json::Value> = accounts
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "display_name": a.display_name,
                "base_url": a.base_url,
                "bot_token_preview": a.bot_token_preview,
                "last_active_at": a.saved_at,
                "status": a.status.wire_tag(),
            })
        })
        .collect();
    Json(serde_json::json!({ "accounts": items }))
}

/// `DELETE /api/desktop/wechat/accounts/{id}` — stop the monitor and
/// delete credential files from disk. Idempotent.
async fn delete_wechat_account_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .desktop
        .remove_wechat_account(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to remove wechat account `{id}`: {e}"),
                }),
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /api/desktop/wechat/login/start` — fetch a fresh QR code and
/// spawn a background task that waits (up to 5 min) for the user to
/// scan + confirm on their phone. Returns an opaque `handle` the
/// frontend uses for subsequent status polls and cancellation.
async fn start_wechat_login_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let base_url = body
        .get("base_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let (handle, qr_image_content, expires_at) = state
        .desktop
        .start_wechat_login(base_url)
        .await
        .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("start_wechat_login failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "handle": handle,
        "qr_image_base64": qr_image_content,
        "expires_at": expires_at,
    })))
}

/// `GET /api/desktop/wechat/login/{handle}/status` — poll the current
/// state of a pending login. Returns 404 if the handle doesn't exist
/// (either never created, or already garbage-collected).
async fn wechat_login_status_handler(
    State(state): State<AppState>,
    Path(handle): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let snapshot = state
        .desktop
        .wechat_login_status(&handle)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("login handle `{handle}` not found"),
                }),
            )
        })?;
    Ok(Json(serde_json::json!({
        "status": snapshot.status,
        "account_id": snapshot.account_id,
        "error": snapshot.error,
    })))
}

/// `POST /api/desktop/wechat/login/{handle}/cancel` — fire the cancel
/// signal to the background login task. The next status poll will
/// return either `cancelled` or, if the task was already past the
/// point where cancel matters, the final `confirmed`/`failed` state.
async fn cancel_wechat_login_handler(
    State(state): State<AppState>,
    Path(handle): Path<String>,
) -> Json<serde_json::Value> {
    let cancelled = state.desktop.cancel_wechat_login(&handle).await;
    Json(serde_json::json!({ "ok": cancelled }))
}

// ── M5 WeChat bridge: health + group-scope config handlers ────────
//
// Three routes, all wire-compatible with the TypeScript wrappers in
// `apps/desktop-shell/src/lib/tauri.ts`. The health endpoint merges
// monitor-side status (poll timestamps, consecutive failures) with
// handler-side dedupe counters (processed / hits / last ingest).

/// Per-channel health row. Mirrors the `ChannelHealth` struct documented
/// in the M5 contract — the same shape is re-emitted by the codegen
/// under `GeneratedChannelHealth` so the frontend can pin the contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelHealth {
    pub channel: String,
    pub running: bool,
    pub last_poll_unix_ms: Option<i64>,
    pub last_inbound_unix_ms: Option<i64>,
    pub last_ingest_unix_ms: Option<i64>,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub processed_msg_count: u64,
    pub dedupe_hit_count: u64,
}

/// Envelope for `GET /api/wechat/bridge/health`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeHealthResponse {
    pub ilink: ChannelHealth,
    pub kefu: ChannelHealth,
    pub config: desktop_core::wechat_ilink::WeChatIngestConfig,
}

/// `GET /api/wechat/bridge/health` — summarise the connection health
/// of both WeChat bridges. The kefu channel is currently dead code
/// (M5 ships ilink-only); we still surface a disconnected row so the
/// frontend can render a two-column layout.
async fn wechat_bridge_health_handler(State(state): State<AppState>) -> Json<BridgeHealthResponse> {
    // ── ilink: merge every registered monitor into a single row.
    // When multiple accounts run in parallel (rare — most users only
    // bind one bot), pick the most-recently-active monitor as the
    // representative and OR the `running` flag.
    let monitors = state.desktop.wechat_ilink_monitor_statuses().await;
    let mut ilink_running = false;
    let mut last_poll: Option<i64> = None;
    let mut last_inbound: Option<i64> = None;
    let mut consecutive_failures: u32 = 0;
    let mut last_error: Option<String> = None;
    for status in monitors {
        if status.running {
            ilink_running = true;
        }
        match (last_poll, status.last_poll_unix_ms) {
            (_, None) => {}
            (None, Some(v)) => last_poll = Some(v),
            (Some(cur), Some(v)) if v > cur => last_poll = Some(v),
            _ => {}
        }
        match (last_inbound, status.last_inbound_unix_ms) {
            (_, None) => {}
            (None, Some(v)) => last_inbound = Some(v),
            (Some(cur), Some(v)) if v > cur => last_inbound = Some(v),
            _ => {}
        }
        if status.consecutive_failures > consecutive_failures {
            consecutive_failures = status.consecutive_failures;
        }
        if status.last_error.is_some() && last_error.is_none() {
            last_error = status.last_error.clone();
        }
    }

    let dedupe = desktop_core::wechat_ilink::dedupe::global();
    let ilink = ChannelHealth {
        channel: "ilink".to_string(),
        running: ilink_running,
        last_poll_unix_ms: last_poll,
        last_inbound_unix_ms: last_inbound,
        last_ingest_unix_ms: dedupe.last_ingest_ms("ilink"),
        consecutive_failures,
        last_error,
        processed_msg_count: dedupe.processed_count("ilink"),
        dedupe_hit_count: dedupe.hit_count("ilink"),
    };

    // ── kefu: M5 keeps the channel as a dead-code stub. Report
    // disconnected but still let the dedupe counters shine through
    // in case a dev-mode harness exercises the kefu path.
    let kefu = ChannelHealth {
        channel: "kefu".to_string(),
        running: false,
        last_poll_unix_ms: None,
        last_inbound_unix_ms: None,
        last_ingest_unix_ms: dedupe.last_ingest_ms("kefu"),
        consecutive_failures: 0,
        last_error: None,
        processed_msg_count: dedupe.processed_count("kefu"),
        dedupe_hit_count: dedupe.hit_count("kefu"),
    };

    Json(BridgeHealthResponse {
        ilink,
        kefu,
        config: desktop_core::wechat_ilink::ingest_config::read_snapshot(),
    })
}

/// `GET /api/wechat/bridge/config` — return the currently-active
/// group-scope config. Equivalent to reading
/// `~/.clawwiki/wechat_ingest_config.json` but served through the
/// cache so repeated polls are cheap.
async fn wechat_bridge_config_get_handler() -> Json<desktop_core::wechat_ilink::WeChatIngestConfig>
{
    Json(desktop_core::wechat_ilink::ingest_config::read_snapshot())
}

/// `POST /api/wechat/bridge/config` — replace the group-scope config.
/// Body must be a full [`WeChatIngestConfig`] payload (the handler
/// does not support PATCH-style merges). The new value is flushed to
/// disk before the cache swap so a reload always sees the latest.
async fn wechat_bridge_config_post_handler(
    Json(body): Json<desktop_core::wechat_ilink::WeChatIngestConfig>,
) -> Result<
    Json<desktop_core::wechat_ilink::WeChatIngestConfig>,
    (StatusCode, Json<serde_json::Value>),
> {
    match desktop_core::wechat_ilink::ingest_config::update(body) {
        Ok(updated) => Ok(Json(updated)),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("failed to save wechat ingest config: {err}")
            })),
        )),
    }
}

// ── Channel B: Official WeChat Customer Service (kefu) handlers ──

async fn save_kefu_config_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = desktop_core::wechat_kefu::KefuConfig {
        corpid: body["corpid"].as_str().unwrap_or_default().to_string(),
        secret: body["secret"].as_str().unwrap_or_default().to_string(),
        token: body["token"].as_str().unwrap_or_default().to_string(),
        encoding_aes_key: body["encoding_aes_key"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        open_kfid: body["open_kfid"].as_str().map(|s| s.to_string()),
        contact_url: None,
        account_name: body["account_name"].as_str().map(|s| s.to_string()),
        saved_at: None,
        cf_api_token: body["cf_api_token"].as_str().map(|s| s.to_string()),
        worker_url: None,
        relay_ws_url: None,
        relay_auth_token: None,
        callback_url: None,
        callback_token_generated: None,
    };
    state.desktop.save_kefu_config(config).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn load_kefu_config_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.desktop.load_kefu_config().await {
        Ok(Some(config)) => {
            let summary = config.to_summary();
            Json(serde_json::to_value(&summary).unwrap_or_default())
        }
        Ok(None) => Json(serde_json::json!({ "configured": false })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn create_kefu_account_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let name = body["name"].as_str().unwrap_or("ClaudeWiki助手");
    let open_kfid = state.desktop.create_kefu_account(name).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
    })?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "open_kfid": open_kfid,
    })))
}

async fn get_kefu_contact_url_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let url = state.desktop.get_kefu_contact_url().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
    })?;
    Ok(Json(serde_json::json!({ "url": url })))
}

async fn kefu_status_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.desktop.kefu_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn start_kefu_monitor_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.desktop.spawn_kefu_monitor().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn stop_kefu_monitor_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.desktop.stop_kefu_monitor().await;
    Json(serde_json::json!({ "ok": true }))
}

/// GET callback — kf.weixin.qq.com URL verification (echostr decrypt).
async fn kefu_callback_verify_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<String, StatusCode> {
    let config = state
        .desktop
        .load_kefu_config()
        .await
        .map_err(|e| {
            eprintln!("[kefu callback] load config failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            eprintln!("[kefu callback] config not found");
            StatusCode::NOT_FOUND
        })?;

    eprintln!(
        "[kefu callback] verify: corpid={} token_len={} aes_key_len={}",
        config.corpid,
        config.token.len(),
        config.encoding_aes_key.len()
    );

    let callback = desktop_core::wechat_kefu::KefuCallback::new(
        &config.token,
        &config.encoding_aes_key,
        &config.corpid,
    )
    .map_err(|e| {
        eprintln!("[kefu callback] CallbackCrypto::new failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let msg_sig = params
        .get("msg_signature")
        .map(|s| s.as_str())
        .unwrap_or("");
    let timestamp = params.get("timestamp").map(|s| s.as_str()).unwrap_or("");
    let nonce = params.get("nonce").map(|s| s.as_str()).unwrap_or("");
    let echostr = params.get("echostr").map(|s| s.as_str()).unwrap_or("");

    eprintln!(
        "[kefu callback] params: msg_sig_len={} ts={} nonce={} echostr_len={}",
        msg_sig.len(),
        timestamp,
        nonce,
        echostr.len()
    );

    callback
        .verify_echostr(msg_sig, timestamp, nonce, echostr)
        .map_err(|e| {
            eprintln!("[kefu callback] verify failed: {e}");
            StatusCode::FORBIDDEN
        })
}

/// POST callback — receive encrypted event notifications from WeChat.
async fn kefu_callback_event_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: String,
) -> Result<String, StatusCode> {
    let config = state
        .desktop
        .load_kefu_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let callback = desktop_core::wechat_kefu::KefuCallback::new(
        &config.token,
        &config.encoding_aes_key,
        &config.corpid,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let msg_sig = params
        .get("msg_signature")
        .map(|s| s.as_str())
        .unwrap_or("");
    let timestamp = params.get("timestamp").map(|s| s.as_str()).unwrap_or("");
    let nonce = params.get("nonce").map(|s| s.as_str()).unwrap_or("");

    let event = callback
        .decrypt_event(msg_sig, timestamp, nonce, &body)
        .map_err(|e| {
            eprintln!("[kefu callback] decrypt failed: {e}");
            StatusCode::BAD_REQUEST
        })?;

    eprintln!("[kefu callback] event: {event:?}");
    state.desktop.dispatch_kefu_callback(event).await;

    Ok("success".to_string())
}

// ── Pipeline handlers ────────────────────────────────────────────

async fn start_kefu_pipeline_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let skip_cf = body["skip_cf_register"].as_bool().unwrap_or(false);
    let cf_token = body["cf_api_token"].as_str().map(String::from);
    let skip_cb = body["skip_callback_config"].as_bool().unwrap_or(false);
    let corpid = body["corpid"].as_str().map(String::from);
    let secret = body["secret"].as_str().map(String::from);

    state
        .desktop
        .start_kefu_pipeline(skip_cf, cf_token, skip_cb, corpid, secret)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn kefu_pipeline_status_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.desktop.kefu_pipeline_status().await {
        Some(s) => {
            let mut val = serde_json::to_value(&s).unwrap_or_default();
            val["active"] = serde_json::Value::Bool(s.is_active());
            Json(val)
        }
        None => {
            let empty = desktop_core::wechat_kefu::pipeline_types::PipelineState::new();
            let mut val = serde_json::to_value(empty).unwrap_or_default();
            val["active"] = serde_json::Value::Bool(false);
            Json(val)
        }
    }
}

async fn cancel_kefu_pipeline_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.desktop.cancel_kefu_pipeline().await;
    Json(serde_json::json!({ "ok": true }))
}

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
struct IngestRawRequest {
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

fn resolve_wiki_root_for_handler() -> Result<wiki_store::WikiPaths, ApiError> {
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
async fn ingest_wiki_raw_handler(
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
async fn preview_wiki_fetch_handler(
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
struct PreviewFetchRequest {
    url: String,
}

/// `GET /api/wiki/raw`
///
/// List every raw entry, sorted by id ascending. Empty wiki returns
/// `{ entries: [] }` (never errors when the directory is missing).
async fn list_wiki_raw_handler() -> Result<Json<serde_json::Value>, ApiError> {
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
async fn get_wiki_raw_handler(Path(id): Path<u32>) -> Result<Json<serde_json::Value>, ApiError> {
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
struct ResolveInboxRequest {
    /// Either `"approve"` or `"reject"`. Anything else returns 400.
    action: String,
}

async fn list_wiki_inbox_handler() -> Result<Json<serde_json::Value>, ApiError> {
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
struct InboxCandidatesQuery {
    /// When `true`, run the graph-signal enrichment pass after the
    /// preliminary top-3 is chosen. Defaults to `false`.
    #[serde(default)]
    with_graph: bool,
}

async fn list_inbox_candidates_handler(
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
async fn get_wiki_schema_handler() -> Result<Json<serde_json::Value>, ApiError> {
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
async fn put_wiki_schema_handler(
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
struct PutSchemaRequest {
    content: String,
}

/// `GET /api/wiki/pages/{slug}/backlinks` (feat Q)
///
/// Return every concept page that contains a markdown link to
/// `concepts/{slug}.md` in its body. This is the reverse lookup for
/// the bidirectional backlinks system required by canonical §8
/// Triggers row 3 ("A→B implies B→A"). Self-references excluded.
///
/// Returns `{ pages: [...WikiPageSummary] }`.
async fn get_wiki_backlinks_handler(
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
async fn get_wiki_graph_handler() -> Result<Json<serde_json::Value>, ApiError> {
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
async fn get_page_graph_handler(
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
struct WikiLineageQuery {
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
async fn get_wiki_lineage_handler(
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
async fn get_inbox_lineage_handler(
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
async fn get_raw_lineage_handler(
    Path(id): Path<u32>,
) -> Result<Json<wiki_store::provenance::RawLineageResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let resp = wiki_store::provenance::read_lineage_for_raw(&paths, id);
    Ok(Json(resp))
}

async fn resolve_wiki_inbox_handler(
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
struct BatchResolveInboxRequest {
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
struct BatchResolveInboxResponse {
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
struct BatchFailedItem {
    id: u32,
    error: String,
}

async fn batch_resolve_wiki_inbox_handler(
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
async fn propose_wiki_inbox_handler(
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
struct ApproveWithWriteRequest {
    proposal: WikiPageProposalBody,
}

#[derive(Debug, Deserialize)]
struct WikiPageProposalBody {
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
async fn approve_wiki_inbox_with_write_handler(
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
struct InboxMaintainRequest {
    /// `"create_new"` | `"update_existing"` | `"reject"`. Kept as a
    /// free string here so an unknown action returns a friendly 400
    /// instead of a serde parse error.
    action: String,
    /// Required when `action == "update_existing"`.
    #[serde(default)]
    target_page_slug: Option<String>,
    /// Required when `action == "reject"`; min 4 chars.
    #[serde(default)]
    rejection_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct InboxMaintainResponse {
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
async fn inbox_maintain_handler(
    Path(id): Path<u32>,
    Json(body): Json<InboxMaintainRequest>,
) -> Result<Json<InboxMaintainResponse>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;

    // Step 1: translate the flat action into the tagged enum, with
    // strict validation of the per-variant required fields.
    let action = match body.action.as_str() {
        "create_new" => wiki_maintainer::MaintainAction::CreateNew,
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
struct CreateProposalRequest {
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
struct ProposalResponse {
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
struct ApplyProposalResponse {
    outcome: String, // "updated" | "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    target_page_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Response body for `POST /api/wiki/inbox/{id}/proposal/cancel`.
/// Deliberately minimal — the UI just needs a "cancelled or error".
#[derive(Debug, Serialize)]
struct CancelProposalResponse {
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
async fn create_proposal_handler(
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
async fn apply_proposal_handler(
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
async fn cancel_proposal_handler(
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
struct CombinedProposalRequest {
    target_slug: String,
    inbox_ids: Vec<u32>,
}

/// HTTP-layer mirror of [`wiki_maintainer::CombinedProposalResponse`].
/// We duplicate the struct rather than forward the domain type so
/// future wire-shape evolutions (e.g. add `warnings: Vec<String>`)
/// don't couple the crate-internal type to HTTP.
#[derive(Debug, Serialize)]
struct CombinedProposalResponse {
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
struct CombinedProposalSource {
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
struct CombinedApplyRequest {
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
struct CombinedApplyResponse {
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
async fn create_combined_proposal_handler(
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
async fn apply_combined_proposal_handler(
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
async fn list_wiki_pages_handler() -> Result<Json<serde_json::Value>, ApiError> {
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
struct WikiSearchQuery {
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
async fn search_wiki_pages_handler(
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
async fn get_wiki_page_handler(
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
    Ok(Json(serde_json::json!({
        "summary": summary,
        "body": body,
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
async fn ws_wechat_inbox_handler(
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

fn install_inbox_notify(tx: tokio::sync::broadcast::Sender<()>) {
    let _ = INBOX_NOTIFY.set(tx);
}

/// Fire the inbox_notify broadcast so all WS subscribers get an
/// instant notification. Best-effort: if no subscribers exist the
/// send is silently dropped.
fn fire_inbox_notify() {
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
async fn get_wiki_index_handler() -> Result<Json<serde_json::Value>, ApiError> {
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
async fn get_wiki_log_handler() -> Result<Json<serde_json::Value>, ApiError> {
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

// ── ClawWiki S2: Codex broker HTTP handlers ───────────────────────
//
// Thin wrappers around `desktop_core::codex_broker::CodexBroker`. The
// broker itself owns its RwLock / persistence / redaction logic;
// these handlers are ONLY responsible for JSON (de)serialization and
// error mapping.
//
// CRITICAL: none of these handlers return access_token or
// refresh_token. `list_cloud_codex_accounts_handler` returns the
// public view (alias + expires + status); `broker_status_handler`
// returns only aggregate counts. There is no route that exposes a
// single account's full record.

#[cfg(feature = "private-cloud")]
#[derive(Debug, Deserialize)]
struct SyncCloudCodexAccountsRequest {
    accounts: Vec<desktop_core::codex_broker::CloudAccountInput>,
}

#[cfg(feature = "private-cloud")]
async fn sync_cloud_codex_accounts_handler(
    State(state): State<AppState>,
    Json(body): Json<SyncCloudCodexAccountsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let count = body.accounts.len();
    state
        .broker()
        .sync_cloud_accounts(body.accounts)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("broker sync failed: {e}"),
                }),
            )
        })?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "pool_size": count,
    })))
}

#[cfg(feature = "private-cloud")]
async fn list_cloud_codex_accounts_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let accounts = state.broker().list_cloud_accounts();
    Json(serde_json::json!({ "accounts": accounts }))
}

#[cfg(feature = "private-cloud")]
async fn clear_cloud_codex_accounts_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.broker().clear_cloud_accounts().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("broker clear failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(feature = "private-cloud")]
async fn broker_status_handler(
    State(state): State<AppState>,
) -> Json<desktop_core::codex_broker::BrokerPublicStatus> {
    Json(state.broker().public_status())
}

pub async fn serve(state: AppState, address: SocketAddr) -> std::io::Result<()> {
    // The graceful-shutdown future fires when the cancel token owned
    // by `state` is tripped. Tests build a plain `AppState::default()`
    // and never cancel, so this degrades to the prior "run until
    // aborted" behaviour for them.
    let cancel = state.shutdown_cancel.clone();
    serve_with_shutdown(state, address, cancel).await
}

/// Binary entry-point variant. Callers own the cancel token so they
/// can flip it from OS signal handlers. The `state` passed in MUST
/// have been constructed via `AppState::new_with_shutdown(..)` with
/// the same `cancel` — the `/internal/shutdown` HTTP route reads
/// `state.shutdown_cancel` and fires the same clone.
///
/// After `axum::serve` returns (either cancelled or error), we drop
/// the state explicitly. That's what lets owned spawn handles inside
/// `DesktopState` finalize — without the drop, the state would linger
/// in this function's frame until the binary's main() returns, and
/// `SessionCleanupGuard::drop` would never have a chance to run.
pub async fn serve_with_shutdown(
    state: AppState,
    address: SocketAddr,
    cancel: CancellationToken,
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    let shutdown_signal = async move { cancel.cancelled().await };
    let result = axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal)
        .await;
    // `axum::serve` already dropped its own copy of `state` when the
    // graceful-shutdown future resolved; here we've got nothing else
    // to drop explicitly, but the bind-site in `main.rs` is free to
    // drop any owned `DesktopState` clone after we return.
    result
}

#[cfg(test)]
mod tests {
    use super::{
        app, AppState, CreateDesktopSessionResponse, DesktopDispatchItemResponse,
        DesktopDispatchResponse, DesktopScheduledResponse, DesktopScheduledTaskResponse,
        DesktopSessionsResponse, HealthResponse,
    };
    use reqwest::Client;
    use std::env;
    use std::fs;
    use std::net::SocketAddr;
    use std::path::{Path, PathBuf};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    struct TestServer {
        address: SocketAddr,
        /// The `AppState` the server is running with. Cloned from the
        /// one handed to `axum::serve` (both point at the same Arc'd
        /// stores). Integration tests that need to reach into the
        /// TaskManager / session store directly (e.g. pre-seeding
        /// state for a 409 assertion) can use `server.state`.
        state: AppState,
        handle: JoinHandle<()>,
    }

    impl TestServer {
        async fn spawn() -> Self {
            Self::spawn_with_state(AppState::default()).await
        }

        /// Spawn with a caller-provided `AppState`. Lets a test keep
        /// a reference to the state (via the returned `TestServer.state`
        /// field, which is a Clone of the one passed to `app(state)`
        /// — AppState is `Clone` and its fields are `Arc`'d so the
        /// clones share the same underlying stores).
        async fn spawn_with_state(state: AppState) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("test listener should bind");
            let address = listener
                .local_addr()
                .expect("listener should report local address");
            let state_for_server = state.clone();
            let handle = tokio::spawn(async move {
                axum::serve(listener, app(state_for_server))
                    .await
                    .expect("desktop server should run");
            });

            Self {
                address,
                state,
                handle,
            }
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

    struct MockOpenAiServer {
        address: SocketAddr,
        handle: JoinHandle<()>,
    }

    impl MockOpenAiServer {
        async fn spawn(answer: impl Into<String>) -> Self {
            async fn chat_completion(
                axum::extract::State(answer): axum::extract::State<String>,
            ) -> axum::Json<serde_json::Value> {
                axum::Json(serde_json::json!({
                    "id": "chatcmpl_desktop_server_test",
                    "model": "mock-query-model",
                    "choices": [{
                        "message": {
                            "role": "assistant",
                            "content": answer,
                            "tool_calls": []
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 11,
                        "completion_tokens": 5
                    }
                }))
            }

            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("mock provider should bind");
            let address = listener
                .local_addr()
                .expect("mock provider should expose address");
            let router = axum::Router::new()
                .route("/chat/completions", axum::routing::post(chat_completion))
                .with_state(answer.into());
            let handle = tokio::spawn(async move {
                axum::serve(listener, router)
                    .await
                    .expect("mock provider should run");
            });
            Self { address, handle }
        }

        fn base_url(&self) -> String {
            format!("http://{}", self.address)
        }
    }

    impl Drop for MockOpenAiServer {
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

    /// T2.1: `create_session` rejects project_path with `..` traversal.
    #[tokio::test]
    async fn create_session_rejects_traversal_path() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/desktop/sessions"))
            .json(&serde_json::json!({
                "title": "Bad session",
                "project_path": "../../../etc",
            }))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        let err: serde_json::Value = response.json().await.expect("json");
        assert!(
            err["error"].as_str().unwrap_or("").contains(".."),
            "expected traversal error, got: {err}"
        );
    }

    /// T2.1: `create_session` still works with a valid path (empty is OK).
    #[tokio::test]
    async fn create_session_accepts_empty_path() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/desktop/sessions"))
            .json(&serde_json::json!({ "title": "Valid session" }))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    }

    /// T2.2: `create_scheduled_task` rejects traversal in project_path.
    #[tokio::test]
    async fn create_scheduled_task_rejects_traversal_path() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/desktop/scheduled"))
            .json(&serde_json::json!({
                "title": "Bad task",
                "prompt": "do something",
                "project_path": "../../secret",
                "schedule": { "kind": "hourly", "interval_hours": 1 }
            }))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }

    /// T2.3: `forward_permission` returns 400 when requestId is missing.
    #[tokio::test]
    async fn forward_permission_rejects_missing_request_id() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/desktop/sessions/desktop-session-1/permission"))
            .json(&serde_json::json!({ "decision": "allow" }))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        let err: serde_json::Value = response.json().await.expect("json");
        assert!(
            err["error"].as_str().unwrap_or("").contains("requestId"),
            "expected requestId error, got: {err}"
        );
    }

    /// T2.3: `forward_permission` returns 400 when decision is invalid.
    #[tokio::test]
    async fn forward_permission_rejects_invalid_decision() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/desktop/sessions/desktop-session-1/permission"))
            .json(&serde_json::json!({
                "requestId": "req-123",
                "decision": "maybe"
            }))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        let err: serde_json::Value = response.json().await.expect("json");
        assert!(
            err["error"]
                .as_str()
                .unwrap_or("")
                .contains("invalid decision"),
            "expected invalid decision error, got: {err}"
        );
    }

    /// T1.4: attachment handler enforces a 10 MiB decoded-size cap,
    /// rejecting payloads that fit under the global 15 MiB body limit
    /// but would exceed the semantic attachment ceiling.
    #[tokio::test]
    async fn attachment_rejects_oversized_payload() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        // Create ~11 MiB of raw bytes → ~14.7 MiB base64. Under the
        // global 15 MiB limit but over the 10 MiB attachment cap.
        use base64::Engine;
        let raw = vec![b'X'; 11 * 1024 * 1024];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
        let body = serde_json::json!({
            "filename": "toobig.txt",
            "base64": b64,
        });

        let response = client
            .post(server.url("/api/desktop/attachments/process"))
            .json(&body)
            .send()
            .await
            .expect("send oversized attachment");

        assert_eq!(
            response.status(),
            reqwest::StatusCode::PAYLOAD_TOO_LARGE,
            "expected 413 for oversized attachment"
        );

        let error: serde_json::Value = response.json().await.expect("error payload");
        let msg = error["error"].as_str().unwrap_or("");
        assert!(
            msg.contains("too large"),
            "expected 'too large' in error message, got: {msg}"
        );
    }

    /// T1.4: normal-sized attachments still work after the cap was added.
    #[tokio::test]
    async fn attachment_accepts_small_text_payload() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        use base64::Engine;
        let content = b"Hello from the T1.4 test!";
        let b64 = base64::engine::general_purpose::STANDARD.encode(content);
        let body = serde_json::json!({
            "filename": "hello.txt",
            "base64": b64,
        });

        let response = client
            .post(server.url("/api/desktop/attachments/process"))
            .json(&body)
            .send()
            .await
            .expect("send small attachment");

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let payload: serde_json::Value = response.json().await.expect("json");
        assert_eq!(payload["filename"], "hello.txt");
        assert_eq!(payload["kind"], "text");
    }

    /// T0.1: global body-size limit must reject payloads larger than
    /// `MAX_REQUEST_BODY_BYTES` (15 MiB) before they reach handler code.
    /// Without this guard, a malicious local caller could OOM the server.
    ///
    /// Note: Axum's `DefaultBodyLimit` rejection manifests in one of two
    /// ways depending on whether the body is sent streamed or all-at-once:
    ///  1. If Content-Length is present, Axum returns 413 Payload Too Large
    ///  2. If the body is chunked/streamed, the server may close the
    ///     connection mid-stream (ConnectionAborted / reset)
    /// Both indicate successful rejection — the test accepts either.
    #[tokio::test]
    async fn oversized_request_body_rejected() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        // Build a ~16 MiB JSON body — above the 15 MiB ceiling.
        let huge_payload = "A".repeat(16 * 1024 * 1024);
        let body = serde_json::json!({
            "filename": "huge.txt",
            "base64": huge_payload,
        });

        let result = client
            .post(server.url("/api/desktop/attachments/process"))
            .json(&body)
            .send()
            .await;

        match result {
            Ok(response) => {
                // Path 1: clean rejection with status code.
                assert_eq!(
                    response.status(),
                    reqwest::StatusCode::PAYLOAD_TOO_LARGE,
                    "expected 413, got {} — DefaultBodyLimit layer may be missing",
                    response.status()
                );
            }
            Err(_) => {
                // Path 2: server closed the connection mid-body. This is
                // also an acceptable rejection mode — any Err result proves
                // the request did not succeed, which is the invariant we
                // care about. A handler OOM or successful handling would
                // produce an Ok(response) with non-413 status, which
                // the Ok-branch above would reject.
            }
        }
    }

    // ── P0-2 regression: query_wiki SSE payload builders ─────────────
    //
    // `query_wiki_handler` used to emit a payload-less
    // `{ "type": "query_done" }` because the JoinHandle was dropped;
    // the fix extracts the JSON construction into two pure helpers so
    // they can be tested without standing up a broker / mpsc / Sse
    // stream. These tests pin the wire format the frontend
    // `useWikiQuery` depends on.

    use super::{make_query_done_payload, make_query_error_payload};
    use wiki_maintainer::{QueryResult, QuerySource};

    #[test]
    fn query_done_payload_carries_sources_and_total_tokens() {
        let result = QueryResult {
            sources: vec![
                QuerySource {
                    slug: "claude-code-skill-best-practices".to_string(),
                    title: "Claude Code 技能最佳实践".to_string(),
                    relevance_score: 0.87,
                    snippet: "关于如何组织 skill 的指南".to_string(),
                },
                QuerySource {
                    slug: "rust-async".to_string(),
                    title: "Rust 异步入门".to_string(),
                    relevance_score: 0.52,
                    snippet: "Future / Pin / Unpin".to_string(),
                },
            ],
            total_tokens: 1234,
        };
        let payload = make_query_done_payload(&result);
        assert_eq!(payload["type"], "query_done");
        assert_eq!(payload["total_tokens"], 1234);
        let sources = payload["sources"]
            .as_array()
            .expect("sources should be a JSON array");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0]["slug"], "claude-code-skill-best-practices");
        assert_eq!(sources[0]["title"], "Claude Code 技能最佳实践");
        // f32 → JSON Number roundtrip has precision loss; compare within
        // tolerance instead of equality.
        let score = sources[1]["relevance_score"]
            .as_f64()
            .expect("relevance_score should be a number");
        assert!((score - 0.52).abs() < 1e-5, "expected ≈0.52, got {score}");
    }

    #[test]
    fn query_done_payload_keeps_empty_sources_array() {
        let result = QueryResult {
            sources: vec![],
            total_tokens: 0,
        };
        let payload = make_query_done_payload(&result);
        assert_eq!(payload["type"], "query_done");
        assert_eq!(payload["total_tokens"], 0);
        assert!(payload["sources"].is_array());
        assert_eq!(payload["sources"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn query_error_payload_shape() {
        let payload = make_query_error_payload("wiki is empty");
        assert_eq!(payload["type"], "query_error");
        assert_eq!(payload["error"], "wiki is empty");
        // Confirm no accidental `sources` leakage — frontend branches
        // on `type` and shouldn't see a sources field on errors.
        assert!(payload.get("sources").is_none());
        assert!(payload.get("total_tokens").is_none());
    }

    #[test]
    fn query_error_payload_preserves_join_error_message() {
        let payload = make_query_error_payload("query task failed: JoinError::Panic(...)");
        assert!(payload["error"]
            .as_str()
            .unwrap()
            .starts_with("query task failed:"));
    }

    // ── Sprint 1-B.1 · step 7b · absorb pipeline integration tests ──
    //
    // Five end-to-end tests covering the POST /api/wiki/absorb HTTP
    // contract (§2.1) + the three §2.5/§2.6/§2.7 read endpoints
    // (absorb-log / backlinks / stats) in one combined test.
    //
    // Sandbox strategy: each test sets `CLAWWIKI_HOME` to a fresh
    // tempdir. Env vars are process-global, so the `ABSORB_TEST_GUARD`
    // mutex serialises the sandbox-touching tests (cannot run parallel
    // with each other). Other tests in this mod that don't touch
    // `CLAWWIKI_HOME` are unaffected.

    use std::sync::Mutex as StdMutex;

    static ABSORB_TEST_GUARD: StdMutex<()> = StdMutex::new(());

    /// RAII sandbox for the `CLAWWIKI_HOME` env var. On construction
    /// acquires `ABSORB_TEST_GUARD` + points `CLAWWIKI_HOME` at a
    /// fresh tempdir; on `Drop` restores the prior value (if any),
    /// lets the tempdir delete itself, and releases the mutex.
    struct WikiSandbox {
        _tempdir: tempfile::TempDir,
        prev: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl WikiSandbox {
        fn new() -> Self {
            let lock = ABSORB_TEST_GUARD.lock().unwrap_or_else(|p| p.into_inner());
            let tempdir = tempfile::tempdir().expect("tempdir");
            let prev = std::env::var_os("CLAWWIKI_HOME");
            std::env::set_var("CLAWWIKI_HOME", tempdir.path());
            Self {
                _tempdir: tempdir,
                prev,
                _lock: lock,
            }
        }
    }

    impl Drop for WikiSandbox {
        fn drop(&mut self) {
            // Restore BEFORE the tempdir cleans itself up + BEFORE the
            // mutex is released, so parallel tests observe a consistent
            // env var if they race against our teardown.
            if let Some(p) = &self.prev {
                std::env::set_var("CLAWWIKI_HOME", p);
            } else {
                std::env::remove_var("CLAWWIKI_HOME");
            }
        }
    }

    // Sandbox for /api/wiki/query HTTP smoke: temp wiki root plus local providers.json.
    struct WikiProviderSandbox {
        tempdir: tempfile::TempDir,
        prev_clawwiki_home: Option<std::ffi::OsString>,
        prev_cwd: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl WikiProviderSandbox {
        fn new(provider_base_url: &str) -> Self {
            let lock = ABSORB_TEST_GUARD.lock().unwrap_or_else(|p| p.into_inner());
            let tempdir = tempfile::tempdir().expect("tempdir");
            let prev_clawwiki_home = env::var_os("CLAWWIKI_HOME");
            let prev_cwd = env::current_dir().expect("current dir");
            let claw_dir = tempdir.path().join(".claw");
            fs::create_dir_all(&claw_dir).expect(".claw dir");
            fs::write(
                claw_dir.join("providers.json"),
                serde_json::json!({
                    "version": 1,
                    "active": "mock",
                    "providers": {
                        "mock": {
                            "kind": "openai_compat",
                            "display_name": "Mock Query Provider",
                            "base_url": provider_base_url,
                            "api_key": "test-key",
                            "model": "mock-query-model",
                            "max_tokens": 4096
                        }
                    }
                })
                .to_string(),
            )
            .expect("providers config");
            env::set_var("CLAWWIKI_HOME", tempdir.path());
            env::set_current_dir(tempdir.path()).expect("set temp cwd");
            wiki_store::init_wiki(tempdir.path()).expect("init wiki");
            Self {
                tempdir,
                prev_clawwiki_home,
                prev_cwd,
                _lock: lock,
            }
        }

        fn root(&self) -> &Path {
            self.tempdir.path()
        }
    }

    impl Drop for WikiProviderSandbox {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.prev_cwd);
            if let Some(p) = &self.prev_clawwiki_home {
                env::set_var("CLAWWIKI_HOME", p);
            } else {
                env::remove_var("CLAWWIKI_HOME");
            }
        }
    }

    struct WikiNoProviderSandbox {
        tempdir: tempfile::TempDir,
        prev_clawwiki_home: Option<std::ffi::OsString>,
        prev_cwd: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl WikiNoProviderSandbox {
        fn new() -> Self {
            let lock = ABSORB_TEST_GUARD.lock().unwrap_or_else(|p| p.into_inner());
            let tempdir = tempfile::tempdir().expect("tempdir");
            let prev_clawwiki_home = env::var_os("CLAWWIKI_HOME");
            let prev_cwd = env::current_dir().expect("current dir");
            env::set_var("CLAWWIKI_HOME", tempdir.path());
            env::set_current_dir(tempdir.path()).expect("set temp cwd");
            wiki_store::init_wiki(tempdir.path()).expect("init wiki");
            Self {
                tempdir,
                prev_clawwiki_home,
                prev_cwd,
                _lock: lock,
            }
        }

        fn root(&self) -> &Path {
            self.tempdir.path()
        }
    }

    impl Drop for WikiNoProviderSandbox {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.prev_cwd);
            if let Some(p) = &self.prev_clawwiki_home {
                env::set_var("CLAWWIKI_HOME", p);
            } else {
                env::remove_var("CLAWWIKI_HOME");
            }
        }
    }

    #[tokio::test]
    async fn query_wiki_http_smoke_streams_chunks_done_and_sources() {
        let provider = MockOpenAiServer::spawn("Transformer mock answer").await;
        let sandbox = WikiProviderSandbox::new(&provider.base_url());
        let paths = wiki_store::WikiPaths::resolve(sandbox.root());
        wiki_store::write_wiki_page_in_category(
            &paths,
            "concept",
            "transformer",
            "Transformer Architecture",
            "Self-attention based neural network.",
            "# Transformer\n\nA Transformer uses self-attention to process sequences.",
            Some(1),
        )
        .expect("seed wiki page");

        let server = TestServer::spawn().await;
        let client = Client::new();
        let response = client
            .post(server.url("/api/wiki/query"))
            .json(&serde_json::json!({
                "question": "What is a Transformer?",
                "max_sources": 5
            }))
            .send()
            .await
            .expect("query POST");

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body = response.text().await.expect("SSE body");
        assert!(
            body.contains("event: skill"),
            "expected skill SSE events: {body}"
        );
        assert!(
            body.contains("\"type\":\"query_chunk\""),
            "missing query chunk: {body}"
        );
        assert!(
            body.contains("Transformer mock answer"),
            "mock provider answer should stream through: {body}"
        );
        assert!(
            body.contains("\"type\":\"query_done\""),
            "missing query_done: {body}"
        );
        assert!(
            body.contains("\"slug\":\"transformer\""),
            "missing source slug: {body}"
        );
        assert!(
            body.contains("\"title\":\"Transformer Architecture\""),
            "missing source title: {body}"
        );
    }

    #[tokio::test]
    async fn proposal_http_smoke_creates_and_applies_merge() {
        let merged_markdown =
            "# Attention\n\nOriginal notes.\n\n## Multi-head update\n\nMerged detail.";
        let provider = MockOpenAiServer::spawn(
            serde_json::json!({
                "after_markdown": merged_markdown,
                "summary": "added multi-head update"
            })
            .to_string(),
        )
        .await;
        let sandbox = WikiProviderSandbox::new(&provider.base_url());
        let paths = wiki_store::WikiPaths::resolve(sandbox.root());
        wiki_store::write_wiki_page_in_category(
            &paths,
            "concept",
            "attention",
            "Attention",
            "Attention summary.",
            "# Attention\n\nOriginal notes.",
            None,
        )
        .expect("seed wiki page");
        let raw = wiki_store::write_raw_entry(
            &paths,
            "paste",
            "attention-update",
            "New source note: multi-head attention lets several attention heads run in parallel.",
            &wiki_store::RawFrontmatter::for_paste("paste", None),
        )
        .expect("seed raw entry");
        let inbox =
            wiki_store::append_new_raw_task(&paths, &raw, "test").expect("seed inbox entry");

        let server = TestServer::spawn().await;
        let client = Client::new();
        let proposal_response = client
            .post(server.url(&format!("/api/wiki/inbox/{}/proposal", inbox.id)))
            .json(&serde_json::json!({ "target_slug": "attention" }))
            .send()
            .await
            .expect("proposal POST");

        assert_eq!(proposal_response.status(), reqwest::StatusCode::OK);
        let proposal: serde_json::Value = proposal_response.json().await.expect("proposal body");
        assert_eq!(proposal["target_slug"], "attention");
        assert_eq!(proposal["summary"], "added multi-head update");
        assert_eq!(proposal["after_markdown"], merged_markdown);

        let staged = wiki_store::list_inbox_entries(&paths)
            .expect("list inbox after proposal")
            .into_iter()
            .find(|entry| entry.id == inbox.id)
            .expect("staged inbox");
        assert_eq!(staged.proposal_status.as_deref(), Some("pending"));
        assert_eq!(staged.target_page_slug.as_deref(), Some("attention"));
        assert_eq!(
            staged.proposed_after_markdown.as_deref(),
            Some(merged_markdown)
        );

        let apply_response = client
            .post(server.url(&format!("/api/wiki/inbox/{}/proposal/apply", inbox.id)))
            .send()
            .await
            .expect("proposal apply POST");

        assert_eq!(apply_response.status(), reqwest::StatusCode::OK);
        let applied: serde_json::Value = apply_response.json().await.expect("apply body");
        assert_eq!(applied["outcome"], "updated");
        assert_eq!(applied["target_page_slug"], "attention");

        let (_summary, body) =
            wiki_store::read_wiki_page(&paths, "attention").expect("read updated wiki page");
        assert!(body.contains("## Multi-head update"));

        let applied_entry = wiki_store::list_inbox_entries(&paths)
            .expect("list inbox after apply")
            .into_iter()
            .find(|entry| entry.id == inbox.id)
            .expect("applied inbox");
        assert_eq!(applied_entry.status, wiki_store::InboxStatus::Approved);
        assert_eq!(applied_entry.proposal_status.as_deref(), Some("applied"));
        assert!(applied_entry.proposed_after_markdown.is_none());
        assert_eq!(
            applied_entry.proposal_summary.as_deref(),
            Some("added multi-head update")
        );
    }

    /// POST /api/wiki/absorb happy path returns 202 plus a canonical task id.
    #[tokio::test]
    async fn absorb_returns_202_with_task_id_for_empty_entry_ids() {
        let _sandbox = WikiSandbox::new();
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/wiki/absorb"))
            .json(&serde_json::json!({ "entry_ids": [] }))
            .send()
            .await
            .expect("absorb POST");

        assert_eq!(
            response.status(),
            reqwest::StatusCode::ACCEPTED,
            "expected 202 Accepted per §2.1"
        );
        let body: serde_json::Value = response.json().await.expect("body");
        assert_eq!(body["status"], "started");
        assert_eq!(body["total_entries"], 0);
        let task_id = body["task_id"].as_str().expect("task_id string");
        assert!(
            task_id.starts_with("absorb-"),
            "task_id must begin with 'absorb-', got: {task_id}"
        );
        let parts: Vec<&str> = task_id.split('-').collect();
        assert_eq!(parts.len(), 3, "task_id shape: {task_id}");
        assert!(parts[1].parse::<u64>().is_ok(), "ts segment: {}", parts[1]);
        assert_eq!(parts[2].len(), 4, "hex segment length: {}", parts[2]);
        assert!(parts[2].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn absorb_returns_503_when_provider_missing_for_non_empty_batch() {
        let sandbox = WikiNoProviderSandbox::new();
        let paths = wiki_store::WikiPaths::resolve(sandbox.root());
        let raw = wiki_store::write_raw_entry(
            &paths,
            "paste",
            "provider-required",
            "A raw note that needs the maintainer model before it can become wiki content.",
            &wiki_store::RawFrontmatter::for_paste("paste", None),
        )
        .expect("seed raw entry");
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/wiki/absorb"))
            .json(&serde_json::json!({ "entry_ids": [raw.id] }))
            .send()
            .await
            .expect("absorb POST");

        assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = response.json().await.expect("body");
        assert!(
            body["error"]
                .as_str()
                .expect("error string")
                .starts_with("BROKER_UNAVAILABLE:"),
            "unexpected error body: {body}"
        );
    }

    #[tokio::test]
    async fn absorb_events_stream_receives_global_absorb_complete() {
        let server = TestServer::spawn().await;
        let client = Client::new();

        let mut response = client
            .get(server.url("/api/wiki/absorb/events"))
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await
            .expect("absorb events stream should connect");

        assert_eq!(response.status(), reqwest::StatusCode::OK);

        server
            .state
            .desktop()
            .broadcast_session_event(desktop_core::DesktopSessionEvent::AbsorbComplete {
                task_id: "absorb-test-stream".to_string(),
                created: 1,
                updated: 0,
                skipped: 0,
                failed: 0,
                duration_ms: 7,
            })
            .await;

        let mut body = String::new();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !body.contains("\"task_id\":\"absorb-test-stream\"") {
                let chunk = response
                    .chunk()
                    .await
                    .expect("stream chunk read should not error")
                    .expect("stream should yield absorb event");
                body.push_str(&String::from_utf8_lossy(&chunk));
            }
        })
        .await
        .expect("absorb event should arrive quickly");

        assert!(body.contains("event: absorb_complete"));
        assert!(body.contains("\"type\":\"absorb_complete\""));
    }

    /// Test 2 / 5 — POST /api/wiki/absorb returns `409 Conflict` +
    /// `ABSORB_IN_PROGRESS` when another "absorb" task is already in
    /// the TaskManager registry. Pre-registers the slot directly via
    /// the shared `AppState` so the race is deterministic (no timing).
    #[tokio::test]
    async fn absorb_returns_409_when_another_absorb_task_is_running() {
        let _sandbox = WikiSandbox::new();
        let server = TestServer::spawn().await;
        let client = Client::new();

        // Pre-acquire the "absorb" kind slot so the next register call
        // from the handler must fail.
        let (pre_task_id, _cancel_token) = server
            .state
            .desktop()
            .task_manager()
            .register("absorb")
            .await
            .expect("pre-register absorb kind");

        let response = client
            .post(server.url("/api/wiki/absorb"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("absorb POST");

        assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
        let body: serde_json::Value = response.json().await.expect("body");
        assert_eq!(body["error"], "ABSORB_IN_PROGRESS");

        // Release the synthetic registration so the sandbox tears
        // down cleanly. (The real handler's spawn would call this via
        // the task_manager.complete at the end of absorb_batch; our
        // pre-register never runs a task body.)
        server
            .state
            .desktop()
            .task_manager()
            .complete(&pre_task_id)
            .await;
    }

    /// Test 3 / 5 — POST /api/wiki/absorb returns `400 Bad Request` +
    /// `INVALID_DATE_RANGE` on both malformed dates and ordering
    /// violations (from > to).
    #[tokio::test]
    async fn absorb_returns_400_on_invalid_date_range() {
        let _sandbox = WikiSandbox::new();
        let server = TestServer::spawn().await;
        let client = Client::new();

        // Malformed (non YYYY-MM-DD shape).
        let response = client
            .post(server.url("/api/wiki/absorb"))
            .json(&serde_json::json!({
                "date_range": { "from": "not-a-date", "to": "2026-04-23" }
            }))
            .send()
            .await
            .expect("POST");
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json().await.expect("body");
        assert_eq!(body["error"], "INVALID_DATE_RANGE");

        // from > to ordering violation.
        let response = client
            .post(server.url("/api/wiki/absorb"))
            .json(&serde_json::json!({
                "date_range": { "from": "2026-04-30", "to": "2026-04-01" }
            }))
            .send()
            .await
            .expect("POST");
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json().await.expect("body");
        assert_eq!(body["error"], "INVALID_DATE_RANGE");
    }

    /// Test 4 / 5 — POST /api/wiki/absorb returns `404 Not Found` +
    /// `ENTRIES_NOT_FOUND` when an explicit `entry_ids` list contains
    /// ids that don't exist on disk. The fresh sandbox has zero raw
    /// entries, so any explicit id is automatically "missing".
    #[tokio::test]
    async fn absorb_returns_404_for_missing_entry_ids() {
        let _sandbox = WikiSandbox::new();
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/wiki/absorb"))
            .json(&serde_json::json!({ "entry_ids": [9999, 8888] }))
            .send()
            .await
            .expect("POST");

        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
        let body: serde_json::Value = response.json().await.expect("body");
        let err = body["error"].as_str().unwrap_or("");
        assert!(
            err.starts_with("ENTRIES_NOT_FOUND"),
            "expected ENTRIES_NOT_FOUND prefix, got: {err}"
        );
        // At least one missing id should be named in the error.
        assert!(
            err.contains("9999") || err.contains("8888"),
            "error should mention a missing id: {err}"
        );
    }

    /// Test 5 / 5 — GET /api/wiki/absorb-log + /api/wiki/backlinks +
    /// /api/wiki/stats all return their canonical shapes on a fresh
    /// wiki root (empty arrays / zero counters).
    #[tokio::test]
    async fn absorb_log_backlinks_stats_endpoints_return_canonical_shapes() {
        let _sandbox = WikiSandbox::new();
        let server = TestServer::spawn().await;
        let client = Client::new();

        // §2.5 GET /api/wiki/absorb-log — wrapped {entries, total}.
        let response = client
            .get(server.url("/api/wiki/absorb-log"))
            .send()
            .await
            .expect("GET absorb-log");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("json");
        assert!(
            body["entries"].is_array(),
            "absorb-log.entries must be array: {body}"
        );
        assert_eq!(body["entries"].as_array().unwrap().len(), 0);
        assert_eq!(body["total"], 0);

        // §2.6 GET /api/wiki/backlinks?slug=foo — per-slug shape.
        let response = client
            .get(server.url("/api/wiki/backlinks?slug=foo"))
            .send()
            .await
            .expect("GET backlinks");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("json");
        assert_eq!(body["slug"], "foo");
        assert!(body["backlinks"].is_array());
        assert_eq!(body["backlinks"].as_array().unwrap().len(), 0);
        assert_eq!(body["count"], 0);

        // §2.7 GET /api/wiki/stats — WikiStats with all 16 fields.
        let response = client
            .get(server.url("/api/wiki/stats"))
            .send()
            .await
            .expect("GET stats");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let stats: serde_json::Value = response.json().await.expect("json");
        // Spot-check all 16 §3.9 fields are present + of correct kind.
        for key in [
            "raw_count",
            "wiki_count",
            "concept_count",
            "people_count",
            "topic_count",
            "compare_count",
            "edge_count",
            "orphan_count",
            "inbox_pending",
            "inbox_resolved",
            "today_ingest_count",
            "week_new_pages",
            "avg_page_words",
        ] {
            assert!(
                stats[key].is_number(),
                "stats.{key} must be number: {stats}"
            );
        }
        assert!(stats["absorb_success_rate"].is_number());
        assert!(stats["knowledge_velocity"].is_number());
        assert!(stats["last_absorb_at"].is_null());
    }
}

// ── Multi-provider registry handlers (generic compatible gateways) ──

// ═══════════════════════════════════════════════════════════════
// Storage migration handler
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct MigrateStorageRequest {
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
async fn migrate_storage_handler(
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
async fn markitdown_check_handler() -> Json<serde_json::Value> {
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
struct MarkItDownConvertRequest {
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
async fn markitdown_convert_handler(
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
struct WechatFetchRequest {
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
async fn wechat_fetch_handler(
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
async fn wechat_fetch_check_handler() -> Json<serde_json::Value> {
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
struct RecentIngestQuery {
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
async fn recent_ingest_handler(
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
async fn node_check_handler() -> Json<serde_json::Value> {
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
async fn opencli_check_handler() -> Json<serde_json::Value> {
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
async fn chromium_check_handler() -> Json<serde_json::Value> {
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
struct InstallDepsRequest {
    #[serde(default = "default_pkg_all")]
    package: String,
}
fn default_pkg_all() -> String {
    "all".to_string()
}

async fn install_python_deps_handler(
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

async fn delete_wiki_raw_handler(Path(id): Path<u32>) -> Result<Json<serde_json::Value>, ApiError> {
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

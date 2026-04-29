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
pub(crate) use handlers::desktop_storage::{
    chromium_check_handler, install_python_deps_handler, markitdown_check_handler,
    markitdown_convert_handler, migrate_storage_handler, node_check_handler, opencli_check_handler,
    recent_ingest_handler, wechat_fetch_check_handler, wechat_fetch_handler,
};
pub(crate) use handlers::desktop_utilities::{
    bootstrap, create_dispatch_item, create_scheduled_task, customize, debug_mcp_call_handler,
    debug_mcp_probe_handler, delete_dispatch_item_handler, delete_scheduled_task_handler,
    deliver_dispatch_item, dispatch, get_permission_mode_handler, health,
    list_workspace_skills_handler, process_attachment_handler, run_scheduled_task_now, scheduled,
    set_permission_mode_handler, settings, update_dispatch_item, update_dispatch_item_status,
    update_scheduled_task, update_scheduled_task_enabled, workbench,
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
pub(crate) use handlers::wechat::{
    cancel_kefu_pipeline_handler, cancel_wechat_login_handler, create_kefu_account_handler,
    delete_wechat_account_handler, get_kefu_contact_url_handler, kefu_callback_event_handler,
    kefu_callback_verify_handler, kefu_pipeline_status_handler, kefu_status_handler,
    list_wechat_accounts_handler, list_wechat_outbox_handler, load_kefu_config_handler,
    save_kefu_config_handler, start_kefu_monitor_handler, start_kefu_pipeline_handler,
    start_wechat_login_handler, stop_kefu_monitor_handler, wechat_bridge_config_get_handler,
    wechat_bridge_config_post_handler, wechat_bridge_health_handler, wechat_health_handler,
    wechat_login_status_handler,
};
pub use handlers::wechat::{BridgeHealthResponse, ChannelHealth};
pub(crate) use handlers::wiki_crud::{
    add_external_ai_write_grant_handler, apply_combined_proposal_handler, apply_proposal_handler,
    approve_wiki_inbox_with_write_handler, batch_resolve_wiki_inbox_handler,
    cancel_proposal_handler, commit_vault_git_handler, create_combined_proposal_handler,
    create_proposal_handler, delete_wiki_raw_handler, discard_vault_git_hunk_handler,
    discard_vault_git_line_handler, discard_vault_git_path_handler,
    get_external_ai_write_policy_handler, get_inbox_lineage_handler, get_page_graph_handler,
    get_raw_lineage_handler, get_rules_file_handler, get_vault_git_audit_handler,
    get_vault_git_diff_handler, get_vault_git_status_handler, get_wiki_backlinks_handler,
    get_wiki_graph_handler, get_wiki_index_handler, get_wiki_lineage_handler, get_wiki_log_handler,
    get_wiki_page_handler, get_wiki_raw_handler, get_wiki_schema_handler, inbox_maintain_handler,
    ingest_wiki_raw_handler, install_inbox_notify, list_inbox_candidates_handler,
    list_wiki_inbox_handler, list_wiki_pages_handler, list_wiki_raw_handler,
    preview_wiki_fetch_handler, propose_wiki_inbox_handler, pull_vault_git_handler,
    push_vault_git_handler, put_rules_file_handler, put_wiki_page_handler, put_wiki_schema_handler,
    resolve_wiki_inbox_handler, resolve_wiki_root_for_handler,
    revoke_external_ai_write_grant_handler, search_wiki_pages_handler,
    set_vault_git_remote_handler, ws_wechat_inbox_handler,
};
pub(crate) use handlers::wiki_reports::{
    breakdown_handler, cleanup_handler, get_absorb_log_handler, get_backlinks_index_handler,
    get_guidance_files_handler, get_patrol_report_handler, get_policy_files_handler,
    get_schema_templates_handler, get_stats_handler, patrol_handler,
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

    #[tokio::test]
    async fn cleanup_preview_returns_patrol_shape_without_writing_inbox() {
        let _sandbox = WikiNoProviderSandbox::new();
        let server = TestServer::spawn().await;
        let client = Client::new();

        let response = client
            .post(server.url("/api/wiki/cleanup?apply=false"))
            .send()
            .await
            .expect("cleanup preview POST");

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("cleanup body");
        assert_eq!(body["applied"], false);
        assert_eq!(body["inbox_created"], 0);
        assert!(body["issues"].is_array(), "issues array missing: {body}");
        assert!(
            body["cleanup_proposals"].is_array(),
            "cleanup proposal array missing: {body}"
        );
        assert!(body["summary"].is_object(), "summary missing: {body}");
    }

    #[tokio::test]
    async fn breakdown_preview_and_apply_write_split_targets() {
        let sandbox = WikiNoProviderSandbox::new();
        let paths = wiki_store::WikiPaths::resolve(sandbox.root());
        let section_a = "Alpha planning note ".repeat(35);
        let section_b = "Beta operations note ".repeat(35);
        let body = format!(
            "# Phase 4 Source\n\n## Alpha\n\n{}\n\n## Beta\n\n{}",
            section_a, section_b
        );
        wiki_store::write_wiki_page_in_category(
            &paths,
            "concept",
            "phase4-source",
            "Phase 4 Source",
            "A mixed page used by the breakdown HTTP smoke test.",
            &body,
            None,
        )
        .expect("seed source wiki page");

        let server = TestServer::spawn().await;
        let client = Client::new();
        let preview = client
            .post(server.url("/api/wiki/breakdown"))
            .json(&serde_json::json!({
                "slug": "phase4-source",
                "apply": false,
                "max_targets": 4
            }))
            .send()
            .await
            .expect("breakdown preview POST");

        assert_eq!(preview.status(), reqwest::StatusCode::OK);
        let preview_body: serde_json::Value = preview.json().await.expect("preview body");
        assert_eq!(preview_body["applied"], false);
        let preview_targets = preview_body["targets"].as_array().expect("targets array");
        assert_eq!(
            preview_targets.len(),
            2,
            "unexpected preview: {preview_body}"
        );

        let apply = client
            .post(server.url("/api/wiki/breakdown"))
            .json(&serde_json::json!({
                "slug": "phase4-source",
                "apply": true,
                "max_targets": 4
            }))
            .send()
            .await
            .expect("breakdown apply POST");

        assert_eq!(apply.status(), reqwest::StatusCode::OK);
        let apply_body: serde_json::Value = apply.json().await.expect("apply body");
        assert_eq!(apply_body["applied"], true);
        let written_paths = apply_body["written_paths"]
            .as_array()
            .expect("written paths array");
        assert_eq!(written_paths.len(), 2, "unexpected apply: {apply_body}");
        let target_slug = apply_body["targets"][0]["slug"]
            .as_str()
            .expect("target slug");
        let (_summary, target_body) =
            wiki_store::read_wiki_page(&paths, target_slug).expect("read split target");
        assert!(target_body.contains("Split from [Phase 4 Source]"));
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

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
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

// S0.4 cut day: `mod code_tools_bridge` is gone along with the
// `/api/desktop/code-tools/*` routes. ClawWiki canonical §11.1 cut #3
// — there is no /code page, no CLI launcher, no claude-bridge proxy.

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    desktop: DesktopState,
    // S2: internal Codex account pool (canonical §9.2). Shared across
    // handlers via Arc because AppState is Clone'd into every request.
    // `Arc<CodexBroker>` — the inner RwLock + AtomicU64 handle their
    // own sync; we never need a Mutex around the whole thing.
    broker: Arc<desktop_core::codex_broker::CodexBroker>,
}

impl Default for AppState {
    fn default() -> Self {
        // Tests + in-process construction fall through here. We can't
        // `unwrap()` broker init because the filesystem may be locked
        // (e.g. Windows AV sandbox during CI), so fall back to an
        // in-memory broker rooted at a tempdir. The tempdir lives
        // until the process exits.
        let fallback = std::env::temp_dir().join(format!(
            "warwolf-broker-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&fallback);
        let broker = desktop_core::codex_broker::CodexBroker::new(&fallback)
            .unwrap_or_else(|_| {
                // This can only fail if secure_storage can't read/write
                // the key file AND we somehow got a partial tempdir.
                // Fall back to an even simpler tempdir.
                let alt = std::env::temp_dir();
                desktop_core::codex_broker::CodexBroker::new(&alt)
                    .expect("broker must construct from tempdir fallback")
            });
        Self {
            desktop: DesktopState::default(),
            broker: Arc::new(broker),
        }
    }
}

impl AppState {
    #[must_use]
    pub fn new(desktop: DesktopState) -> Self {
        // Use the canonical `~/.clawwiki/.clawwiki/` meta directory
        // resolved through wiki_store so the broker's encrypted blob
        // lives alongside the rest of the wiki metadata.
        let wiki_root = wiki_store::default_root();
        let _ = wiki_store::init_wiki(&wiki_root);
        let paths = wiki_store::WikiPaths::resolve(&wiki_root);
        let broker = desktop_core::codex_broker::CodexBroker::new(&paths.meta)
            .unwrap_or_else(|err| {
                eprintln!("codex_broker: failed to init from {:?}: {err}", paths.meta);
                eprintln!("codex_broker: falling back to empty in-memory pool");
                // Construct with a non-existent path so any `new` that
                // tries to reload finds nothing. `sync` calls later
                // will still work because `persist` recreates parents.
                desktop_core::codex_broker::CodexBroker::new(&paths.meta)
                    .expect("broker second-try must succeed")
            });
        let broker_arc = Arc::new(broker);
        // A.2: install as the process-global so desktop-core's
        // `execute_live_turn` free-function can consult it without
        // having to thread an AppState handle through the session
        // runtime. First install wins; tests that construct a second
        // AppState silently skip.
        desktop_core::codex_broker::install_global(Arc::clone(&broker_arc));
        Self {
            desktop,
            broker: broker_arc,
        }
    }

    #[must_use]
    pub fn desktop(&self) -> &DesktopState {
        &self.desktop
    }

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

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
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
        // S0.4 cut day: /api/desktop/code-tools/* routes are gone.
        // (launch-profile + claude-bridge passthrough). No /code page,
        // no CLI launcher in canonical ClawWiki §11.1 cut #3.
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
        .route("/api/desktop/sessions/{id}/compact", post(compact_session))
        .route("/api/desktop/sessions/{id}/fork", post(fork_session))
        .route("/api/desktop/sessions/{id}/lifecycle", post(set_session_lifecycle_handler))
        .route("/api/desktop/sessions/{id}/flag", post(set_session_flag_handler))
        .route("/api/desktop/attachments/process", post(process_attachment_handler))
        .route("/api/desktop/skills", get(list_workspace_skills_handler))
        .route("/api/desktop/settings/permission-mode", post(set_permission_mode_handler).get(get_permission_mode_handler))
        // SG-01: Debug routes are intended for development + QA only.
        // They are always registered so the release binary can still help
        // diagnose MCP issues, but production deployments should keep the
        // server bound to 127.0.0.1 only (see `main.rs` DEFAULT_ADDRESS).
        // A future hardening step could gate these behind an env flag
        // (`OCL_ENABLE_DEBUG=1`) if the server is ever exposed beyond
        // localhost — they expose no secrets and run under the same
        // validate_project_path() guard as other handlers.
        .route("/api/desktop/debug/mcp/probe", post(debug_mcp_probe_handler))
        .route("/api/desktop/debug/mcp/call", post(debug_mcp_call_handler))
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
        // ── Phase 3-5 multi-provider routes: DELETED on S0.4 cut day ──
        // The 5 routes (`/api/desktop/providers` GET/POST/DELETE/{id}/
        // activate / templates / test) along with their handlers were
        // removed when `desktop_core::providers_config` was deleted.
        // ClawWiki canonical §11.1 cut #6 — single Codex pool, no user
        // provider picker, no API-key paste form. The S2 codex_broker
        // module will own this surface (and will not expose any HTTP).
        // ── ClawWiki S2 Codex broker routes ────────────────────────
        // Per canonical §9.2 the broker pool lives inside the Rust
        // process — these four routes never return tokens, only a
        // redacted view + aggregate counts.
        .route(
            "/api/desktop/cloud/codex-accounts",
            get(list_cloud_codex_accounts_handler)
                .post(sync_cloud_codex_accounts_handler),
        )
        .route(
            "/api/desktop/cloud/codex-accounts/clear",
            post(clear_cloud_codex_accounts_handler),
        )
        .route("/api/broker/status", get(broker_status_handler))
        // ── ClawWiki S1 wiki/raw layer routes ──────────────────────
        // The "raw" layer is the immutable facts directory under
        // `~/.clawwiki/raw/`. Per canonical §10 every WeChat-ingested
        // article / paste / URL lands here exactly once and is never
        // mutated; the wiki_maintainer agent (S4) reads it and produces
        // wiki/ pages on top.
        .route(
            "/api/wiki/raw",
            get(list_wiki_raw_handler).post(ingest_wiki_raw_handler),
        )
        .route("/api/wiki/raw/{id}", get(get_wiki_raw_handler))
        // ── ClawWiki S4 inbox layer routes ─────────────────────────
        // Maintainer-proposed tasks that need user approval. S4 MVP:
        // raw-ingest side-effect auto-appends a `new-raw` task; future
        // sprints add conflict / stale / deprecate kinds once the
        // maintainer LLM runs.
        .route("/api/wiki/inbox", get(list_wiki_inbox_handler))
        .route(
            "/api/wiki/inbox/{id}/resolve",
            post(resolve_wiki_inbox_handler),
        )
        // ── ClawWiki S4 maintainer MVP (engram-style) ──────────────
        // `propose` fires one chat_completion against the Codex pool
        // via the wiki_maintainer crate and returns a JSON proposal
        // without touching disk. `approve-with-write` is the
        // human-confirmed follow-up: it writes the concept page to
        // `wiki/concepts/{slug}.md` and flips the inbox entry to
        // `approved` atomically. Canonical §4 blade 3.
        .route(
            "/api/wiki/inbox/{id}/propose",
            post(propose_wiki_inbox_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/approve-with-write",
            post(approve_wiki_inbox_with_write_handler),
        )
        // ── ClawWiki S4 wiki concept pages (read) ──────────────────
        // Pure read routes for the Wiki tab and the InboxPage diff
        // preview. Writes ONLY happen through the propose →
        // approve-with-write flow above, so no POST /api/wiki/pages
        // exists (the plan had one; review trimmed it because there's
        // no UI producing standalone pages without a raw-entry
        // seed — that's a follow-up sprint).
        .route("/api/wiki/pages", get(list_wiki_pages_handler))
        .route("/api/wiki/pages/{slug}", get(get_wiki_page_handler))
        // ── ClawWiki S6 schema layer (read-only) ───────────────────
        // Returns the text of `schema/CLAUDE.md` so the SchemaEditorPage
        // can render the maintainer agent's rule book. Per canonical
        // §8 / §10, schema/ is human-owned: the maintainer agent may
        // PROPOSE changes via the Inbox but never writes here directly,
        // and neither does this HTTP route (no PUT/POST).
        .route("/api/wiki/schema", get(get_wiki_schema_handler))
        // ── Phase 6C: WeChat account management ────────────────────
        .route(
            "/api/desktop/wechat/accounts",
            get(list_wechat_accounts_handler),
        )
        .route(
            "/api/desktop/wechat/accounts/{id}",
            delete(delete_wechat_account_handler),
        )
        .route(
            "/api/desktop/wechat/login/start",
            post(start_wechat_login_handler),
        )
        .route(
            "/api/desktop/wechat/login/{handle}/status",
            get(wechat_login_status_handler),
        )
        .route(
            "/api/desktop/wechat/login/{handle}/cancel",
            post(cancel_wechat_login_handler),
        )
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
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

/// Helper: validate an optional `project_path` field from a request body.
///
/// Accepts `None` and empty strings (both fall through unchanged). Non-empty
/// values are sent to `desktop_core::validate_project_path` which checks
/// for `..` traversal, canonicalizes, and verifies the path is an existing
/// directory. See S-02 for the original audit motivation.
fn validate_optional_project_path(path: &Option<String>) -> Result<(), ApiError> {
    if let Some(p) = path.as_ref() {
        if !p.is_empty() {
            desktop_core::validate_project_path(p).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse { error: e }),
                )
            })?;
        }
    }
    Ok(())
}

async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopSessionRequest>,
) -> ApiResult<(StatusCode, Json<CreateDesktopSessionResponse>)> {
    // IM-01: reject path traversal / nonexistent project paths at the edge,
    // before a session is created and persisted with bad metadata.
    validate_optional_project_path(&payload.project_path)?;

    let session = state.desktop().create_session(payload).await;
    Ok((
        StatusCode::CREATED,
        Json(CreateDesktopSessionResponse { session }),
    ))
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

async fn fork_session(
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

/// Update a session's lifecycle status (Todo / InProgress / NeedsReview
/// / Done / Archived). Body: `{ "status": "needs_review" }`.
async fn set_session_lifecycle_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status_str = body
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
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

/// Toggle the flagged bit on a session. Body: `{ "flagged": true }`.
async fn set_session_flag_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flagged = body.get("flagged").and_then(|v| v.as_bool()).unwrap_or(false);
    let session = state
        .desktop
        .set_session_flagged(&id, flagged)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "session": session })))
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

    let base64_data = body
        .get("base64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
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
    let mode = body
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
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

async fn compact_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state
        .desktop
        .compact_session_messages(&id)
        .await
        .map_err(into_api_error)?;
    Ok(Json(serde_json::json!({ "compacted": true, "session": session })))
}

/// Forward a permission decision (allow/deny) to an in-flight session.
///
/// Request body (all fields required):
/// ```json
/// { "requestId": "<id>", "decision": "allow" | "deny" }
/// ```
///
/// IM-05: previous version accepted `requestId: ""` and silently coerced
/// missing `decision` to "deny", which hid client bugs and made test
/// failures opaque. This handler now fails fast with 400 on missing or
/// invalid fields.
async fn forward_permission(
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
                error: format!(
                    "invalid decision: {decision} (expected: allow | deny)"
                ),
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
async fn list_wechat_accounts_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
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
    state.desktop.remove_wechat_account(&id).await.map_err(|e| {
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
    let (handle, qr_image_content, expires_at) =
        state.desktop.start_wechat_login(base_url).await.map_err(|e| {
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
    let snapshot = state.desktop.wechat_login_status(&handle).await.ok_or_else(|| {
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
    title: String,
    /// Markdown body. Written to disk verbatim under the frontmatter.
    body: String,
    /// Optional source URL. When present, recorded in the frontmatter.
    #[serde(default)]
    source_url: Option<String>,
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

    // B.2: when `source == "url"` AND the caller supplied a
    // `source_url`, upgrade the request by actually fetching the URL
    // through `wiki_ingest::url::fetch_and_body`. This replaces the
    // S1 placeholder body (a fixed `<{url}>` stub) with real content
    // pulled from the upstream server — text/html gets wrapped in a
    // code fence, text/plain and text/markdown land verbatim,
    // opaque MIMEs get a stub with byte count + content-type.
    //
    // The S1 behavior is preserved as a fallback when the caller
    // explicitly passes a non-empty `body` — this gives CLI tests
    // and integration test fixtures a way to avoid the live network
    // round-trip.
    let (effective_title, effective_body, effective_source_url) =
        if body.source == "url" && body.body.is_empty() {
            let url = body.source_url.as_deref().unwrap_or("").trim().to_string();
            if url.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "url source requires either `body` or `source_url`".to_string(),
                    }),
                ));
            }
            match wiki_ingest::url::fetch_and_body(&url).await {
                Ok(result) => {
                    let title = if body.title.trim().is_empty() {
                        result.title
                    } else {
                        body.title.clone()
                    };
                    (title, result.body, result.source_url)
                }
                Err(err) => {
                    return Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ErrorResponse {
                            error: format!("url fetch failed: {err}"),
                        }),
                    ));
                }
            }
        } else {
            // Non-url sources still require a body. The url fast-path
            // above is the ONLY case where body may legitimately be empty.
            if body.body.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "body must not be empty".to_string(),
                    }),
                ));
            }
            (body.title.clone(), body.body.clone(), body.source_url.clone())
        };

    let paths = resolve_wiki_root_for_handler()?;
    let frontmatter =
        wiki_store::RawFrontmatter::for_paste(&body.source, effective_source_url);
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
    }

    Ok(Json(raw_entry_to_json(&entry)))
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
async fn get_wiki_raw_handler(
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

async fn resolve_wiki_inbox_handler(
    Path(id): Path<u32>,
    Json(body): Json<ResolveInboxRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let updated = wiki_store::resolve_inbox_entry(&paths, id, &body.action).map_err(
        |e| match e {
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
        },
    )?;
    Ok(Json(serde_json::json!({ "entry": updated })))
}

fn raw_entry_to_json(entry: &wiki_store::RawEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "filename": entry.filename,
        "source": entry.source,
        "slug": entry.slug,
        "date": entry.date,
        "source_url": entry.source_url,
        "ingested_at": entry.ingested_at,
        "byte_size": entry.byte_size,
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

    // Step 2: build a broker adapter. When the process-global broker
    // is not installed (CLI tool path, tests) we explicitly return
    // 503 so the frontend can distinguish "LLM unavailable" from
    // "raw entry missing" and render the right CTA.
    let adapter = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "codex broker is not installed in this process".to_string(),
                }),
            )
        })?;

    // Step 3: fire the proposal.
    let proposal =
        wiki_maintainer::propose_for_raw_entry(&paths, raw_id, &adapter)
            .await
            .map_err(|e| match e {
                wiki_maintainer::MaintainerError::RawNotAvailable(msg) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: format!("raw entry not available: {msg}"),
                    }),
                ),
                wiki_maintainer::MaintainerError::Broker(msg) => {
                    // NoAccountAvailable lands here via BrokerAdapter's
                    // string flattening. Pin the 503 on anything that
                    // includes the pool-empty signal; everything else
                    // is an upstream LLM error worth a 502.
                    let is_empty_pool =
                        msg.contains("no codex account") || msg.contains("pool_size");
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

    // Step 2: flip the inbox entry to approved. Soft-fail: we log
    // and keep going even if resolve fails, because the wiki page
    // is already persisted and re-running the approve from the
    // frontend will get a 200 next time.
    let inbox_result = wiki_store::resolve_inbox_entry(&paths, id, "approve");
    let inbox_entry_json = match inbox_result {
        Ok(updated) => Some(serde_json::to_value(&updated).unwrap_or(serde_json::Value::Null)),
        Err(e) => {
            eprintln!(
                "approve-with-write: wiki page written but inbox resolve failed: {e}"
            );
            None
        }
    };

    Ok(Json(serde_json::json!({
        "written_path": written_path.display().to_string(),
        "slug": p.slug,
        "inbox_entry": inbox_entry_json,
    })))
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

fn wiki_page_proposal_to_json(p: &wiki_maintainer::WikiPageProposal) -> serde_json::Value {
    serde_json::json!({
        "slug": p.slug,
        "title": p.title,
        "summary": p.summary,
        "body": p.body,
        "source_raw_id": p.source_raw_id,
    })
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

#[derive(Debug, Deserialize)]
struct SyncCloudCodexAccountsRequest {
    accounts: Vec<desktop_core::codex_broker::CloudAccountInput>,
}

async fn sync_cloud_codex_accounts_handler(
    State(state): State<AppState>,
    Json(body): Json<SyncCloudCodexAccountsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let count = body.accounts.len();
    state.broker().sync_cloud_accounts(body.accounts).map_err(|e| {
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

async fn list_cloud_codex_accounts_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let accounts = state.broker().list_cloud_accounts();
    Json(serde_json::json!({ "accounts": accounts }))
}

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

async fn broker_status_handler(
    State(state): State<AppState>,
) -> Json<desktop_core::codex_broker::BrokerPublicStatus> {
    Json(state.broker().public_status())
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
            err["error"]
                .as_str()
                .unwrap_or("")
                .contains(".."),
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
            err["error"]
                .as_str()
                .unwrap_or("")
                .contains("requestId"),
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

        let error: serde_json::Value =
            response.json().await.expect("error payload");
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
}

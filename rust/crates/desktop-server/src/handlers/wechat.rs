use super::super::*;

// ── Phase 6C: WeChat account management HTTP handlers ──────────────
//
// Lets the frontend drive a full QR-login → monitor-spawn flow from
// the Settings UI without requiring the `desktop-server wechat-login`
// CLI. All real work happens inside DesktopState methods; these
// handlers are thin JSON wrappers.

/// `GET /api/desktop/wechat/accounts` — list persisted WeChat bots
/// with their connection status (connected / disconnected / expired).
pub(crate) async fn list_wechat_accounts_handler(
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
pub(crate) async fn delete_wechat_account_handler(
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
pub(crate) async fn start_wechat_login_handler(
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
pub(crate) async fn wechat_login_status_handler(
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
pub(crate) async fn cancel_wechat_login_handler(
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
pub(crate) async fn wechat_bridge_health_handler(
    State(state): State<AppState>,
) -> Json<BridgeHealthResponse> {
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
pub(crate) async fn wechat_bridge_config_get_handler(
) -> Json<desktop_core::wechat_ilink::WeChatIngestConfig> {
    Json(desktop_core::wechat_ilink::ingest_config::read_snapshot())
}

/// `POST /api/wechat/bridge/config` — replace the group-scope config.
/// Body must be a full [`WeChatIngestConfig`] payload (the handler
/// does not support PATCH-style merges). The new value is flushed to
/// disk before the cache swap so a reload always sees the latest.
pub(crate) async fn wechat_bridge_config_post_handler(
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

pub(crate) async fn save_kefu_config_handler(
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

pub(crate) async fn load_kefu_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.desktop.load_kefu_config().await {
        Ok(Some(config)) => {
            let summary = config.to_summary();
            Json(serde_json::to_value(&summary).unwrap_or_default())
        }
        Ok(None) => Json(serde_json::json!({ "configured": false })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub(crate) async fn create_kefu_account_handler(
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

pub(crate) async fn get_kefu_contact_url_handler(
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

pub(crate) async fn kefu_status_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.desktop.kefu_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

pub(crate) async fn start_kefu_monitor_handler(
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

pub(crate) async fn stop_kefu_monitor_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    state.desktop.stop_kefu_monitor().await;
    Json(serde_json::json!({ "ok": true }))
}

/// GET callback — kf.weixin.qq.com URL verification (echostr decrypt).
pub(crate) async fn kefu_callback_verify_handler(
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
pub(crate) async fn kefu_callback_event_handler(
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

pub(crate) async fn start_kefu_pipeline_handler(
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

pub(crate) async fn kefu_pipeline_status_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
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

pub(crate) async fn cancel_kefu_pipeline_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    state.desktop.cancel_kefu_pipeline().await;
    Json(serde_json::json!({ "ok": true }))
}

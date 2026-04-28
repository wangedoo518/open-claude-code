use super::super::*;

pub(crate) fn install(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/wechat/bridge/health",
            get(wechat_bridge_health_handler),
        )
        .route(
            "/api/wechat/bridge/config",
            get(wechat_bridge_config_get_handler).post(wechat_bridge_config_post_handler),
        )
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
        .route(
            "/api/desktop/wechat-kefu/config",
            post(save_kefu_config_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/config",
            get(load_kefu_config_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/account/create",
            post(create_kefu_account_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/contact-url",
            get(get_kefu_contact_url_handler),
        )
        .route("/api/desktop/wechat-kefu/status", get(kefu_status_handler))
        .route(
            "/api/desktop/wechat-kefu/monitor/start",
            post(start_kefu_monitor_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/monitor/stop",
            post(stop_kefu_monitor_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/callback",
            get(kefu_callback_verify_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/callback",
            post(kefu_callback_event_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/pipeline/start",
            post(start_kefu_pipeline_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/pipeline/status",
            get(kefu_pipeline_status_handler),
        )
        .route(
            "/api/desktop/wechat-kefu/pipeline/cancel",
            post(cancel_kefu_pipeline_handler),
        )
        // R1.2 reliability gate · durable outbox read endpoint. Lists
        // every WeChat reply the system tried or queued to send, so
        // the UI can surface "1 failed / 2 pending" instead of users
        // discovering missed replies anecdotally.
        .route(
            "/api/desktop/wechat/outbox",
            get(list_wechat_outbox_handler),
        )
        // R1.3 reliability gate · aggregate health snapshot. Returns
        // a single derived "connected / degraded / disconnected /
        // not_configured" verdict plus the per-channel detail the UI
        // needs to render the WeChatHealthPanel.
        .route(
            "/api/desktop/wechat/health",
            get(wechat_health_handler),
        )
}

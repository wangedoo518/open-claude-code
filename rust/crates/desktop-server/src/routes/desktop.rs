use super::super::*;

pub(crate) fn install(router: Router<AppState>) -> Router<AppState> {
    router
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
        .route(
            "/api/desktop/sessions/cleanup-empty",
            post(cleanup_empty_sessions_handler),
        )
        .route(
            "/api/desktop/sessions/{id}",
            get(get_session).delete(delete_session_handler),
        )
        .route("/api/desktop/sessions/{id}/messages", post(append_message))
        .route("/api/desktop/sessions/{id}/title", post(rename_session))
        .route("/api/desktop/sessions/{id}/cancel", post(cancel_session))
        .route("/api/desktop/sessions/{id}/resume", post(resume_session))
        .route("/api/desktop/sessions/{id}/compact", post(compact_session))
        .route("/api/desktop/sessions/{id}/fork", post(fork_session))
        .route(
            "/api/desktop/sessions/{id}/lifecycle",
            post(set_session_lifecycle_handler),
        )
        .route(
            "/api/desktop/sessions/{id}/flag",
            post(set_session_flag_handler),
        )
        .route(
            "/api/desktop/sessions/{id}/bind",
            post(bind_source_handler).delete(clear_source_binding_handler),
        )
        .route(
            "/api/desktop/attachments/process",
            post(process_attachment_handler),
        )
        .route("/api/desktop/skills", get(list_workspace_skills_handler))
        .route(
            "/api/desktop/settings/permission-mode",
            post(set_permission_mode_handler).get(get_permission_mode_handler),
        )
        .route(
            "/api/desktop/debug/mcp/probe",
            post(debug_mcp_probe_handler),
        )
        .route("/api/desktop/debug/mcp/call", post(debug_mcp_call_handler))
        .route(
            "/api/desktop/sessions/{id}/permission",
            post(forward_permission),
        )
        .route(
            "/api/desktop/sessions/{id}/events",
            get(stream_session_events),
        )
        .route("/api/ask/sessions", get(list_sessions).post(create_session))
        .route(
            "/api/ask/sessions/{id}",
            get(get_session).delete(delete_session_handler),
        )
        .route("/api/ask/sessions/{id}/messages", post(append_message))
        .route("/api/ask/sessions/{id}/cancel", post(cancel_session))
        .route("/api/ask/sessions/{id}/resume", post(resume_session))
        .route("/api/ask/sessions/{id}/events", get(stream_session_events))
        .route(
            "/api/ask/sessions/{id}/permission",
            post(forward_permission),
        )
        .route("/api/ask/sessions/{id}/title", post(rename_session))
        .route("/api/ask/sessions/{id}/compact", post(compact_session))
        .route("/api/ask/sessions/{id}/fork", post(fork_session))
        .route(
            "/api/ask/sessions/{id}/lifecycle",
            post(set_session_lifecycle_handler),
        )
        .route(
            "/api/ask/sessions/{id}/flag",
            post(set_session_flag_handler),
        )
        .route(
            "/api/ask/sessions/{id}/bind",
            post(bind_source_handler).delete(clear_source_binding_handler),
        )
        .route("/ws/wechat-inbox", get(ws_wechat_inbox_handler))
        .route(
            "/api/desktop/scheduled/{id}",
            delete(delete_scheduled_task_handler).post(update_scheduled_task),
        )
        .route(
            "/api/desktop/dispatch/items/{id}",
            delete(delete_dispatch_item_handler).post(update_dispatch_item),
        )
        .route(
            "/api/desktop/markitdown/check",
            get(markitdown_check_handler),
        )
        .route(
            "/api/desktop/markitdown/convert",
            post(markitdown_convert_handler),
        )
        .route("/api/desktop/wechat-fetch", post(wechat_fetch_handler))
        .route(
            "/api/desktop/wechat-fetch/check",
            get(wechat_fetch_check_handler),
        )
        .route("/api/desktop/url-ingest/recent", get(recent_ingest_handler))
        .route("/api/desktop/node/check", get(node_check_handler))
        .route("/api/desktop/opencli/check", get(opencli_check_handler))
        .route("/api/desktop/chromium/check", get(chromium_check_handler))
        .route(
            "/api/desktop/python-deps/install",
            post(install_python_deps_handler),
        )
        .route(
            "/api/desktop/storage/migrate",
            post(migrate_storage_handler),
        )
        .route(
            "/api/desktop/providers",
            get(list_providers_handler).post(upsert_provider_handler),
        )
        .route(
            "/api/desktop/providers/templates",
            get(list_provider_templates_handler),
        )
        .route(
            "/api/desktop/providers/{id}",
            delete(delete_provider_handler),
        )
        .route(
            "/api/desktop/providers/{id}/activate",
            post(activate_provider_handler),
        )
        .route(
            "/api/desktop/providers/{id}/test",
            post(test_provider_handler),
        )
        .route(
            "/api/desktop/providers/{id}/health-check",
            post(health_check_provider_handler),
        )
}

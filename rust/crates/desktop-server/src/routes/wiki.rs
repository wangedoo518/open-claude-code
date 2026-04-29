use super::super::*;

pub(crate) fn install(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/wiki/raw",
            get(list_wiki_raw_handler).post(ingest_wiki_raw_handler),
        )
        .route(
            "/api/wiki/raw/{id}",
            get(get_wiki_raw_handler).delete(delete_wiki_raw_handler),
        )
        .route("/api/wiki/fetch", post(preview_wiki_fetch_handler))
        .route("/api/wiki/inbox", get(list_wiki_inbox_handler))
        .route(
            "/api/wiki/inbox/{id}/resolve",
            post(resolve_wiki_inbox_handler),
        )
        .route(
            "/api/wiki/inbox/batch/resolve",
            post(batch_resolve_wiki_inbox_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/propose",
            post(propose_wiki_inbox_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/approve-with-write",
            post(approve_wiki_inbox_with_write_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/maintain",
            post(inbox_maintain_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/proposal",
            post(create_proposal_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/proposal/apply",
            post(apply_proposal_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/proposal/cancel",
            post(cancel_proposal_handler),
        )
        .route(
            "/api/wiki/proposal/combined",
            post(create_combined_proposal_handler),
        )
        .route(
            "/api/wiki/proposal/combined/apply",
            post(apply_combined_proposal_handler),
        )
        .route(
            "/api/wiki/inbox/{id}/candidates",
            get(list_inbox_candidates_handler),
        )
        .route("/api/wiki/pages", get(list_wiki_pages_handler))
        .route(
            "/api/wiki/pages/{slug}",
            get(get_wiki_page_handler).put(put_wiki_page_handler),
        )
        .route("/api/wiki/search", get(search_wiki_pages_handler))
        .route("/api/wiki/index", get(get_wiki_index_handler))
        .route("/api/wiki/log", get(get_wiki_log_handler))
        .route(
            "/api/wiki/schema",
            get(get_wiki_schema_handler).put(put_wiki_schema_handler),
        )
        .route("/api/wiki/graph", get(get_wiki_graph_handler))
        .route(
            "/api/wiki/pages/{slug}/backlinks",
            get(get_wiki_backlinks_handler),
        )
        .route("/api/wiki/pages/{slug}/graph", get(get_page_graph_handler))
        .route("/api/lineage/wiki/{slug}", get(get_wiki_lineage_handler))
        .route("/api/lineage/inbox/{id}", get(get_inbox_lineage_handler))
        .route("/api/lineage/raw/{id}", get(get_raw_lineage_handler))
        .route("/api/wiki/absorb", post(absorb_handler))
        .route("/api/wiki/absorb/events", get(stream_absorb_events_handler))
        .route("/api/wiki/query", post(query_wiki_handler))
        .route("/api/wiki/cleanup", post(cleanup_handler))
        .route("/api/wiki/breakdown", post(breakdown_handler))
        .route("/api/wiki/patrol", post(patrol_handler))
        .route("/api/wiki/absorb-log", get(get_absorb_log_handler))
        .route("/api/wiki/backlinks", get(get_backlinks_index_handler))
        .route("/api/wiki/stats", get(get_stats_handler))
        .route("/api/wiki/patrol/report", get(get_patrol_report_handler))
        .route(
            "/api/wiki/schema/templates",
            get(get_schema_templates_handler),
        )
        .route("/api/wiki/git/status", get(get_vault_git_status_handler))
        .route("/api/wiki/git/diff", get(get_vault_git_diff_handler))
        .route("/api/wiki/git/commit", post(commit_vault_git_handler))
        .route("/api/wiki/git/pull", post(pull_vault_git_handler))
        .route("/api/wiki/git/push", post(push_vault_git_handler))
        .route("/api/wiki/git/remote", post(set_vault_git_remote_handler))
        .route(
            "/api/wiki/external-ai/write-policy",
            get(get_external_ai_write_policy_handler),
        )
        .route(
            "/api/wiki/external-ai/write-policy/grants",
            post(add_external_ai_write_grant_handler),
        )
        .route(
            "/api/wiki/external-ai/write-policy/grants/{id}",
            delete(revoke_external_ai_write_grant_handler),
        )
}

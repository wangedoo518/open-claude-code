use super::super::*;

pub(crate) async fn codex_runtime(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexRuntimeResponse>> {
    let runtime = state
        .desktop()
        .codex_runtime_state()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexRuntimeResponse { runtime }))
}

pub(crate) async fn codex_auth_overview(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexAuthOverviewResponse>> {
    let overview = state
        .desktop()
        .codex_auth_overview()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexAuthOverviewResponse { overview }))
}

pub(crate) async fn import_codex_auth_profile(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexAuthOverviewResponse>> {
    let overview = state
        .desktop()
        .import_codex_auth_profile()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexAuthOverviewResponse { overview }))
}

pub(crate) async fn begin_codex_login(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopCodexLoginSessionResponse>> {
    let session = state
        .desktop()
        .begin_codex_login()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopCodexLoginSessionResponse { session }))
}

pub(crate) async fn poll_codex_login(
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

pub(crate) async fn activate_codex_auth_profile(
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

pub(crate) async fn refresh_codex_auth_profile(
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

pub(crate) async fn remove_codex_auth_profile(
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

pub(crate) async fn managed_auth_providers(
    State(state): State<AppState>,
) -> ApiResult<Json<DesktopManagedAuthProvidersResponse>> {
    let providers = state
        .desktop()
        .managed_auth_providers()
        .await
        .map_err(into_api_error)?;
    Ok(Json(DesktopManagedAuthProvidersResponse { providers }))
}

pub(crate) async fn managed_auth_accounts(
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

pub(crate) async fn import_managed_auth_accounts(
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

pub(crate) async fn begin_managed_auth_login(
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

pub(crate) async fn poll_managed_auth_login(
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

pub(crate) async fn set_managed_auth_default_account(
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

pub(crate) async fn refresh_managed_auth_account(
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

pub(crate) async fn remove_managed_auth_account(
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

fn redact_api_key_for_display(key: &str) -> String {
    let len = key.chars().count();
    if len <= 8 {
        return "***".to_string();
    }
    let prefix: String = key.chars().take(4).collect();
    let suffix: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix} ({len} chars)")
}

fn entry_to_redacted_json(
    id: &str,
    entry: &desktop_core::providers_config::ProviderEntry,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "kind": match entry.kind {
            desktop_core::providers_config::ProviderKind::Anthropic => "anthropic",
            desktop_core::providers_config::ProviderKind::OpenAiCompat => "openai_compat",
        },
        "display_name": entry.display_name,
        "base_url": entry.effective_base_url(),
        "api_key_display": redact_api_key_for_display(&entry.api_key),
        "api_key_length": entry.api_key.chars().count(),
        "model": entry.model,
        "max_tokens": entry.effective_max_tokens(),
    })
}

fn resolve_project_path_for_providers(
    provided: Option<&str>,
) -> Result<std::path::PathBuf, ApiError> {
    if let Some(raw) = provided {
        if !raw.trim().is_empty() {
            return desktop_core::validate_project_path(raw)
                .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })));
        }
    }
    std::env::current_dir().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("cannot resolve cwd: {e}"),
            }),
        )
    })
}

pub(crate) async fn list_providers_handler(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project =
        resolve_project_path_for_providers(params.get("project_path").map(String::as_str))?;
    let config = desktop_core::providers_config::load(&project).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to load providers.json: {e}"),
            }),
        )
    })?;
    let providers: Vec<serde_json::Value> = config
        .providers
        .iter()
        .map(|(id, entry)| entry_to_redacted_json(id, entry))
        .collect();
    Ok(Json(serde_json::json!({
        "version": config.version,
        "active": config.active,
        "providers": providers,
    })))
}

pub(crate) async fn upsert_provider_handler(
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "missing or empty 'id' field".to_string(),
                }),
            )
        })?;
    let project =
        resolve_project_path_for_providers(body.get("project_path").and_then(|v| v.as_str()))?;
    let entry_json = body.get("entry").cloned().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "missing 'entry' field".to_string(),
            }),
        )
    })?;
    let mut entry: desktop_core::providers_config::ProviderEntry =
        serde_json::from_value(entry_json).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid provider entry: {e}"),
                }),
            )
        })?;
    let mut config = desktop_core::providers_config::load(&project).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to load providers.json: {e}"),
            }),
        )
    })?;
    if entry.api_key.is_empty() {
        if let Some(existing) = config.providers.get(id) {
            entry.api_key = existing.api_key.clone();
        }
    }
    config.upsert(id, entry).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("upsert failed: {e}"),
            }),
        )
    })?;
    desktop_core::providers_config::save(&project, &config).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to save providers.json: {e}"),
            }),
        )
    })?;
    let saved = config.providers.get(id).expect("just inserted");
    Ok(Json(serde_json::json!({
        "ok": true,
        "id": id,
        "entry": entry_to_redacted_json(id, saved),
        "active": config.active,
    })))
}

pub(crate) async fn delete_provider_handler(
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project =
        resolve_project_path_for_providers(params.get("project_path").map(String::as_str))?;
    let mut config = desktop_core::providers_config::load(&project).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to load providers.json: {e}"),
            }),
        )
    })?;
    config.remove(&id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("remove failed: {e}"),
            }),
        )
    })?;
    desktop_core::providers_config::save(&project, &config).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to save providers.json: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id,
        "active": config.active,
    })))
}

pub(crate) async fn activate_provider_handler(
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project =
        resolve_project_path_for_providers(params.get("project_path").map(String::as_str))?;
    let mut config = desktop_core::providers_config::load(&project).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to load providers.json: {e}"),
            }),
        )
    })?;
    config.activate(&id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("activate failed: {e}"),
            }),
        )
    })?;
    desktop_core::providers_config::save(&project, &config).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to save providers.json: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "active": config.active,
    })))
}

pub(crate) async fn test_provider_handler(
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project =
        resolve_project_path_for_providers(params.get("project_path").map(String::as_str))?;
    let config = desktop_core::providers_config::load(&project).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to load providers.json: {e}"),
            }),
        )
    })?;
    let entry = config.providers.get(&id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("provider `{id}` not found"),
            }),
        )
    })?;
    let result = desktop_core::probe_provider_entry(entry).await;
    Ok(Json(serde_json::json!({
        "ok": result.ok,
        "latency_ms": result.latency_ms,
        "error": result.error,
        "model_echo": result.model_echo,
    })))
}

pub(crate) async fn list_provider_templates_handler() -> Json<serde_json::Value> {
    let templates = desktop_core::providers_config::builtin_templates();
    let items: Vec<serde_json::Value> = templates
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "display_name": t.display_name,
                "kind": match t.kind {
                    desktop_core::providers_config::ProviderKind::Anthropic => "anthropic",
                    desktop_core::providers_config::ProviderKind::OpenAiCompat => "openai_compat",
                },
                "base_url": t.base_url,
                "default_model": t.default_model,
                "max_tokens": t.max_tokens,
                "description": t.description,
                "api_key_url": t.api_key_url,
            })
        })
        .collect();
    Json(serde_json::json!({ "templates": items }))
}

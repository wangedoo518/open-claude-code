# Provider Health Check Upgrade (E22) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the Settings 「测试」 button actually predict whether chat will work. Today it tests `providers.json` directly, while the chat path runs through `resolve_runtime_credentials` (env → settings → managed-auth → providers.json). When OAuth tokens or env vars shadow the user's configured provider, the test passes but chat fails — exactly the situation that produced today's stale `****9007` error report. E22 closes the gap on three fronts at once: (1) reorder the priority chain so a UI-configured provider wins over latent OAuth tokens, (2) replace the probe with a full-stack streaming health check that runs the same resolver path chat uses, (3) surface the resolved source + TTFT + shadow warnings in the UI.

**Architecture:** New `comprehensive_probe(state, project_path, provider_id)` in `desktop-core` that invokes the production resolver path, fires a real streaming chat-completion, measures TTFT + total, classifies the source. New handler `POST /api/desktop/providers/{id}/health-check` taking `State<AppState>` (the existing `test_provider_handler` doesn't carry state and can't reach managed-auth). Frontend swaps `TestResultBadge` for a richer `HealthCheckBadge` that surfaces source + TTFT + shadow warnings inline. The legacy `test_provider_handler` stays as a `?cheap=true` fast path for the case where the user just wants to validate a key against the provider, not the full chat path.

**Tech Stack:** Rust (axum + tokio), TypeScript (React 19 + React Query 5). No new deps.

**Slicing:** 3 slices, single shippable point at end.
- **E22.1** = Priority chain fix + tests (the actual bug)
- **E22.2** = comprehensive_probe fn + handler + TS API
- **E22.3** = HealthCheckBadge UI + shadow-detection warning + ship

**Why this design (over alternatives):**
- **Promote providers.json over managed-auth, not the reverse**: managed-auth is the "I OAuth'd once, forgot about it" path; providers.json with `active` set is "I just clicked this in Settings yesterday." The most-recent-explicit-intent should win.
- **Keep env var at #1**: env is the universal override; power users rely on it for ephemeral debugging. Don't break that.
- **New endpoint, not patched test_provider_handler**: the existing handler doesn't take `State<AppState>`. Patching it would force every existing caller into an async state-handle. Net new endpoint is cleaner; we can deprecate the legacy one in E23.
- **Streaming probe**: 90% of chat traffic is streaming. Non-streaming probe missed today's bug because it didn't exercise the same code path. If a provider 401s only on `stream: true` (rare but real), the new probe catches it.

**Out of scope (defer to E23+):**
- Auto-disabling expired managed-auth tokens (this slice only WARNS)
- Per-day cost telemetry on health checks (each call is one LLM completion ≈ $0.001)
- Health check status persistence (every click is a fresh call)
- Wikipedia-style "what changed since you last tested" diff view

---

## Slice E22.1 — Priority chain fix

After this slice ships: a user with both `providers.json` (active=deepseek) AND a stale `~/.codex/auth.json` will have chat use DeepSeek. Today chat would silently use the codex JWT. This slice alone fixes the "test passes / chat 401s" bug class for the most common shadow scenario, even before the comprehensive probe lands.

### Task 1: Reorder `resolve_runtime_credentials` to prefer providers.json over managed-auth

**Files:**
- Modify: `rust/crates/desktop-core/src/lib.rs:7057-7190` (the function body — full priority chain)

**Step 1: Write failing tests**

The function reads disk files (env vars, .claude/settings.json, ~/.codex/auth.json, providers.json), so tests need a sandbox. Pattern: existing tests in `desktop-core::tests` use `tempdir` + env-var manipulation. Add new tests near where existing `resolve_runtime_credentials` tests live (search for "resolve_runtime_credentials" in test modules).

```rust
#[tokio::test]
async fn providers_json_active_wins_over_codex_oauth() {
    // Reviewer: this is the regression test for the "test passes /
    // chat 401s" trap reported in E21 follow-up debugging. A stale
    // ~/.codex/auth.json was shadowing a freshly-configured DeepSeek
    // entry in providers.json.
    let _g = ENV_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("ANTHROPIC_API_KEY");
    let project = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    std::env::set_var("CODEX_HOME", codex_home.path());

    // Seed a "looks valid" Codex auth.json (would normally win at
    // priority 3 today).
    std::fs::write(
        codex_home.path().join("auth.json"),
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"stale-jwt-12345","refresh_token":"rt","id_token":"id","account_id":"acct"},"OPENAI_API_KEY":null,"last_refresh":"2026-05-05T13:50:47Z"}"#,
    ).unwrap();
    // Seed providers.json with an active DeepSeek entry.
    std::fs::create_dir_all(project.path().join(".claw")).unwrap();
    std::fs::write(
        project.path().join(".claw").join("providers.json"),
        r#"{"version":1,"active":"deepseek","providers":{"deepseek":{"kind":"openai_compat","base_url":"https://api.deepseek.com/v1","api_key":"sk-ds-key","model":"deepseek-chat"}}}"#,
    ).unwrap();

    let state = DesktopState::new_for_testing();
    let client = resolve_runtime_credentials(&state, project.path()).await.unwrap();
    assert!(
        client.provider_id.starts_with("providers-json:"),
        "expected providers.json to win, got source={}",
        client.provider_id
    );
    assert_eq!(client.bearer_token, "sk-ds-key");

    std::env::remove_var("CODEX_HOME");
}

#[tokio::test]
async fn managed_auth_used_when_providers_json_has_no_active() {
    // Sanity: if the user hasn't configured providers.json (or
    // active is empty), fall through to managed-auth as before.
    let _g = ENV_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("ANTHROPIC_API_KEY");
    let project = tempdir().unwrap();
    // No .claw/providers.json — managed-auth should be tried.
    let state = DesktopState::new_for_testing_with_codex_token("oauth-jwt");
    let client = resolve_runtime_credentials(&state, project.path()).await.unwrap();
    assert_eq!(client.provider_id, "codex-openai");
}

#[tokio::test]
async fn env_var_still_wins_over_providers_json() {
    // Sanity: env override is universal — power users debug with it.
    let _g = ENV_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("ANTHROPIC_API_KEY", "sk-env-override");
    let project = tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".claw")).unwrap();
    std::fs::write(
        project.path().join(".claw").join("providers.json"),
        r#"{"version":1,"active":"x","providers":{"x":{"kind":"anthropic","api_key":"sk-pjs"}}}"#,
    ).unwrap();
    let state = DesktopState::new_for_testing();
    let client = resolve_runtime_credentials(&state, project.path()).await.unwrap();
    assert_eq!(client.bearer_token, "sk-env-override");
    std::env::remove_var("ANTHROPIC_API_KEY");
}
```

(Adjust `DesktopState::new_for_testing` / `ENV_TEST_GUARD` to match what already exists in the test module — if they don't exist, add minimal helpers as part of this task.)

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p desktop-core providers_json_active_wins_over_codex_oauth
```
Expected: FAIL — currently managed-auth wins at line 7085-7090 before providers.json check at 7115+.

**Step 3: Reorder the chain**

In `rust/crates/desktop-core/src/lib.rs:7057-7190`:

1. **Cut** the providers.json block (currently lines 7092-7183) out of step 5.
2. **Paste** it as a new step 3, BEFORE the managed-auth block.
3. The new chain becomes:
   - Step 1: `ANTHROPIC_API_KEY` env (unchanged)
   - Step 2: `.claude/settings.json` direct_api_key (unchanged)
   - **Step 3: `.claw/providers.json` with non-empty `active` (PROMOTED)**
   - Step 4: managed-auth codex-openai (unchanged content, was step 3)
   - Step 5: managed-auth qwen-code (unchanged content, was step 4)
4. Add a doc-comment at the top of the function explaining the rationale:

```rust
/// Resolve the credential chain that the chat path will use.
///
/// Priority order (first non-error wins):
///   1. ANTHROPIC_API_KEY env var — universal override for power
///      users debugging with a specific key.
///   2. .claude/settings.json direct_api_key — explicit project setting.
///   3. .claw/providers.json active provider — UI-configured, most
///      specific intent. PROMOTED ahead of managed-auth in E22 to
///      fix the "Settings 测试 passes but chat 401s" trap (a stale
///      OAuth token would silently shadow a freshly-saved provider).
///   4. Managed-auth codex-openai — OAuth login flow.
///   5. Managed-auth qwen-code — OAuth login flow.
///
/// Rationale for the E22 promotion: managed-auth is the "I OAuth'd
/// once, forgot about it" path; providers.json with `active` set is
/// "I just clicked this in Settings." The most-recent-explicit
/// intent should win.
```

**Step 4: Run to confirm pass**

```bash
cd rust && cargo test -p desktop-core providers_json_active_wins_over_codex_oauth managed_auth_used_when_providers_json_has_no_active env_var_still_wins_over_providers_json
```
Expected: 3 PASS.

**Step 5: Run full workspace**

```bash
cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED" | head -15
```
Expected: all green. If anything else broke, the resolver was probably load-bearing in unexpected places — fix or push back to plan.

**Step 6: Commit**

```bash
git add rust/crates/desktop-core/src/lib.rs
git commit -m "$(cat <<'EOF'
fix(desktop-core): promote providers.json over managed-auth in resolver (E22.1)

Today's debugging session uncovered a real "test passes / chat
401s" trap: when a user has a stale ~/.codex/auth.json (from a
prior OAuth login they since forgot about) AND a freshly
configured DeepSeek provider in .claw/providers.json with
active=deepseek, chat silently uses the expired Codex JWT and
401s, while the Settings 测试 button reads providers.json
directly and shows green.

Reorder resolve_runtime_credentials so providers.json (when
active is set + that entry exists) wins over managed-auth fallback.
ANTHROPIC_API_KEY env var still has top priority — power users
need that escape hatch — but the case where a user explicitly
clicks 使用中 in Settings now matches their intent.

Tests cover the regression, the fall-through (managed-auth still
used when providers.json absent), and the env override.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Slice E22.2 — `comprehensive_probe` + handler + TS API

After this slice ships: a new endpoint `POST /api/desktop/providers/{id}/health-check` runs the full chat path (same resolver, same streaming) and returns source + TTFT + total + status. Used by the upgraded UI in E22.3.

### Task 2: `comprehensive_probe` in desktop-core

**Files:**
- Modify: `rust/crates/desktop-core/src/lib.rs` — add new pub async fn near `probe_provider_entry` (~line 7247)

**Step 1: Write failing tests**

```rust
#[tokio::test]
async fn comprehensive_probe_reports_providers_json_source_when_active() {
    let _g = ENV_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("ANTHROPIC_API_KEY");
    let project = tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".claw")).unwrap();
    // Seed a fake provider pointing at a local mock server (or use
    // an existing test mock — depends on what's already wired).
    let mock = MockProvider::start_returning_streaming_ping().await;
    std::fs::write(
        project.path().join(".claw").join("providers.json"),
        format!(
            r#"{{"version":1,"active":"x","providers":{{"x":{{"kind":"openai_compat","base_url":"{}","api_key":"k","model":"m"}}}}}}"#,
            mock.url()
        ),
    ).unwrap();
    let state = DesktopState::new_for_testing();

    let result = comprehensive_probe(&state, project.path(), "x").await.unwrap();
    assert!(result.ok);
    assert_eq!(result.source.kind, "providers_json");
    assert_eq!(result.source.id, "x");
    assert!(result.ttft_ms.unwrap() > 0);
    assert!(result.total_ms >= result.ttft_ms.unwrap());
    assert!(result.error.is_none());
    assert!(!result.shadow.detected);
}

#[tokio::test]
async fn comprehensive_probe_detects_shadow_when_codex_oauth_wins() {
    // Pre-E22.1 behavior — verify that even AFTER E22.1's reorder,
    // we can still surface "shadow detected" when the SOURCE returned
    // doesn't match what the user thinks they configured. This
    // happens if the user has env var set OR has direct_api_key
    // in settings.json.
    let _g = ENV_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("ANTHROPIC_API_KEY", "sk-env-override");
    let project = tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".claw")).unwrap();
    std::fs::write(
        project.path().join(".claw").join("providers.json"),
        r#"{"version":1,"active":"deepseek","providers":{"deepseek":{"kind":"openai_compat","base_url":"https://example.com","api_key":"k","model":"m"}}}"#,
    ).unwrap();
    let state = DesktopState::new_for_testing();

    // Probe asks "test the providers.json/deepseek entry" but resolver
    // returns env-var Anthropic. shadow should be true.
    let result = comprehensive_probe(&state, project.path(), "deepseek").await.unwrap();
    assert!(result.shadow.detected, "expected shadow when env var overrides");
    assert_eq!(result.shadow.actual_source.kind, "anthropic_env");
    assert_eq!(result.shadow.requested_id, "deepseek");

    std::env::remove_var("ANTHROPIC_API_KEY");
}

#[tokio::test]
async fn comprehensive_probe_returns_error_on_provider_401() {
    let mock = MockProvider::start_returning_401("api key invalid").await;
    let project = tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".claw")).unwrap();
    std::fs::write(
        project.path().join(".claw").join("providers.json"),
        format!(
            r#"{{"version":1,"active":"x","providers":{{"x":{{"kind":"openai_compat","base_url":"{}","api_key":"bad","model":"m"}}}}}}"#,
            mock.url()
        ),
    ).unwrap();
    let state = DesktopState::new_for_testing();

    let result = comprehensive_probe(&state, project.path(), "x").await.unwrap();
    assert!(!result.ok);
    assert_eq!(result.http_status, Some(401));
    assert!(result.error.as_ref().unwrap().contains("api key invalid"));
}
```

(If `MockProvider` doesn't exist yet, add a minimal axum-based mock in `tests/common/`. Or use `wiremock` crate — check Cargo.toml first to see if already a dev-dep.)

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p desktop-core comprehensive_probe_
```
Expected: FAIL — function undefined.

**Step 3: Implement**

Add to `lib.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ProbeSource {
    /// One of: "anthropic_env", "anthropic_settings", "providers_json",
    /// "codex_oauth", "qwen_oauth". Mirrors the priority-chain branch
    /// that produced the credentials.
    pub kind: String,
    /// For "providers_json": the entry id ("deepseek", etc.).
    /// For others: a stable label like "codex-openai".
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeShadow {
    /// true when the requested provider id (from the URL) doesn't
    /// match the source the resolver actually picked.
    pub detected: bool,
    /// The id the user asked to test (from URL path).
    pub requested_id: String,
    /// What the resolver actually picked. Same shape as the top-level
    /// `source` when no shadow; populated for visibility when shadow.
    pub actual_source: ProbeSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComprehensiveProbeResult {
    pub ok: bool,
    pub source: ProbeSource,
    pub shadow: ProbeShadow,
    pub ttft_ms: Option<u64>,
    pub total_ms: u64,
    pub http_status: Option<u16>,
    pub error: Option<String>,
    pub model_echo: Option<String>,
}

/// Slice E22 — full-stack health check. Runs the SAME resolver path
/// chat uses, fires a streaming chat-completion, measures TTFT +
/// total. Used by the new health-check endpoint to predict whether
/// an actual chat will work.
///
/// `requested_provider_id`: the id from the URL path (e.g.
/// "deepseek"). Used only for shadow detection — the resolver itself
/// doesn't take a provider id; it picks based on priority chain.
pub async fn comprehensive_probe(
    state: &DesktopState,
    project_path: &Path,
    requested_provider_id: &str,
) -> Result<ComprehensiveProbeResult, DesktopStateError> {
    let started = std::time::Instant::now();

    // 1. Resolve credentials via the production path.
    let client = match resolve_runtime_credentials(state, project_path).await {
        Ok(c) => c,
        Err(e) => {
            return Ok(ComprehensiveProbeResult {
                ok: false,
                source: ProbeSource {
                    kind: "none".to_string(),
                    id: String::new(),
                },
                shadow: ProbeShadow {
                    detected: false,
                    requested_id: requested_provider_id.to_string(),
                    actual_source: ProbeSource {
                        kind: "none".to_string(),
                        id: String::new(),
                    },
                },
                ttft_ms: None,
                total_ms: started.elapsed().as_millis() as u64,
                http_status: None,
                error: Some(format!("no credentials available: {e}")),
                model_echo: None,
            });
        }
    };

    // 2. Classify source from the client's provider_id.
    let source = classify_provider_source(&client.provider_id);

    // 3. Detect shadow: requested_id from URL vs actual source id.
    // Shadow is "you asked to test deepseek but resolver returned X".
    let shadow_detected = match source.kind.as_str() {
        "providers_json" => source.id != requested_provider_id,
        _ => true, // any non-providers_json source is a shadow when the
                   // user asked to test a providers.json entry
    };
    let shadow = ProbeShadow {
        detected: shadow_detected,
        requested_id: requested_provider_id.to_string(),
        actual_source: source.clone(),
    };

    // 4. Fire a streaming chat-completion. Measure TTFT + total.
    let request = MessageRequest {
        model: client.default_model.clone().unwrap_or_default(),
        max_tokens: 8,
        messages: vec![InputMessage::user_text("ping")],
        system: None,
        tools: None,
        tool_choice: None,
        stream: true,
    };

    let stream_started = std::time::Instant::now();
    let mut ttft_ms: Option<u64> = None;
    let mut http_status: Option<u16> = None;
    let mut error: Option<String> = None;
    let mut model_echo: Option<String> = None;

    match send_streaming_with_classification(&client, &request).await {
        Ok(mut stream) => {
            while let Some(event) = stream.next().await {
                match event {
                    Ok(chunk) => {
                        if ttft_ms.is_none() {
                            ttft_ms = Some(stream_started.elapsed().as_millis() as u64);
                        }
                        if model_echo.is_none() {
                            model_echo = chunk.model.clone();
                        }
                    }
                    Err(e) => {
                        // Capture HTTP status from the error if available.
                        if let Some(status) = e.http_status() {
                            http_status = Some(status);
                        }
                        error = Some(e.to_string());
                        break;
                    }
                }
            }
        }
        Err(e) => {
            if let Some(status) = e.http_status() {
                http_status = Some(status);
            }
            error = Some(e.to_string());
        }
    }

    let total_ms = started.elapsed().as_millis() as u64;
    let ok = error.is_none() && http_status.unwrap_or(200) < 400;

    Ok(ComprehensiveProbeResult {
        ok,
        source,
        shadow,
        ttft_ms,
        total_ms,
        http_status,
        error,
        model_echo,
    })
}

fn classify_provider_source(provider_id: &str) -> ProbeSource {
    if let Some(rest) = provider_id.strip_prefix("providers-json:") {
        ProbeSource {
            kind: "providers_json".to_string(),
            id: rest.to_string(),
        }
    } else if provider_id == "codex-openai" {
        ProbeSource {
            kind: "codex_oauth".to_string(),
            id: provider_id.to_string(),
        }
    } else if provider_id == "qwen-code" {
        ProbeSource {
            kind: "qwen_oauth".to_string(),
            id: provider_id.to_string(),
        }
    } else if provider_id == "direct-anthropic" {
        // Could be from env or settings.json — distinguish by checking
        // which path was taken. Cleanest: pass a hint from
        // resolve_runtime_credentials. For v1 this lumps both as
        // "anthropic_direct" — refine if user asks.
        ProbeSource {
            kind: "anthropic_direct".to_string(),
            id: provider_id.to_string(),
        }
    } else {
        ProbeSource {
            kind: "unknown".to_string(),
            id: provider_id.to_string(),
        }
    }
}
```

**Note on `send_streaming_with_classification`:** if no existing helper handles streaming + http-status extraction in one call, look at how `wiki_maintainer` calls `broker.chat_completion` (non-streaming) and what `OpenAiCompatClient` exposes. May need to add a thin wrapper. Check `rust/crates/desktop-core/src/lib.rs:7207` (`build_provider_client_from_entry`) and follow `ProviderClient::OpenAi(client)` for the streaming method.

**Step 4: Run to confirm pass**

```bash
cd rust && cargo test -p desktop-core comprehensive_probe_
```
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add rust/crates/desktop-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(desktop-core): comprehensive_probe — full-stack provider health check (E22.2)

Runs the same resolve_runtime_credentials + streaming chat path
that production chat uses, measures TTFT + total, classifies the
returned credentials by source. Returns shadow detection: when
the resolver picked a different source than the URL-requested
provider id, the UI can warn the user "you tested deepseek but
chat will use codex_oauth — disable the OAuth login or your
DeepSeek config will never run."

Three tests cover happy path, shadow detection (via env var
override), and provider 401 surfaced with the http_status field.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Handler `POST /api/desktop/providers/{id}/health-check` + route + TS API

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/provider_runtime.rs` (new handler near `test_provider_handler` at line 455)
- Modify: `rust/crates/desktop-server/src/routes/desktop.rs` (or wherever provider routes live — find by grepping `test_provider_handler`)
- Modify: `rust/crates/desktop-server/src/lib.rs` (export new handler)
- Modify: `apps/desktop-shell/src/api/desktop/settings.ts` (TS helper near `testProvider` at line 512)

**Step 1: Write failing tests**

```rust
// In a draft_tests-style submodule near test_provider_handler tests:

#[tokio::test]
async fn health_check_handler_returns_404_when_provider_id_unknown_and_no_resolver_match() {
    // If providers.json doesn't have the requested id AND nothing else
    // resolves, return 404 — there's literally nothing to test.
    let state = AppState::new_for_testing();
    let err = health_check_provider_handler(
        State(state),
        Path("does-not-exist".to_string()),
        Query(Default::default()),
    )
    .await
    .expect_err("missing id with no resolver fallback");
    assert_eq!(err.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn health_check_handler_returns_200_with_shadow_when_resolver_picks_other_source() {
    // Set up providers.json with id "deepseek" pointing at a mock
    // server, AND env ANTHROPIC_API_KEY set. The handler should run,
    // resolver picks env (priority 1), result.ok = true (anthropic
    // mock returned 200), result.shadow.detected = true.
    // ... [similar shape to comprehensive_probe shadow test]
}
```

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p desktop-server health_check_
```
Expected: FAIL — handler undefined.

**Step 3: Implement handler**

In `rust/crates/desktop-server/src/handlers/provider_runtime.rs`, near line 455:

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct HealthCheckQuery {
    #[serde(default)]
    project_path: Option<String>,
}

pub(crate) async fn health_check_provider_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HealthCheckQuery>,
) -> Result<Json<ComprehensiveProbeResult>, ApiError> {
    let project_path = params
        .project_path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let result = desktop_core::comprehensive_probe(
        state.desktop_state(),
        &project_path,
        &id,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("health check failed: {e}"),
            }),
        )
    })?;

    Ok(Json(result))
}
```

**Step 4: Register route + export**

In `rust/crates/desktop-server/src/routes/desktop.rs` (search for `test_provider_handler` route registration):

```rust
.route(
    "/api/desktop/providers/{id}/health-check",
    post(health_check_provider_handler),
)
```

In `lib.rs`, extend the `pub(crate) use handlers::provider_runtime::{...}` block:

```rust
health_check_provider_handler,
```

**Step 5: TS helper**

In `apps/desktop-shell/src/api/desktop/settings.ts`, append near `testProvider` (line 512):

```typescript
export interface ProbeSource {
  kind:
    | "providers_json"
    | "codex_oauth"
    | "qwen_oauth"
    | "anthropic_direct"
    | "unknown"
    | "none";
  id: string;
}

export interface ProbeShadow {
  detected: boolean;
  requested_id: string;
  actual_source: ProbeSource;
}

export interface HealthCheckResult {
  ok: boolean;
  source: ProbeSource;
  shadow: ProbeShadow;
  ttft_ms: number | null;
  total_ms: number;
  http_status: number | null;
  error: string | null;
  model_echo: string | null;
}

/**
 * Slice E22 — comprehensive provider health check. Runs the same
 * resolver + streaming chat path that production chat uses; surfaces
 * source + TTFT + shadow detection. Replaces the lighter
 * `testProvider` for the Settings UI; the cheap probe stays
 * available for future "quick check" UX.
 */
export async function healthCheckProvider(
  id: string,
  projectPath?: string,
): Promise<HealthCheckResult> {
  const qs = projectPath
    ? `?project_path=${encodeURIComponent(projectPath)}`
    : "";
  return fetchJson<HealthCheckResult>(
    `/api/desktop/providers/${encodeURIComponent(id)}/health-check${qs}`,
    { method: "POST" },
  );
}
```

**Step 6: Type-check + build**

```bash
cd apps/desktop-shell && npx tsc --noEmit
```
Expected: clean.

**Step 7: Commit**

```bash
git add rust/crates/desktop-server/src/handlers/provider_runtime.rs \
        rust/crates/desktop-server/src/routes/desktop.rs \
        rust/crates/desktop-server/src/lib.rs \
        apps/desktop-shell/src/api/desktop/settings.ts
git commit -m "$(cat <<'EOF'
feat(api): POST /api/desktop/providers/{id}/health-check + TS helper (E22.2)

Wraps comprehensive_probe behind a stateful axum handler so it can
reach managed_auth_runtime_client. Returns the full ProbeResult
shape — frontend uses it for source + TTFT + shadow surfacing.

Legacy test_provider_handler stays for the cheap-probe path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Slice E22.3 — UI: HealthCheckBadge + shadow warning + ship

After this slice ships: the Settings provider row's badge shows source + TTFT + total + ⚠️ shadow warning (with actionable hint). v0.1.14 tagged + pushed.

### Task 4: HealthCheckBadge component (replaces TestResultBadge)

**Files:**
- Create: `apps/desktop-shell/src/features/settings/sections/HealthCheckBadge.tsx`
- Modify: `apps/desktop-shell/src/features/settings/sections/MultiProviderSettings.tsx` — swap `TestResultBadge` import + wire the new mutation

**Step 1: Create the badge component**

```tsx
import {
  CheckCircle2,
  XCircle,
  AlertTriangle,
  Loader2,
  Activity,
} from "lucide-react";
import type { HealthCheckResult } from "@/api/desktop/settings";

/**
 * Slice E22.3 — multi-line badge for the new comprehensive health
 * check. Surfaces three things the old TestResultBadge didn't:
 *   - Which credential source the resolver actually picked (not
 *     just "the providers.json entry you tested").
 *   - First-token latency (TTFT), which is what users actually
 *     experience as "responsiveness" — total latency is misleading.
 *   - Shadow detection: a one-click pointer at the misconfiguration
 *     when the resolved source ≠ the entry being tested.
 */
export function HealthCheckBadge({
  result,
  testing,
}: {
  result: HealthCheckResult | null;
  testing: boolean;
}) {
  if (testing) {
    return (
      <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        测试中…
      </div>
    );
  }
  if (!result) return null;

  const ok = result.ok;
  const Icon = ok ? CheckCircle2 : XCircle;
  const tone = ok ? "text-emerald-600" : "text-destructive";

  return (
    <div className="flex flex-col gap-1 text-[12px]">
      <div className={`flex items-center gap-1.5 ${tone}`}>
        <Icon className="size-3.5" />
        {ok ? "通" : "不通"}
        {result.ttft_ms !== null && (
          <span className="text-muted-foreground">
            · 首字 {result.ttft_ms}ms
          </span>
        )}
        <span className="text-muted-foreground">· 总 {result.total_ms}ms</span>
      </div>

      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <Activity className="size-3" />
        来源: {sourceLabel(result.source)}
      </div>

      {result.shadow.detected && (
        <div className="flex items-start gap-1.5 rounded-md border border-amber-500/40 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-700 dark:text-amber-400">
          <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
          <div>
            <div className="font-medium">配了但没在用</div>
            <div className="mt-0.5">
              你测的是 <code>{result.shadow.requested_id}</code>，但 chat
              实际会走 <code>{sourceLabel(result.shadow.actual_source)}</code>。
              {shadowFixHint(result.shadow.actual_source)}
            </div>
          </div>
        </div>
      )}

      {result.error && (
        <div className="text-[11px] text-destructive">
          错误: {result.error}
          {result.http_status !== null && (
            <span className="ml-1 text-muted-foreground">
              (HTTP {result.http_status})
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function sourceLabel(s: { kind: string; id: string }): string {
  switch (s.kind) {
    case "providers_json":
      return `providers.json:${s.id}`;
    case "codex_oauth":
      return "Codex OAuth (~/.codex/auth.json)";
    case "qwen_oauth":
      return "Qwen OAuth";
    case "anthropic_direct":
      return "Anthropic key (env or settings.json)";
    case "none":
      return "(none)";
    default:
      return s.id;
  }
}

function shadowFixHint(actual: { kind: string }): string {
  switch (actual.kind) {
    case "codex_oauth":
      return "去 ~/.codex/auth.json 删掉过期的 OAuth token，或在 Settings 里登出 OpenAI 账号。";
    case "qwen_oauth":
      return "在 Settings 里登出 Qwen Code 账号。";
    case "anthropic_direct":
      return "你设置了 ANTHROPIC_API_KEY 环境变量或 .claude/settings.json 里配了 direct_api_key — 取消其中之一。";
    default:
      return "检查你的环境变量和 ~/.claude/settings.json。";
  }
}
```

**Step 2: Wire into MultiProviderSettings**

In `apps/desktop-shell/src/features/settings/sections/MultiProviderSettings.tsx`:

1. Add import:
   ```tsx
   import { healthCheckProvider } from "@/api/desktop/settings";
   import type { HealthCheckResult } from "@/api/desktop/settings";
   import { HealthCheckBadge } from "./HealthCheckBadge";
   ```

2. Replace the `testProvider` mutation with the new one:
   ```tsx
   const healthCheckMutation = useMutation({
     mutationFn: (id: string) => healthCheckProvider(id, projectPath ?? undefined),
   });
   ```

3. In `ProviderCard` (line 317-432), swap `<TestResultBadge .../>` for:
   ```tsx
   <HealthCheckBadge
     result={healthCheckMutation.data ?? null}
     testing={healthCheckMutation.isPending}
   />
   ```
   Pass result keyed per-provider (the existing pattern probably keeps test results per id).

4. Change the "测试" button label to "健康检查" (more accurate now):
   ```tsx
   onClick={() => healthCheckMutation.mutate(provider.id)}
   ```

**Step 3: Type-check + build**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: clean.

**Step 4: Manual smoke**

In `npm run tauri:dev`:
1. Settings → 账户与模型 → DeepSeek → 健康检查 → wait ~5-30s
2. Verify badge shows: ✓ 通 · 首字 200-500ms · 总 800-2000ms · 来源 providers.json:deepseek
3. Set `set ANTHROPIC_API_KEY=sk-junk` in the buddy launch env, restart, click 健康检查 again
4. Verify ⚠️ shadow warning appears with the actionable hint about env var

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/settings/sections/HealthCheckBadge.tsx \
        apps/desktop-shell/src/features/settings/sections/MultiProviderSettings.tsx
git commit -m "$(cat <<'EOF'
feat(settings): HealthCheckBadge with source + TTFT + shadow warning (E22.3)

Replaces the single-line "测试通过 758ms" badge with a 3-row
display: status + first-token + total latency, the resolved
credential source, and an inline ⚠️ amber warning when the
resolver picked a different source than the entry the user
tested. Each shadow case carries an actionable hint pointing
the user at the file/env to fix.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Bump v0.1.14 + plan link + push

**Files:**
- Modify: `apps/desktop-shell/src-tauri/tauri.conf.json` (version)
- Modify: `apps/desktop-shell/src-tauri/Cargo.toml` (version)
- Modify: `docs/desktop-shell/plans/README.md` (link this plan)

**Step 1: Final verification**

```bash
cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED" | head -15
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: all green / clean.

**Step 2: Bump version**

```bash
sed -i 's/"version": "0.1.13"/"version": "0.1.14"/' apps/desktop-shell/src-tauri/tauri.conf.json
sed -i 's/^version = "0.1.13"$/version = "0.1.14"/' apps/desktop-shell/src-tauri/Cargo.toml
cd apps/desktop-shell/src-tauri && cargo check 2>&1 | tail -2
```

**Step 3: Update plan index**

Add to `docs/desktop-shell/plans/README.md`:

```markdown
- [Provider Health Check Implementation Plan](./2026-05-11-provider-health-check-plan.md)
```

**Step 4: Commit + tag + push**

```bash
git add docs/desktop-shell/plans/README.md docs/desktop-shell/plans/2026-05-11-provider-health-check-plan.md \
  apps/desktop-shell/src-tauri/tauri.conf.json apps/desktop-shell/src-tauri/Cargo.toml \
  apps/desktop-shell/src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
chore: bump desktop-shell to v0.1.14 + Provider Health Check plan

E22 ships the fix for the "Settings 测试 passes but chat 401s"
trap that bit a user during E21 follow-up. Three pieces:

1. resolve_runtime_credentials reorder: providers.json with
   active set wins over managed-auth fallback, so a stale
   ~/.codex/auth.json no longer silently shadows a freshly
   configured DeepSeek entry.
2. comprehensive_probe + new health-check endpoint: runs the
   same resolver + streaming chat path production uses, so the
   button result actually predicts whether chat will work.
3. HealthCheckBadge: surfaces source + TTFT + total + shadow
   warning with actionable per-source fix hints.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git tag -a v0.1.14 -m "v0.1.14: Provider Health Check (E22)"
git push origin main
git push origin v0.1.14
```

---

## Done criteria

- A user with `~/.codex/auth.json` AND active providers.json/deepseek can chat with DeepSeek (priority chain fixed).
- A user clicking 健康检查 sees the resolved source, not just "providers.json said yes".
- A user with shadow misconfiguration (env var set, OAuth token lingering, etc.) sees ⚠️ inline + per-source fix hint.
- TTFT is shown alongside total latency.
- Workspace cargo tests green; tsc clean; vite build clean.

## Risks called out

1. **Breaking change for managed-auth users**: a user who DELIBERATELY relies on Codex OAuth + has an old providers.json sitting around will suddenly start using providers.json. Mitigation: the providers.json check requires `active` to be non-empty AND the entry to exist — if both conditions fail, fall through to managed-auth as before. So the "delete providers.json to use OAuth" path still works.
2. **Cost per health check**: each call is one streaming chat-completion (max 8 tokens out). At Claude Sonnet pricing ≈ $0.001 / call. Acceptable. Don't auto-fire on Settings page load — only on button click.
3. **TTFT measurement quirks**: some providers buffer the first chunk for 1-2s before streaming. TTFT reported will include that buffer. Document in the badge tooltip ("首字延迟 = 从请求发出到收到第一个非空 chunk").
4. **Streaming-only providers**: if a provider doesn't support `stream: true`, the probe will fail. Acceptable — that's the same failure chat would hit.

## Out of scope (defer to E23+)

- Auto-disable expired managed-auth tokens after N consecutive 401s.
- Per-day budget cap on health checks (one-off cost is too low to justify).
- Health check status persistence across UI sessions.
- "What changed since last test" diff view.
- Cleanup tooling for `~/.codex/auth.json` orphans.

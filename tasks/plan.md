# Audit Fix Plan (v4) — Full Code Review Sprint

## Status
- **Created**: 2026-04-08
- **Baseline commit**: `a3a8091` (after S-01/S-02/S-03 fixes)
- **Source**: 全量代码审查 (full-code-review) — 18 findings, 3 parallel expert agents + human verification
- **Author**: Claude (planning skill)

## Findings summary

| Severity | Count | IDs |
|---|---|---|
| Critical | 4 | CR-01 CR-02 CR-03 CR-04 |
| Important | 5 | IM-01 IM-02 IM-03 IM-04 IM-05 |
| Suggestion | 9 | SG-01 SG-02 SG-03 SG-04 SG-05 SG-06 SG-07 SG-08 SG-09 |

Downgraded (agent over-reported, verified non-issues):
- S-05 debug endpoints → localhost-only binding mitigates → SG-01
- I1 EventSource listeners → GC collects with source.close() → non-issue
- I2 Escape-to-deny → intentional fail-closed → non-issue

---

## Dependency graph

```
                  T0.1  T0.2           ← shared foundations
                   │     │
          ┌────────┼─────┼──────┐
          ▼        ▼     ▼      ▼
          T1.4    T1.3  T1.1   T1.2   ← Phase 1 Critical (parallel after T0)
          │        │     │      │
          └────────┴──┬──┴──────┘
                     ▼
                Checkpoint 1 (E2E attachment flow)
                     ▼
          ┌──────────┼──────────┐
          ▼          ▼          ▼
         T2.1       T2.2       T2.3    ← Phase 2 Important validation (parallel)
          └──────────┬──────────┘
                     ▼
                Checkpoint 2 (HTTP smoke)
                     ▼
          ┌──────────┼──────────┐
          ▼          ▼          ▼
         T3.1       T3.2                ← Phase 3 Important reliability (parallel)
          └──────────┬──────────┘
                     ▼
                Checkpoint 3 (cargo test + strict)
                     ▼
          T4.1 … T4.9                   ← Phase 4 Suggestions (batched)
                     ▼
                Final Checkpoint (全量回归)
                     ▼
                 commit + push
```

---

## Vertical slicing principle

Each task is a **complete vertical slice** — code change + test + verification in one step. No horizontal layers. Each task can be reverted independently without breaking unrelated tasks.

---

## Phase 0 — Shared foundations

### T0.1 — Axum `DefaultBodyLimit` layer
**Problem**: Router has no body-size limit; malicious payloads can OOM.
**Change**:
- `rust/crates/desktop-server/src/lib.rs:160-164`
- Add `.layer(DefaultBodyLimit::max(15 * 1024 * 1024))` before `.with_state(state)`.
**Test**:
- `cargo test -p desktop-server` — existing pass
- `curl` a >15MB POST → expect 413
**Acceptance**:
- cargo check zero error
- Oversize payload rejected with 413 not 500

### T0.2 — `lib/security.ts` with `sanitizeFilename`
**Problem**: No centralized filename sanitizer; RTL/zero-width chars render raw.
**Change**:
- Create `apps/desktop-shell/src/lib/security.ts`
- Export `sanitizeFilename(name: string): string` stripping `\u200E \u200F \u200B \u200C \u200D \u202A-\u202E`
- Export `isDisplaySafe(name: string): boolean`
**Test**:
- Create `apps/desktop-shell/src/lib/security.test.ts`
- Cases: normal, RTL override, zero-width, mixed, empty, CJK
**Acceptance**:
- vitest pass (or tsc compile at minimum if no vitest configured)
- Function referenceable from T1.3

---

## Phase 1 — Critical fixes (4 tasks)

### T1.1 — CR-01 Send button accepts attachment-only
**File**: `apps/desktop-shell/src/features/session-workbench/InputBar.tsx:675`
**Change**:
```diff
- disabled={!value.trim()}
+ disabled={!value.trim() && attachments.length === 0}
```
**Also** update the className condition on L669 to match.
**Test**: Manual — upload file without text, button enables; click sends.
**Acceptance**:
- tsc pass
- Manual verification via Chrome devtools preview (or `file:` UI walkthrough if backend stub)
- Button visually enabled when attachment present but text empty

### T1.2 — CR-02 File upload validation + error feedback
**File**: `apps/desktop-shell/src/features/session-workbench/InputBar.tsx:55-192`
**Change**:
1. Extract `validateFile(file: File): string | null` helper.
2. MIME whitelist:
   ```ts
   const ALLOWED_MIME = new Set([
     "text/plain", "text/markdown", "text/csv", "application/json",
     "image/png", "image/jpeg", "image/gif", "image/webp",
     "application/pdf",
   ]);
   ```
3. Allow files with empty `file.type` but whose extension is in a safe list (.md, .txt, .log, .rs, .ts, .tsx, .js, .py, .go, .java, .rb, .sh).
4. Size check: 10MB raw size, reject > limit with clear error.
5. In `handleFiles`, call `validateFile(file)` first; append error to an array; show last error via `setUploadError` and total rejected count.
**Test**: Add `InputBar.test.tsx` (or manual if no test runner): drop .exe → rejected; drop 15MB .txt → rejected with message.
**Acceptance**:
- tsc pass
- Rejected files never reach `uploadAttachment`
- User sees explicit error for each rejection

### T1.3 — CR-03 Filename rendering sanitization
**File**: `apps/desktop-shell/src/features/session-workbench/InputBar.tsx:479`
**Depends on**: T0.2
**Change**:
```tsx
import { sanitizeFilename } from "@/lib/security";
// ...
<span
  className="max-w-[180px] truncate font-medium"
  dir="ltr"
  title={sanitizeFilename(att.filename)}
>
  {sanitizeFilename(att.filename)}
</span>
```
**Test**: Manual with filename `evil\u202Etxt.exe` → displays `eviltxt.exe` with LTR, no visual deception.
**Acceptance**:
- tsc pass
- RTL override visually neutralized

### T1.4 — CR-04 Attachment handler strict size limit
**File**: `rust/crates/desktop-server/src/lib.rs:947-1002`
**Depends on**: T0.1
**Change**:
1. Before base64 decode, check `base64_data.len() < 14 * 1024 * 1024` (~10MB decoded).
2. After decode, re-check `bytes.len() < 10 * 1024 * 1024`.
3. Return `413 PAYLOAD_TOO_LARGE` with ErrorResponse.
**Test**: `cargo test -p desktop-server` — add new test `attachments_rejects_oversized_payload`.
**Acceptance**:
- cargo test passes
- >10MB payload returns 413 JSON error

### Checkpoint 1
- `cargo test --workspace` — 82+ tests, 0 failures
- `npx tsc --noEmit` in apps/desktop-shell — 0 errors
- Backend smoke: POST `/attachments/process` with small legit file → 200
- Backend smoke: POST with 20MB payload → 413
- Frontend manual: Send button + drop+send flow + RTL filename

---

## Phase 2 — Important validation (3 tasks)

### T2.1 — IM-01 `create_session` validates project_path
**File**: `rust/crates/desktop-server/src/lib.rs:595-604`
**Change**:
```rust
async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateDesktopSessionRequest>,
) -> ApiResult<(StatusCode, Json<CreateDesktopSessionResponse>)> {
    if let Some(ref path) = payload.project_path {
        if !path.is_empty() {
            desktop_core::validate_project_path(path).map_err(|e| {
                (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e }))
            })?;
        }
    }
    // ... existing body, wrap result in Ok(...)
}
```
Note: return type changes to `ApiResult<...>`; update route registration if needed.
**Test**: `cargo test -p desktop-server` — add `create_session_rejects_traversal`.
**Acceptance**:
- cargo test pass
- POST with `project_path: "../../../etc"` → 400

### T2.2 — IM-02 `create_scheduled_task` validates project_path
**File**: `rust/crates/desktop-server/src/lib.rs:606-619`
**Change**: Same pattern as T2.1.
**Test**: `create_scheduled_task_rejects_traversal`.
**Acceptance**: Same as T2.1.

### T2.3 — IM-05 `forward_permission` fail-fast validation
**File**: `rust/crates/desktop-server/src/lib.rs:1157-1170`
**Change**:
```rust
let request_id = body.get("requestId").and_then(|v| v.as_str())
    .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(ErrorResponse {
        error: "missing requestId".to_string(),
    })))?;
let decision = body.get("decision").and_then(|v| v.as_str())
    .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(ErrorResponse {
        error: "missing decision".to_string(),
    })))?;
// Also validate decision is one of "allow" | "deny"
if !matches!(decision, "allow" | "deny") {
    return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse {
        error: format!("invalid decision: {} (expected: allow | deny)", decision),
    })));
}
```
**Test**: `cargo test -p desktop-server` — add `forward_permission_rejects_missing_fields`.
**Acceptance**:
- cargo test pass
- Missing field → 400 JSON error

### Checkpoint 2
- `cargo test --workspace` — still green
- `curl` smoke: create_session with traversal → 400
- `curl` smoke: forward_permission with empty body → 400

---

## Phase 3 — Important reliability (2 tasks)

### T3.1 — IM-03 `u32::try_from` no-panic path
**File**: `rust/crates/desktop-core/src/lib.rs:4308`
**Change**: Replace `.expect(...)` with `.map_err(|_| AgenticError::Internal("response block index overflow".to_string()))?`. The function `response_to_events` must return `Result<Vec<AssistantEvent>, AgenticError>`. Cascade update all call sites.
**Alternative (simpler)**: keep return type infallible; cap blocks at `u32::MAX` via `.take(u32::MAX as usize)` + log warning.
**Decision**: Use the simpler alternative (cap + log) to avoid a ripple-effect refactor. Panic on > 4B content blocks is theoretically only reachable via malicious/broken API and capping is safe.
**Test**: Add unit test `response_with_many_blocks_does_not_panic`.
**Acceptance**:
- cargo test pass
- No `expect()` at the panic site

### T3.2 — IM-04 Tool execution timeout
**File**: `rust/crates/desktop-core/src/agentic_loop.rs:556-560`
**Change**:
```rust
let result = match tokio::time::timeout(
    Duration::from_secs(120),
    tokio::task::spawn_blocking(move || {
        execute_tool_in_workspace(&tool_cwd, &name, &input_value)
    })
).await {
    Ok(Ok(result)) => result,
    Ok(Err(e)) => Err(format!("tool task panicked: {e}")),
    Err(_) => Err("tool execution timeout (120s)".to_string()),
};
```
**Test**: Add a built-in test tool that sleeps 200s; verify timeout triggers.
**Acceptance**:
- cargo test pass
- Long-running tool returns timeout error, does not block process

### Checkpoint 3
- `cargo test --workspace` — still green
- `cargo clippy` (non-strict) — no new warnings in changed files
- Stress: invoke `execute_turn()` 50× in a loop — no leaks

---

## Phase 4 — Suggestions (batched)

Batched into 2-3 commits to keep diffs focused.

### T4.1 — SG-01 Debug endpoints cfg-gated
**Change**: Wrap `/api/desktop/debug/mcp/{probe,call}` route registrations in `#[cfg(debug_assertions)]` block, with runtime fallback via `std::env::var("OCL_ENABLE_DEBUG").is_ok()` for release diagnostics.

### T4.2 — SG-02 Permission mode turn-caching docs
**Change**: Add a rustdoc comment at `lib.rs:2918` explaining that permission_mode is captured per-turn and a mid-turn UI change won't affect the in-flight turn. Reference audit-lessons L-06.

### T4.3 — SG-03 `strip_yaml_frontmatter` boundary safety
**Change**: `system_prompt.rs:359` — replace manual slice with `rest.get(end_idx + 5..).unwrap_or(rest)` which is safe against non-boundary indices.

### T4.4 — SG-04 CLI `--json` redaction
**Change**: `desktop-cli/src/main.rs` — add `redact_sensitive_fields(value: &mut Value)` that recursively walks the JSON and replaces values of keys in `{token, password, secret, api_key, apiKey, access_token, refresh_token}` with `"***"`. Call it before JSON pretty-print.
**Test**: New unit test `redact_nested_sensitive_fields`.

### T4.5 — SG-05 Drop-handler folder rejection
**Change**: `InputBar.tsx:212-221` — filter out entries where `file.webkitRelativePath && file.webkitRelativePath !== file.name` and surface "Folders are not supported" via `setUploadError`.

### T4.6 — SG-06 Sidebar context menu listener stability
**Change**: `SessionWorkbenchSidebar.tsx:89-99` — change dep array to `[]`, use the ref check inline and call `setContextMenu(null)` through a ref to latest state to avoid stale closures. Use `setTimeout(0)` to delay listener registration past the opening click.

### T4.7 — SG-07 CORS policy comment
**Change**: `desktop-server/lib.rs:161-164` — add doc comment explaining the liberal CORS is intentional for the local-only Tauri scenario and must not be relaxed further.

### T4.8 — SG-08 Attachment chip stable key
**Change**: Add a `id: string` field to `ProcessedAttachment` generated with `crypto.randomUUID()` at upload time. Use `att.id` as React key.

### T4.9 — SG-09 Handler naming consistency
**Change**: Rename all handlers that lack the `_handler` suffix to add it. This is mechanical. Touches ~15 route registrations and function defs.

### Final Checkpoint
- `cargo test --workspace` — all green (expect 95+ tests now)
- `cargo check --workspace --all-targets` — 0 errors
- `npx tsc --noEmit` in desktop-shell — 0 errors
- Backend smoke: full E2E flow of sessions + permissions + attachments
- CLI smoke: all 12 documented ocl commands
- Git diff review: no accidental scope creep
- Single squashed-commit push OR staged commits per phase (user preference)

---

## Out of scope

- L-series (audit-lessons.md) — all already fixed in earlier rounds
- S-01/S-02/S-03 — already fixed in `ff37438` / `a3a8091`
- Tauri shell manual click-through — requires user + screen
- Real-LLM E2E (needs credentials user declined to share)
- Full accessibility audit — separate sprint
- Performance profiling — separate sprint

---

## Success criteria (exit bar)

1. **0 Critical** remaining
2. **0 Important** remaining
3. **≤3 Suggestion** remaining (the ones deferred here)
4. Test count ≥ 90 passing (current 82 + ~8 new)
5. Zero new compile warnings with `cargo build` (not strict clippy)
6. Zero TS errors
7. All CLI commands still work end-to-end
8. Commit history: clean, one commit per phase or one squashed

---

## Estimated effort

| Phase | Tasks | Est. | Parallelizable |
|---|---|---|---|
| Phase 0 | 2 | 20 min | yes |
| Phase 1 | 4 | 70 min | yes (after T0) |
| Phase 2 | 3 | 40 min | yes |
| Phase 3 | 2 | 50 min | yes |
| Phase 4 | 9 | 90 min | mostly yes |
| Checkpoints × 4 | — | 40 min | no |
| **Total** | **20 tasks** | **~5 hours** | |

---

## Risk log

| Risk | Mitigation |
|---|---|
| T2.1 return type change cascades | Start with `ApiResult` only; existing tests will fail fast |
| T3.1 "cap + log" path hides real bugs | Add `tracing::warn!` so ops sees it |
| T3.2 timeout breaks legitimate long tools | 120s default, make env-overridable via `OCL_TOOL_TIMEOUT_SECS` |
| T4.9 rename breaks git blame | Do this as the last commit so history is clean |
| RTL sanitizer accidentally strips legitimate chars | Unit tests with CJK + Arabic (non-RTL-override) + Hebrew |

# Audit Fix Checklist (v4)

**Status**: Awaiting execution approval
**Baseline**: `a3a8091`
**Source**: 全量代码审查 — 18 findings (4 Critical / 5 Important / 9 Suggestion)
**Estimated**: ~5 hours total
**Plan doc**: `tasks/plan.md`

---

## Phase 0 — Shared foundations (parallel)

- [ ] **T0.1** Add `DefaultBodyLimit::max(15MB)` layer to Axum router
  - File: `rust/crates/desktop-server/src/lib.rs:160-164`
  - Verify: >15MB POST returns 413
- [ ] **T0.2** Create `apps/desktop-shell/src/lib/security.ts` with `sanitizeFilename()`
  - New file + unit test
  - Covers RTL override, zero-width chars, CJK safe

---

## Phase 1 — Critical (blocks ship) — **4 tasks**

- [ ] **T1.1** CR-01: Send button accepts attachment-only messages
  - File: `InputBar.tsx:669-675`
  - Fix: `disabled={!value.trim() && attachments.length === 0}`
- [ ] **T1.2** CR-02: File upload MIME whitelist + clear error feedback
  - File: `InputBar.tsx:169-192`
  - Add `validateFile()` helper; surface per-file errors
- [ ] **T1.3** CR-03: Filename rendering with `sanitizeFilename` + `dir="ltr"` (depends T0.2)
  - File: `InputBar.tsx:479`
- [ ] **T1.4** CR-04: Attachment handler 10MB strict limit + 413 (depends T0.1)
  - File: `desktop-server/lib.rs:947-1002`
  - Add test `attachments_rejects_oversized_payload`
- [ ] **Checkpoint 1**: cargo test + tsc + HTTP smoke (small file OK, big file 413)

---

## Phase 2 — Important validation (parallel) — **3 tasks**

- [ ] **T2.1** IM-01: `create_session` validates `project_path`
  - File: `desktop-server/lib.rs:595-604`
  - Test: `create_session_rejects_traversal`
- [ ] **T2.2** IM-02: `create_scheduled_task` validates `project_path`
  - File: `desktop-server/lib.rs:606-619`
  - Test: `create_scheduled_task_rejects_traversal`
- [ ] **T2.3** IM-05: `forward_permission` fail-fast on missing requestId/decision
  - File: `desktop-server/lib.rs:1157-1170`
  - Test: `forward_permission_rejects_missing_fields`
- [ ] **Checkpoint 2**: cargo test + HTTP smoke

---

## Phase 3 — Important reliability (parallel) — **2 tasks**

- [ ] **T3.1** IM-03: `u32::try_from` cap-and-log instead of panic
  - File: `desktop-core/lib.rs:4303-4313`
  - Test: `response_with_many_blocks_does_not_panic`
- [ ] **T3.2** IM-04: Tool execution wrapped in 120s `tokio::time::timeout`
  - File: `desktop-core/agentic_loop.rs:556-560`
  - Env override: `OCL_TOOL_TIMEOUT_SECS`
  - Test: new long-running mock tool triggers timeout
- [ ] **Checkpoint 3**: cargo test + stress loop 50×

---

## Phase 4 — Suggestions (batched) — **9 tasks**

- [ ] **T4.1** SG-01: Debug routes `#[cfg(debug_assertions)]` + `OCL_ENABLE_DEBUG` env gate
- [ ] **T4.2** SG-02: Permission mode turn-caching doc comment
- [ ] **T4.3** SG-03: `strip_yaml_frontmatter` `get()` instead of slice
- [ ] **T4.4** SG-04: CLI `--json` sensitive-field redaction + unit test
- [ ] **T4.5** SG-05: Drag-drop folder rejection
- [ ] **T4.6** SG-06: Sidebar context menu listener stability
- [ ] **T4.7** SG-07: CORS policy doc comment
- [ ] **T4.8** SG-08: Attachment chip stable `id` key
- [ ] **T4.9** SG-09: Handler naming consistency (`_handler` suffix batch rename) — **do last**
- [ ] **Final Checkpoint**: full regression + optional commit

---

## Exit criteria

- [ ] 0 Critical remaining
- [ ] 0 Important remaining
- [ ] Test count ≥ 90 passing
- [ ] `cargo check --workspace --all-targets` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] CLI 12 commands smoke-tested
- [ ] Backend HTTP 30+ cases all pass

---

## Commit strategy

- **Option A (recommended)**: 4 commits — one per phase, clean history
- **Option B**: 1 squashed commit at the end

Per user standing rule: **do not push until explicitly told**.

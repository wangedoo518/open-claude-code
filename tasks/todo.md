# Task Checklist (v2 — post Zustand migration)

## Phase 1-5: Rust Agent Runtime ✅
- [x] `agentic_loop.rs` — async loop + PermissionGate + streaming SSE
- [x] `system_prompt.rs` — CLAUDE.md discovery + tool descriptions
- [x] Wire into `append_user_message` + `finalize_agentic_turn`
- [x] CancellationToken for cancel_session
- [x] `cargo check -p desktop-server` passes

## Phase 6: Redux → Zustand (done by upstream) ✅
- [x] Zustand stores: permissions, settings, code-tools, tabs, minapps
- [x] Session dual state eliminated (React Query only)
- [x] Redux completely removed

## Phase 7: SSE Events → Zustand
- [ ] **7.1** Extend `api/client.ts` SSE with `permission_request` + `text_delta`
- [ ] **7.2** Create `streaming-store.ts` (isStreaming, streamingContent)
- [ ] **7.3** Wire handlers in `SessionWorkbenchPage.tsx`
- [ ] **Checkpoint**: `tsc --noEmit` passes

## Phase 8: InputBar Command Cleanup
- [ ] **8.1** Remove 6 stub commands, add 4 missing ones
- [ ] **Checkpoint**: InputBar ↔ commandExecutor 1:1

## Phase 9: E2E Test + Robustness
- [ ] **9.1** Incremental persistence during agentic loop
- [ ] **9.2** Error recovery (API error, tool panic, max iterations)
- [ ] **9.3** Verify gates/tokens cleanup in finalize
- [ ] **Checkpoint**: `cargo test` all pass

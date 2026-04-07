# Task Checklist (v3 — current state)

## Phase 1-5: Rust Agent Runtime ✅
- [x] `agentic_loop.rs` — async loop + PermissionGate + streaming SSE
- [x] `system_prompt.rs` — CLAUDE.md discovery + tool descriptions
- [x] Wire into `append_user_message` + `finalize_agentic_turn`
- [x] CancellationToken for cancel_session
- [x] `cargo check -p desktop-server` passes

## Phase 6: Redux → Zustand (upstream) ✅
- [x] Zustand stores: permissions, settings, code-tools, tabs, minapps
- [x] Session dual state eliminated (React Query only)
- [x] Redux completely removed

## Phase 7: SSE Events → Zustand ✅
- [x] Extend `api/client.ts` SSE with `permission_request` + `text_delta`
- [x] Create `streaming-store.ts` (isStreaming, streamingContent, isPlanMode)
- [x] Wire handlers in `SessionWorkbenchPage.tsx`

## Phase 8: InputBar Command Cleanup ✅
- [x] Remove 6 stub commands, add 4 missing ones

## Phase 9: Robustness ✅
- [x] Error recovery: API errors → visible error message + graceful return
- [x] Max iterations → system message explaining limit
- [x] Tool output truncation at 100KB
- [x] Incremental persistence via `on_iteration_complete` callback
- [x] gates/tokens cleanup in `finalize_agentic_turn`

## Phase 10: Session Compaction ✅
- [x] Auto-compact in agentic loop (runtime::should_compact + compact_session)
- [x] Backend API: POST /sessions/{id}/compact
- [x] /compact command calls backend API

## Phase 11: TodoWrite Frontend ✅
- [x] TodoMessage component with status icons + progress counter
- [x] Detect TodoWrite tool_results and render dedicated UI

## Phase 12: Plan Mode Indicator ✅
- [x] isPlanMode in streaming-store
- [x] StatusLine badge with FileSearch icon

## Phase 13: Enhanced Tool Rendering ✅
- [x] GrepResult component (file:line highlighting)
- [x] StreamingIndicator (real-time text + blinking cursor)
- [x] Tool name alias mapping (read_file, write_file, etc.)

## Critical Bug Fixes ✅
- [x] CWD set to project_path before tool execution
- [x] /compact wired to backend (was UI-only)

---

## Remaining (Not Started)

### Phase 14: Hooks System
- [ ] PreToolUse / PostToolUse lifecycle hooks
- [ ] Hook configuration loading from .claude/settings.json

### Phase 15: Windows Terminal Support
- [ ] cc_switch_terminal.rs Windows/Linux implementation
- [ ] Terminal detection for Windows (cmd, PowerShell, Windows Terminal)

### Phase 16: MCP Client Runtime
- [ ] stdio transport MCP client
- [ ] Tool discovery from MCP servers
- [ ] MCP tools exposed to LLM alongside built-in tools

### Phase 17: Sub-agent Management UI
- [ ] Agent tool output → dedicated SubagentPanel interaction
- [ ] Sub-agent creation/monitoring from frontend

### Phase 18: Session Fork/Branch
- [ ] Fork session from specific message
- [ ] Branch visualization in sidebar

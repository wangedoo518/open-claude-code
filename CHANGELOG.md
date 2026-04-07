# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] — 2026-04-07

This release closes 15 audit-discovered issues, integrates 5 features
inspired by `craft-agents-oss`, and ships a new CLI client.

### Added — Frontend (UI)

- **Session lifecycle workflow** — Sidebar shows inbox/in-progress/
  needs-review/done/archived status badges and a flag icon. Right-click
  context menu lets users move sessions through the workflow. (`5b7c77b`)
- **Workspace skills panel** — Status bar shows `N skills` badge that
  opens a popover listing each loaded skill with its description and
  source path. (`eae8778`)
- **Drag-drop attachments** — `InputBar` accepts file drops or
  paperclip-button uploads. PDFs, images, and text files are processed
  by the backend and prepended to the next message as a markdown block.
  10 MB per-file cap. (`14e8d4f`)
- **Streaming text indicator** — Real-time text from `TextDelta` SSE
  events with RAF batching at ~60Hz. (`c120b76`)
- **Plan Mode badge** in the status line. (`6c5463a`)

### Added — Backend (`desktop-core`)

- **Async agentic loop** with permission gate, cancel propagation,
  cancellation-aware HTTP+SSE+permission waits, and 60Hz streaming.
  (`9391ad8`, `7aabbef`)
- **Real MCP integration** — Persistent `McpServerManager` bypasses
  the vendored crate's private global registry. Subprocesses are
  reused across tool calls. (`d6ce4fb`)
- **Workspace skills loader** — `system_prompt::find_workspace_skills`
  reads `.claude/skills/*.md` (file form) and `.claude/skills/*/SKILL.md`
  (directory form). YAML front matter is stripped. (`9e87e48`)
- **AES-256-GCM secure storage** — `secure_storage` module + Qwen
  credential store wired through it. Auto-migrates existing plaintext
  files on first save. (`3578304`, `d69df4d`)
- **File attachment processing** — `attachments::process_attachment`
  handles PDF (via `pdf-extract`), images (base64), text formats
  (UTF-8 decode), Office stubs. UTF-8-safe truncation at 50 KB.
  (`53ff8e4`)
- **Direct ANTHROPIC_API_KEY mode** — 4-step credential chain: env
  var → `direct_api_key` in settings.json → codex OAuth → qwen OAuth.
  (`1ede464`)
- **Session lifecycle workflow backend** — `DesktopLifecycleStatus`
  enum, auto-transitions on send/finalize, two new HTTP routes.
  (`d65f0f4`)
- **Startup reconcile** — Sessions stuck in `Running` from a previous
  crash are reset to `Idle` on backend startup. (`42cd302`)
- **Performance optimizations** — Removed per-iteration disk persist
  (18× faster turns). Connection-pooled `reqwest::Client`. Unified
  CWD workspace lock. FIFO persist queue. (`05a7feb`, `631307b`)

### Added — `desktop-cli` (new crate)

- **`ocl` binary** with 8 command groups: `health`, `sessions`,
  `mcp`, `permission-mode`. Supports `--server`, `--json`,
  `OCL_SERVER` env var. (`963eb14`)

### Added — Documentation

- `docs/audit-lessons.md` — 15 archived bug stories with root
  cause and fix
- `docs/performance-report.md` — long-session benchmark results
- `docs/getting-started.md` — full setup walkthrough
- This `CHANGELOG.md`

### Added — CI

- GitHub Actions 3-job pipeline: Rust tests + warnings, frontend
  TypeScript check, audit-guard regression checks (L-09 + L-06).
  (`d572765`)

### Fixed (15 audit lessons L-01 → L-15)

- L-01 PermissionGate timeout vs resolve race (`6249672`)
- L-02 on_iteration_complete out-of-order writes (`631307b`)
- L-03 Drop guard async-spawn failure on shutdown (`42cd302`)
- L-04 cancel_token doesn't interrupt HTTP/permission wait (`7aabbef`)
- L-05 SSE multi-byte UTF-8 corrupted across chunks (`157dc64`)
- L-06 permissionMode dual source-of-truth (`6249672`)
- L-07 isStreaming vs turn_state divergence (`06e8734`)
- L-08 two independent CWD process locks (`631307b`)
- L-09 MCP "discovered" but not callable (`13c038b` → `d6ce4fb`)
- L-10 fork_session loses parent state (`56d377f`, `f37235d`)
- L-11 /compact optimistic UI without rollback (`157dc64`)
- L-12 hooks system config source not wired (`7aabbef`)
- L-13 tool_use input not validated as object (`157dc64`)
- L-14 tool output truncate panics on UTF-8 boundary
- L-15 CLAUDE.md ancestor-directory injection unwarned (`56d377f`)

### Fixed — Other

- All 10 `desktop-core` cargo warnings cleared. CI now enforces
  zero-warnings via `RUSTFLAGS="-D warnings"`. (`03b820d`)

### Performance

- **Per-turn latency:** 13.6ms → 740µs (~18× faster) for a 20-iteration
  turn, by deferring disk flush to `finalize_agentic_turn` and
  removing per-iteration persist calls. (`05a7feb`)

### Test coverage

- **62 unit tests** + 1 ignored benchmark (was 10 at start of audit)
- **MCP E2E** verified with Python mock server, single-process reuse
  confirmed across 3 tool calls
- **Attachments E2E** verified for markdown / Chinese+emoji / PNG

---

## Acknowledgements

Several features in this release were inspired by
[`lukilabs/craft-agents-oss`](https://github.com/lukilabs/craft-agents-oss):
session lifecycle workflow, workspace skills layer, AES-256-GCM
encrypted credentials, CLI client, and drag-drop attachments. The
implementation is independent — only the design pattern was borrowed.

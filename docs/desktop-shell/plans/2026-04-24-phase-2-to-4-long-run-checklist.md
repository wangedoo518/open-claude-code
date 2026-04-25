---
title: Phase 2-4 Long-Run Execution Checklist
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-04-25
related:
  - docs/desktop-shell/plans/2026-04-24-phase-2-closure.md
  - docs/desktop-shell/plans/2026-04-24-phase-2-readiness-audit.md
  - docs/desktop-shell/architecture/overview.md
  - rust/README.md
  - backlog/phase1-deferred.md
---

# Phase 2-4 Long-Run Execution Checklist

This checklist turns the Phase 1 review into a long-running execution rail.
The operating rule stays the same: `pre-land scan -> audit -> gap-fill ->
trust-but-verify`. Do not rewrite a surface until the scan proves it is truly
missing.

## Current Baseline

- [x] Phase 1 MVP absorb path is reachable from the Wiki header.
- [x] Global absorb progress SSE is wired into the frontend store.
- [x] Local `.claw/providers.json` is ignored by git and active for provider
  fallback.
- [x] Provider fallback tolerates UTF-8 BOM in local JSON configs.
- [x] Ask `?question` and `/query question` entrypoints are pre-landed.
- [x] WeChat Kefu `?` / `？` classification is pre-landed and covered by tests.
- [x] Phase 2 readiness matrix is recorded in
  `2026-04-24-phase-2-readiness-audit.md`.

## Always-On Guardrails

- [ ] Before every sprint, run a pre-land scan for the exact endpoint, hook,
  component, route, store, and test names in scope.
- [ ] Do not add new imports from feature modules into `src/lib/tauri.ts`.
- [ ] Do not add new public Wiki repository consumers to
  `features/ingest/persist.ts`; prefer `src/api/wiki` or `src/domain/wiki`.
- [ ] If a route is added, update the canonical shell route config and derive
  sidebar, palette, and `<Routes>` from that same config.
- [ ] If `desktop-server/src/lib.rs` gets a new handler before the route split,
  mark it as temporary and keep the change narrow.
- [ ] Any architecture-facing change must update the current-truth docs under
  `docs/desktop-shell/architecture`, `tokens`, or `operations`.
- [ ] Keep local runtime secrets in ignored files only; never put API keys into
  tracked docs or code.

## Phase 2: Readiness And User-Visible Closure

- [x] Unlock provider runtime locally with ignored `.claw/providers.json`.
- [x] Verify provider fallback with a real `BrokerAdapter -> query_wiki` smoke.
- [x] Replace AbsorbTrigger completion polling with SSE/store updates.
- [x] Move Ask query source DTOs out of `features/ingest`.
- [x] Add or confirm HTTP-level `/api/wiki/query` smoke once standalone server
  startup is decoupled from WeChat monitor readiness.
- [x] Add a mock/handler E2E for Kefu `?` query reply with sources.
- [x] Add a mock/handler E2E for URL/text ingest -> conflict notification.
- [x] Confirm empty-wiki query returns a friendly UI error in Ask.
- [x] Confirm `query_done.sources` renders and opens the matching Wiki tab.
- [x] Close UX backlog item 11: Wiki file tree keyboard up/down navigation.
- [x] Close UX backlog item 12: Wiki article `confidence` and `last_verified`
  display.
- [x] Close update-branch LLM merge and W2 proposal/apply HTTP smoke.
- [x] Close bidirectional link visibility for frontend wikilinks.

Phase 2 status: closed at code-readiness level. The only residual is live
enterprise WeChat/device E2E, tracked as an environment validation item rather
than an application-code blocker.

## Phase 2.5: Architecture Debt Slice

- [x] Route config single source of truth:
  `CLAWWIKI_ROUTES`, sidebar, palette, and `<Routes>` must derive from one
  canonical route config, including `/connect-wechat`.
- [ ] Frontend API boundary:
  split `src/lib/tauri.ts` into neutral contracts and API clients under
  `src/api/desktop`, `src/api/wiki`, `src/api/wechat`, and
  `src/api/contracts`.
- [x] Desktop API first slice:
  move session and settings/provider/wechat desktop HTTP clients under
  `src/api/desktop`, with old feature paths kept as compatibility re-exports.
- [x] Desktop API consumer migration:
  move active settings, WeChat, Ask, Dashboard, and private-cloud imports from
  `features/settings/api/*` to `src/api/desktop/*`; keep old feature paths as
  compatibility re-exports only.
- [x] Wiki repository boundary:
  migrate common Wiki data access from `features/ingest/persist.ts` to
  `src/api/wiki` or `src/domain/wiki/repository`.
- [x] Inbox scorer boundary:
  move target resolver/scoring helpers into a domain service instead of
  dynamic importing across feature folders.
- [x] Rust server route split first slice:
  extract route assembly into `routes/desktop`, `routes/wiki`,
  `routes/wechat`, and `routes/internal`; keep handler bodies in `lib.rs` for
  a lower-risk follow-up slice.
- [x] Rust server handler split first slice:
  move Wiki report/maintenance handlers (`cleanup`, `patrol`, `absorb-log`,
  `backlinks`, `stats`, `patrol/report`, `schema/templates`) into
  `handlers/wiki_reports.rs` while keeping route names stable through crate
  re-exports.
- [x] Rust server handler split second slice:
  move Wiki task handlers (`absorb`, `absorb/events`, `query`) and their DTOs
  into `handlers/wiki_tasks.rs` while keeping route names stable through crate
  re-exports.
- [x] Rust server handler split third slice:
  move Codex runtime/auth and providers.json CRUD handlers into
  `handlers/provider_runtime.rs` while keeping desktop route names stable
  through crate re-exports.
- [x] Rust server handler split fourth slice:
  move desktop/ask session lifecycle, source binding, session SSE, compaction,
  and permission forwarding handlers into `handlers/desktop_sessions.rs` while
  keeping route names stable through crate re-exports.
- [x] Rust server handler split fifth slice:
  move desktop bootstrap/settings, scheduled/dispatch CRUD, attachments,
  workspace skills, MCP debug, and permission-mode handlers into
  `handlers/desktop_utilities.rs` while keeping route names stable through
  crate re-exports.
- [ ] Rust server handler split follow-up:
  continue moving handler DTOs and implementations out of `lib.rs` by domain;
  suggested next slices are WeChat account/Kefu handlers, desktop storage
  utility handlers, then inbox/raw/wiki page CRUD handlers.
- [x] Current-truth docs:
  refresh `docs/desktop-shell/architecture/overview.md` and `rust/README.md`
  whenever a slice lands.

## Phase 3: Patrol And Quality

- [x] Promote `wiki_patrol` from local checks to a user-visible dashboard
  signal.
- [ ] Add orphan, stale, schema violation, oversized, stub, confidence decay,
  and uncrystallized detectors as explicit patrol categories.
- [ ] Add quality sampling for LLM-maintained pages.
- [ ] Connect patrol results to Dashboard cards and Inbox actions.
- [ ] Add tests that keep `wiki_stats` and `wiki_patrol` orphan semantics in
  sync.

## Phase 4: Power Tools And Viewer

- [x] Graph View enhancements first slice: relation-kind filters for
  `derived-from` source edges and `references` wikilink edges.
- [ ] Graph View enhancements follow-up: backlinks and source
  drilldown.
- [ ] `/cleanup`: patrol-backed cleanup proposal flow.
- [ ] `/breakdown`: split oversized or mixed-topic pages into maintainable
  targets.
- [ ] Settings Modal final sweep: provider, storage, WeChat, and runtime health.
- [ ] Web viewer: stable read-only view for wiki pages and graph entrypoints.

## Verification Cadence

- [ ] Frontend minimum: `cd apps/desktop-shell && npm run build`.
- [ ] Tauri minimum: `cd apps/desktop-shell/src-tauri && cargo check`.
- [ ] Rust minimum: `cd rust && cargo check --workspace`.
- [ ] Query tests: `cargo test -p wiki_maintainer query_wiki` and
  `cargo test -p desktop-server query_done`.
- [ ] Kefu tests: `cargo test -p desktop-core wechat_kefu`.
- [ ] Provider fallback tests: `cargo test -p desktop-core provider_config`.
- [ ] Before commit: `git diff --check` and `git status --short`.

## Current Next Slice

- [x] Phase 2.5 route config single source of truth.
- [x] Then frontend API boundary split for desktop/settings/ask clients.
- [x] Then Wiki repository extraction from `features/ingest/persist.ts`.
- [x] Then Rust route assembly split into domain route modules.
- [x] Then handler-body split first slice for Wiki report/maintenance handlers.
- [x] Then handler-body split second slice for query/absorb task handlers.
- [x] Then handler-body split third slice for provider/runtime handlers.
- [x] Then handler-body split fourth slice for desktop session handlers.
- [x] Then handler-body split fifth slice for desktop utility handlers.
- [ ] Next: continue handler-body split with WeChat account/Kefu handlers or
  desktop storage utilities, after Phase 2 closure remains green.

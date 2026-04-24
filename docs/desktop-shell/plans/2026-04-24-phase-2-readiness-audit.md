---
title: Phase 2 Readiness Audit
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-04-24
related:
  - backlog/phase1-deferred.md
  - docs/design/technical-design.md
  - docs/design/modules/01-skill-engine.md
  - docs/design/modules/04-wechat-kefu.md
  - docs/desktop-shell/architecture/overview.md
---

# Phase 2 Readiness Audit

Phase 1 MVP closed with the `pre-land scan -> audit -> gap-fill -> trust-but-verify`
loop validated. Phase 2 should keep that loop: verify what already landed, fill
small user-visible gaps first, and defer broad architecture debt to a dedicated
Phase 2.5 slice.

## Readiness Matrix

| Area | Status | Evidence | Next work |
| --- | --- | --- | --- |
| Provider runtime fallback | Runtime unlocked | `.claw/providers.json` is ignored by git and `desktop-core/src/wiki_maintainer_adapter.rs` reads the active provider from the current directory or any ancestor project root. | Keep the file local-only. Smoke `/api/wiki/query` and `/api/wiki/absorb` with the configured provider. |
| `/api/wiki/query` backend | Mostly landed | `desktop-server/src/lib.rs` exposes `POST /api/wiki/query`; `wiki_maintainer::query_wiki` has source and empty-wiki coverage. | Add/keep focused tests for SSE terminal payload, source propagation, and empty-wiki friendly failure. |
| Ask query frontend | Mostly landed | `apps/desktop-shell/src/features/ask/useWikiQuery.ts` consumes `query_chunk`, `query_done`, and `query_error`; query source DTOs now live under `src/api/wiki/types.ts`. | Prefer the Ask hook for UI flow. Do not add new consumers to the ingest feature barrel. |
| WeChat Kefu `?` query | Mostly landed | `desktop-core/src/wechat_kefu/desktop_handler.rs` classifies `?` and `？`, calls `wiki_maintainer::query_wiki`, and formats sources in the reply. | Keep mock/handler coverage when no real WeChat device is available; run real device E2E only when credentials are present. |
| WeChat audit notification | Partially landed | `check_and_notify_conflicts` scans pending conflict inbox entries after URL/text ingest and sends a Kefu notification. | Add focused coverage around conflict filtering and notification formatting before changing behavior. |
| Absorb progress UI | Gap confirmed | Backend emits `absorb_progress` / `absorb_complete`, but `AbsorbTriggerButton` still polls `getWikiStats().last_absorb_at`. | Replace polling with SSE-backed store updates and keep a small timeout/error fallback. |
| Phase 2 UX polish | Deferred | Backlog items 11 and 12 cover file-tree keyboard navigation and article `confidence` / `last_verified` display. | Start only after provider + query + absorb progress are green. |

## Phase 2.5 Guardrails

The following review findings are real architecture debt, but they should not
block Phase 2 readiness unless a Phase 2 change would make them worse.

- Do not add new feature imports to `src/lib/tauri.ts`; new API clients should live under a neutral API/lib layer.
- Do not add new Wiki repository consumers to `features/ingest/persist.ts`; migrate consumers toward `src/api/wiki` or `src/domain/wiki` in Phase 2.5.
- Keep any new `desktop-server/src/lib.rs` route additions minimal and document them as temporary until route modules land.
- Treat `CLAWWIKI_ROUTES` and the hard-coded `<Routes>` list as drift-prone; do not add new navigation entries without updating both.
- Refresh `docs/desktop-shell/architecture/overview.md` and `rust/README.md` when Phase 2.5 starts so AGENTS-facing docs match implementation.

## Execution Order

1. Keep `.claw/providers.json` local and active.
2. Wire absorb progress to SSE/store and remove `last_absorb_at` polling from the trigger.
3. Run targeted query and Kefu handler tests.
4. Smoke the provider-backed `/api/wiki/query` path.
5. Only then start UX polish items 11 and 12.

## Sprint 1 Scan Update

- Ask already has reachable typed query entrypoints in `AskWorkbench`: `?question`
  and `/query question` both route through `useWikiQuery`.
- The design-era `QuickActionsBar` entrypoint is not present in the current
  codebase; typed Ask queries are the current reachable UI path.
- Kefu `?` and full-width `？` routing is pre-landed in
  `desktop-core/src/wechat_kefu/desktop_handler.rs`; Sprint 1 keeps that path
  testable by extracting conflict-notification formatting into pure helpers.
- Query result source DTOs moved to `src/api/wiki/types.ts`, so Ask no longer
  imports query contracts from the ingest feature layer.

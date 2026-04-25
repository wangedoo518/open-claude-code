---
title: Desktop Shell Operations
doc_type: operation
status: active
owner: desktop-shell
last_verified: 2026-04-25
source_of_truth: true
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/architecture/overview.md
---

# Desktop Shell Operations

This document answers: how to maintain and verify `desktop-shell`.

## Required Updates

- Update architecture docs when product structure changes.
- Update tokens when shared UI or functional language changes.
- Update `AGENTS.md` only when navigation or documentation rules change.
- Update `apps/desktop-shell/src/state/` docs and storage behavior together when adding or removing a persisted domain.

## Verification Commands

- `cd apps/desktop-shell && npm run build`
- `cd apps/desktop-shell/src-tauri && cargo check`
- `cd rust && cargo check --workspace`
- `git diff --check`

## Phase 5 Smoke

Run the Phase 5 power-tools regression smoke from the repository root:

```bash
npm run smoke:phase5
```

The smoke creates a temporary `CLAWWIKI_HOME`, builds and starts the real
`desktop-server`, verifies `/api/wiki/cleanup?apply=false` and
`/api/wiki/breakdown` over HTTP, builds `apps/desktop-shell` with a temporary
`VITE_DESKTOP_API_BASE`, starts Vite preview, then drives `/viewer`,
`/viewer/wiki/phase5-source`, and `/viewer/graph` in a real browser through
`playwright-cli`. It requires `cargo`, `npm`, and `npx`; `playwright-cli` is
downloaded through `npx --package @playwright/cli` when the smoke runs.

## State Verification

- Verify local state consumers import from `@/state/*` instead of `@/store`.
- Keep Router, TanStack Query, and Zustand ownership boundaries aligned with `docs/desktop-shell/architecture/overview.md`.
- When changing persistence, preserve compatibility with the legacy `persist:open-claude-code` payload or document the migration explicitly.

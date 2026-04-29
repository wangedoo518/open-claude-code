---
title: Desktop Shell Operations
doc_type: operation
status: active
owner: desktop-shell
last_verified: 2026-04-29
source_of_truth: true
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/specs/2026-04-29-buddy-tolaria-deep-product-design.md
---

# Desktop Shell Operations

This document answers: how to maintain and verify `desktop-shell`.

## Required Updates

- Update architecture docs when product structure changes.
- Update tokens when shared UI or functional language changes.
- Update `AGENTS.md` only when navigation or documentation rules change.
- Update `apps/desktop-shell/src/state/` docs and storage behavior together when adding or removing a persisted domain.

## Main-only Workflow

Buddy adopts a Tolaria-style main-only workflow for `desktop-shell` work.
Main-only means the quality loop moves earlier; it does not mean skipping
design, tests, review discipline, or documentation.

- Keep changes small, reviewable, and reversible.
- Write or update a spec before substantial product, architecture, token, or
  workflow changes.
- Split approved specs into plans before implementation.
- Land each implementation slice with its relevant tests and verification.
- Do not rely on a later cleanup branch to satisfy minimum quality gates.
- When changing Buddy Vault, Git, Rules Studio, external AI writes, or schema
  semantics, update architecture/tokens/operations docs in the same slice.
- Buddy Vault remote sync must preserve a clean-worktree checkpoint discipline:
  pull/push only after local changes are committed, pull with fast-forward-only
  semantics, and surface Git non-fast-forward/conflict output instead of
  auto-merging.
- Remote URL setup writes to Git config and status surfaces only redacted URLs;
  do not log or duplicate credential-bearing remote URLs in Buddy-owned files.
- When copying Tolaria source, include provenance, license preservation, tests,
  and documentation in the same main-only slice.

## Quality Discipline

- **Spec/plan first**: product structure, shared interaction models, token
  contracts, Git behavior, external AI write scopes, and Vault format changes
  require a spec and a plan.
- **TDD where behavior is shared**: queue scoring, schema validation, purpose
  assignment, Git diff/commit flows, and controlled external writes should have
  unit or integration tests before broad UI work.
- **Quality gates**: run the minimum command set that matches the touched
  surface; include `git diff --check` for every change set.
- **Native QA**: verify shell-level changes in realistic desktop window sizes,
  including light/dark, narrow width, long Chinese text, empty state, loading
  state, error state, and permission/authorization state.
- **Documentation sync**: stable results move from specs/plans into
  `architecture/`, `tokens/`, or `operations/` so there is one current truth.

## Tolaria Source Reuse

Buddy may copy Tolaria source only as an intentional, reviewed engineering
decision. Treat copied or derived code as licensed source, not as informal
reference material.

- Confirm the team accepts the relevant `AGPL-3.0-or-later` obligations before
  merging copied or derived Tolaria code.
- Preserve copyright notices, license headers, and attribution when present.
- Record the Tolaria source path, source commit/ref, copied scope, Buddy target
  path, modifications, reviewer, and follow-up obligations in the spec/plan or
  PR description.
- Keep copied slices small and auditable; prefer layout, token, command, or
  workflow primitives over wholesale feature drops.
- Apply the same TDD, quality gates, native QA, and documentation sync rules as
  first-party Buddy code.
- Review `.pen` design assets, visual assets, and product copy with the same
  license/provenance discipline as source code.

## Verification Commands

- `cd apps/desktop-shell && npm run build`
- `cd apps/desktop-shell && BUDDY_API_BASE=http://127.0.0.1:4358 BUDDY_SMOKE_URL=http://127.0.0.1:5174/ npm run test:buddy:smoke`
- `cd apps/desktop-shell/src-tauri && cargo check`
- `cd rust && cargo check --workspace`
- `cd rust && cargo test -p wiki_store`
- `git diff --check`

## Known Issues

- Current desktop shell known issues and demo-readiness gaps are tracked in `docs/desktop-shell/operations/KNOWN_ISSUES.md`.

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

## Buddy Tolaria Smoke

Run the Tolaria-inspired shell smoke with a real desktop-server and Vite app:

```bash
cd apps/desktop-shell
BUDDY_API_BASE=http://127.0.0.1:4358 BUDDY_SMOKE_URL=http://127.0.0.1:5174/ npm run test:buddy:smoke
```

The smoke expects the desktop-server to be reachable at `BUDDY_API_BASE` and
the app to be reachable at `BUDDY_SMOKE_URL`. It verifies Home/Pulse, Rules
Studio folded Advanced state, Connections, Knowledge, the global status bar,
and absence of runtime error boundaries, then seeds a wiki page fixture and
drives the `/wiki/{slug}` CodeMirror edit/save flow.

## State Verification

- Verify local state consumers import from `@/state/*` instead of `@/store`.
- Keep Router, TanStack Query, and Zustand ownership boundaries aligned with `docs/desktop-shell/architecture/overview.md`.
- When changing persistence, preserve compatibility with the legacy `persist:open-claude-code` payload or document the migration explicitly.

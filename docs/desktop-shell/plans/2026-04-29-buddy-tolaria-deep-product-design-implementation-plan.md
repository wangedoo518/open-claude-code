---
title: Buddy Tolaria Deep Product Design Implementation Plan
doc_type: plan
status: implemented
owner: desktop-shell
last_verified: 2026-04-29
source_of_truth: false
related:
  - docs/desktop-shell/specs/2026-04-29-buddy-tolaria-deep-product-design.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/operations/README.md
---

# Buddy Tolaria Deep Product Design Implementation Plan

This plan records the first main-only implementation slice for the
Tolaria-inspired Buddy design.

## Implemented Slice

- Route IA changed to `/` Home/Pulse, `/rules`, `/connections`, `/wiki`,
  `/inbox`, and `/ask` as the main rail surfaces.
- Legacy `/dashboard`, `/schema`, `/wechat`, `/raw`, `/graph`, `/cleanup`,
  `/breakdown`, `/viewer`, and `/connect-wechat` routes remain mounted for
  compatibility and command-palette access.
- Home/Pulse now presents an external-brain health check backed by Inbox,
  Stats, Patrol, Vault/Git, and external-AI authorization state.
- Knowledge and Rules mount a Tolaria-style 250px secondary sidebar by default.
- Global BuddyStatusBar shows health, Inbox, Git/Vault, permission, and
  external-AI read/write status.
- Purpose Lens defaults include `writing`, `building`, `operating`, `learning`,
  `personal`, and `research`.
- Wiki pages expose full Markdown/YAML editing and save through
  `PUT /api/wiki/pages/{slug}` with validation and audit log entry
  `human-edit-wiki-page`.
- Rules Studio replaces the old Schema Editor surface; Advanced YAML/CodeMirror
  is folded by default.
- Connections exposes controlled-write authorization levels: session grant and
  permanent rule.
- Buddy Vault initialization seeds root `AGENTS.md` / `CLAUDE.md` shims,
  `schema/purpose-lenses.yml`, personal/research templates, `.gitignore`, and
  Git by default when Git is available.

## Verification

- `cd apps/desktop-shell && npm run build`
- `cd apps/desktop-shell/src-tauri && cargo check`
- `cd rust && cargo check --workspace`
- `cd rust && cargo test -p wiki_store`

## Follow-up Slices

- Add a full Git status/diff/commit endpoint and replace placeholder UI badges
  with live Git state.
- Add CodeMirror 6 as the concrete editor implementation for Wiki and Rules.
- Add external AI write-policy persistence and revocation endpoints.
- Add browser smoke coverage for Home/Pulse, Rules Studio folded state,
  Wiki edit save, and Connections authorization copy.

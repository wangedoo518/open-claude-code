---
title: Desktop Shell Architecture Overview
doc_type: architecture
status: active
owner: desktop-shell
last_verified: 2026-04-29
source_of_truth: true
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/specs/2026-04-29-buddy-tolaria-deep-product-design.md
  - docs/desktop-shell/plans/2026-04-29-buddy-tolaria-deep-product-design-implementation-plan.md
  - docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md
---

# Desktop Shell Architecture Overview

This document answers: how `desktop-shell` is currently organized.

## Application Layers

- App shell and routing. `apps/desktop-shell/src/shell/clawwiki-routes.tsx`
  is the canonical route config; sidebar navigation, command-palette route
  entries, and `<Routes>` are derived from the same list.
- Main rail surfaces are now `/` Home/Pulse, `/ask`, `/inbox`, `/wiki`,
  `/rules`, `/connections`, and `/settings`. Legacy surfaces such as
  `/dashboard`, `/schema`, `/wechat`, `/raw`, `/graph`, `/cleanup`,
  `/breakdown`, `/viewer`, and `/connect-wechat` remain mounted for
  compatibility and command-palette access.
- `apps/desktop-shell/src/shell/BuddyStatusBar.tsx` is the global shell status
  bar for health, Inbox, Git/Vault, permission mode, and external-AI write
  posture.
- Knowledge and Rules receive a Tolaria-style 250px secondary sidebar from
  `apps/desktop-shell/src/shell/Sidebar.tsx`.
- Feature modules own UI and feature-specific orchestration.
- Neutral API clients under `apps/desktop-shell/src/api/` own cross-feature
  HTTP/SSE surfaces. Common Wiki repository access lives under
  `src/api/wiki`; desktop session/settings/provider clients live under
  `src/api/desktop`.
- Phase 4 power surfaces are mounted through the same route config:
  `/cleanup` previews/applies patrol-backed Inbox cleanup proposals,
  `/breakdown` previews/applies deterministic wiki-page split targets, and
  `/viewer/*` provides read-only wiki and graph entrypoints.
- Domain services under `apps/desktop-shell/src/domain/` own shared pure
  client-side business logic, such as Wiki target scoring and fallback
  resolution.
- Purpose Lens UI constants live in
  `apps/desktop-shell/src/features/purpose/purpose-lenses.ts`; the default
  frontmatter values are `writing`, `building`, `operating`, `learning`,
  `personal`, and `research`.
- Wiki and Rules advanced editors use CodeMirror 6 through
  `apps/desktop-shell/src/components/CodeMirrorEditor.tsx`.
- Shared UI and utility layer
- Desktop integration layer

## Compatibility Shims

- `features/ingest/persist.ts` and `features/ingest/types.ts` are legacy
  compatibility re-exports for the neutral Wiki API layer.
- `features/inbox/candidate-scoring.ts` and
  `features/inbox/target-resolver.ts` are legacy compatibility re-exports for
  `src/domain/wiki`.
- `features/ask/api/client.ts`, `features/settings/api/client.ts`, and
  `features/settings/api/private-cloud.ts` are legacy compatibility re-exports
  for `src/api/desktop`.

## State Ownership

- Router owns navigational identity.
- TanStack Query owns remote state.
- Zustand owns local application state under `apps/desktop-shell/src/state/`.
- Persisted Zustand domains currently include `settings`, `command-palette`, and `wiki-tabs`.
- `ask-ui`, `permissions`, `skill-store`, and `streaming-store` are in-memory UI/runtime stores and are not persisted.
- Wiki maintenance progress is delivered through `/api/wiki/absorb/events`, a session-agnostic SSE stream backed by desktop-core SKILL events.

## Rust Integration Layer

- `rust/crates/desktop-server/src/routes/` owns route assembly by domain:
  `desktop`, `wiki`, `wechat`, and `internal`.
- `rust/crates/desktop-server/src/handlers/` owns migrated handler bodies by
  domain. Landed slices include `handlers/wiki_reports.rs` for Wiki cleanup,
  breakdown, patrol, absorb-log, backlinks index, stats, patrol report, and
  schema template endpoints, plus `handlers/wiki_tasks.rs` for absorb/query task endpoints and
  absorb progress SSE, plus `handlers/provider_runtime.rs` for Codex
  runtime/auth and providers.json CRUD endpoints, plus
  `handlers/desktop_sessions.rs` for desktop/ask session lifecycle, source
  binding, session SSE, compaction, and permission forwarding, plus
  `handlers/desktop_utilities.rs` for desktop bootstrap/settings,
  scheduled/dispatch CRUD, attachments, skills, MCP debug, and permission-mode
  endpoints, plus `handlers/desktop_storage.rs` for storage migration,
  MarkItDown/WeChat fetch helpers, URL-ingest diagnostics, and environment
  doctor probes, plus `handlers/wiki_crud.rs` for raw/inbox/page CRUD,
  lineage, proposal, combined-merge, and inbox notification handlers.
- `PUT /api/wiki/pages/{slug}` is the human wiki edit path. It accepts complete
  Markdown including YAML frontmatter, validates required fields, writes
  atomically through `wiki_store::overwrite_wiki_page_content`, and appends
  `human-edit-wiki-page` to the wiki log.
- Buddy Vault Git is a first-class HTTP surface:
  `GET /api/wiki/git/status`, `GET /api/wiki/git/diff`, and
  `POST /api/wiki/git/commit` wrap `wiki_store::vault_git_*` helpers for live
  status, diff preview, and checkpoint commits.
- External AI controlled-write authorization is stored under
  `.clawwiki/external-ai-write-policy.json`. The desktop server exposes
  `GET /api/wiki/external-ai/write-policy`,
  `POST /api/wiki/external-ai/write-policy/grants`, and
  `DELETE /api/wiki/external-ai/write-policy/grants/{id}` for read, grant, and
  revoke flows.
- `wiki_store::init_wiki` seeds Buddy Vault defaults: `raw/`, `wiki/`,
  `schema/`, `.clawwiki/`, root `AGENTS.md` / `CLAUDE.md` shims,
  `schema/purpose-lenses.yml`, personal/research templates, `.gitignore`, and
  Git initialization when Git is available.
- `desktop-server/src/lib.rs` owns shared `AppState`, common response types,
  private-cloud-only broker routes, shutdown wiring, and top-level Router
  assembly. New handler-body work should add domain modules instead of growing
  `lib.rs`.

## Change Policy

If these boundaries change, update this document in the same change set.

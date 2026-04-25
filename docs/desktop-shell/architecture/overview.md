---
title: Desktop Shell Architecture Overview
doc_type: architecture
status: active
owner: desktop-shell
last_verified: 2026-04-25
source_of_truth: true
related:
  - docs/desktop-shell/README.md
  - docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md
---

# Desktop Shell Architecture Overview

This document answers: how `desktop-shell` is currently organized.

## Application Layers

- App shell and routing. `apps/desktop-shell/src/shell/clawwiki-routes.tsx`
  is the canonical route config; sidebar navigation, command-palette route
  entries, and `<Routes>` are derived from the same list.
- Feature modules own UI and feature-specific orchestration.
- Neutral API clients under `apps/desktop-shell/src/api/` own cross-feature
  HTTP/SSE surfaces. Common Wiki repository access lives under
  `src/api/wiki`; desktop session/settings/provider clients live under
  `src/api/desktop`.
- Domain services under `apps/desktop-shell/src/domain/` own shared pure
  client-side business logic, such as Wiki target scoring and fallback
  resolution.
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
  patrol, absorb-log, backlinks index, stats, patrol report, and schema template
  endpoints, plus `handlers/wiki_tasks.rs` for absorb/query task endpoints and
  absorb progress SSE, plus `handlers/provider_runtime.rs` for Codex
  runtime/auth and providers.json CRUD endpoints.
- `desktop-server/src/lib.rs` still owns shared `AppState`, common response
  types, and handler bodies that have not yet moved. New handler-body split
  work should add domain modules instead of growing `lib.rs`.

## Change Policy

If these boundaries change, update this document in the same change set.

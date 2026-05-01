---
title: Desktop Shell Architecture Overview
doc_type: architecture
status: active
owner: desktop-shell
last_verified: 2026-05-01
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
- Command registry. `apps/desktop-shell/src/features/palette/command-manifest.ts`
  builds the testable command manifest from the same route config, including
  stable command ids, menu/palette flags, shortcut metadata, route coverage,
  and drift diagnostics. Route palette rows carry the matching `commandId`.
- Main rail surfaces are now `/` Home/Pulse, `/ask`, `/inbox`, `/wiki`,
  `/rules`, `/connections`, and `/settings`. Legacy surfaces such as
  `/dashboard`, `/schema`, `/wechat`, `/raw`, `/graph`, `/cleanup`,
  `/breakdown`, `/viewer`, and `/connect-wechat` remain mounted for
  compatibility and command-palette access.
- `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx` owns
  Home/Pulse as the external-brain health check: Inbox pressure, knowledge
  quality, Git/Vault state, external-AI write posture, and the latest local
  Git audit entry are summarized in the first viewport.
- `apps/desktop-shell/src/shell/BuddyStatusBar.tsx` is the global shell status
  bar for health, Inbox, Git/Vault, permission mode, and external-AI write
  posture. Status items link to Home/Pulse, Inbox, Connections Git, Settings
  permissions, Connections external-AI authorization, and Knowledge.
- `apps/desktop-shell/src/features/connections/ConnectionsPage.tsx` owns the
  Buddy Vault Git operator surface, including structured hunk/line diff review,
  checkpointing, remote sync, discard controls, and local Git audit display.
  Discard controls are intentionally scoped to unstaged changes; staged diffs
  are shown as read-only review material.
- `apps/desktop-shell/src/features/inbox/InboxPage.tsx` surfaces Git/Vault
  checkpoint pressure inside the review metrics row and invalidates Git state
  after Inbox mutations that can change the Vault. The page is wrapped in an
  `inbox-redesign-stage` flex container that places the existing 980px shell
  next to a 300px sticky right-side `InboxInspector` aside (hidden under
  1100px viewports). The Inspector reads the focused entry's recommended
  action, source raw, target slug, queue status, lineage events, and schema
  validation hint; it has no backend dependency beyond the existing inbox
  list and lineage queries.
- Knowledge and Rules receive a Tolaria-style 250px secondary sidebar from
  `apps/desktop-shell/src/shell/Sidebar.tsx`.
- `apps/desktop-shell/src/features/schema/SchemaEditorPage.tsx` owns Rules
  Studio. It keeps Advanced YAML / CodeMirror folded by default, reads live
  Git/Vault status, renders the `schema/templates/*.md` template catalog by
  default, renders root/schema guidance file status from `GET /api/wiki/guidance`,
  renders schema policy files from `GET /api/wiki/policies`, surfaces the
  latest patrol-backed Validation snapshot, and can edit allowlisted rule files
  through `GET/PUT /api/wiki/rules/file`. Rule file saves invalidate Git state
  after writing.
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
- Home/Pulse reads `GET /api/wiki/pages` alongside stats/patrol/Git state to
  render a weekly Purpose Lens digest: each default lens shows new absorbed
  pages, reusable/expressible pages, and recent page titles, with a warning
  when wiki pages lack `purpose`.
- Inbox Maintain decisions include a Purpose Lens picker. `create_new` writes
  the selected lenses into the new wiki page frontmatter, while
  `update_existing` merges reviewed lenses into the target page without
  dropping existing `expressed_in` refs.
- Wiki page summaries now carry optional `expressed_in` and `source_refs`
  frontmatter refs. Home uses `expressed_in` for the `最近表达` pulse and counts
  pages with source lineage in the health stats. Missing source lineage counts
  as a Home/Pulse follow-up action that links to Knowledge's `source=missing`
  filter, while the positive stat links to `source=sourced`. Purpose Lens
  digest counts expressed pages separately from pages that are still ready to
  express, Knowledge search/listing can scan and preview `source_refs`, and
  Wiki Article surfaces them as compact lineage chips. Page relations and the
  graph layer treat `source_refs` and `source_raw_id` as one normalized lineage
  signal for related-page scoring and `derived-from` edges.
- Wiki direct edit accepts the full Markdown/YAML file, but both the editor
  save panel and `PUT /api/wiki/pages/{slug}` validate schema-sensitive
  frontmatter fields (`type`, `status`, `schema`, `source_raw_id`, `purpose`,
  `expressed_in`, `source_refs`) before writing.
- Ask turns with an explicit `wiki:<slug>` source binding append
  `ask:<session-id>` to that page's `expressed_in` frontmatter once per
  session. This write is local-first and intentionally leaves Buddy Vault dirty
  so the existing Git checkpoint surfaces pick it up.
- Ask wiki queries (`?` prefix) crystallize substantive answers into
  `raw/query` and append a pending NewRaw Inbox task. The final SSE payload
  returns the created raw/inbox ids, and the Ask answer block links directly to
  Raw Library and Inbox review.
- Ask Purpose mode is carried on
  `POST /api/desktop/sessions/{id}/messages` as optional `purpose` values.
  The Rust layer normalizes the slugs, stores them on
  `ContextBasis.purpose_lenses`, and injects the Purpose Lens instruction into
  every Ask runtime path without changing the persisted user message.
- Wiki and Rules advanced editors use CodeMirror 6 through
  `apps/desktop-shell/src/components/CodeMirrorEditor.tsx`.
- Knowledge `/wiki` pages tab is wrapped in a `.ds-kb-stage` flex
  container that puts a 250px `KnowledgeFilterSidebar` (类型/目的/来源)
  on the left of the existing shell. The sidebar shares state with the
  toolbar selects and consumes the Slice 41 semantic token aliases
  (`--surface-sidebar`, `--state-selected`, `--accent-blue`).
- A Tolaria-style semantic token layer in `globals.css` exposes
  `--surface-*`, `--text-*`, `--border-*`, `--accent-blue/red`, and
  `--state-selected/hover` as aliases of the shadcn layer. Components
  are migrated opt-in; adoption does not require a token-rename sweep
  because the shadcn names continue to work in parallel.
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
  breakdown, patrol, absorb-log, backlinks index, stats, patrol report, schema
  template, guidance status, and policy status endpoints, plus
  `handlers/wiki_tasks.rs` for absorb/query task endpoints and
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
  `human-edit-wiki-page` to the wiki log. The wiki edit panel also reads live
  Buddy Vault Git status so the user can see whether the save will create a
  checkpointable diff before they leave the page.
- `GET/PUT /api/wiki/rules/file` is the Rules Studio human edit path for
  allowlisted files only: root `AGENTS.md` / `CLAUDE.md`, `schema/AGENTS.md`,
  `schema/CLAUDE.md`, `schema/purpose-lenses.yml`,
  `schema/templates/*.md`, and `schema/policies/*.md`. The backend rejects
  absolute paths, parent traversal, unknown files, and empty content.
- Buddy Vault Git is a first-class HTTP surface:
  `GET /api/wiki/git/status`, `GET /api/wiki/git/diff`, and
  `POST /api/wiki/git/commit`, `POST /api/wiki/git/pull`, and
  `POST /api/wiki/git/push`, `POST /api/wiki/git/remote`, and
  `POST /api/wiki/git/discard`, `POST /api/wiki/git/discard-hunk`,
  `POST /api/wiki/git/discard-line`, and
  `POST /api/wiki/git/discard-change-block` wrap
  `wiki_store::vault_git_*` helpers for live status, diff preview, checkpoint
  commits, remote synchronization, remote setup, single-path discard, and
  tracked unstaged hunk, standalone-added-line, or replacement-block discard.
  Diff previews return a combined patch plus file-level sections, including
  staged tracked changes and unstaged untracked files. Sections include hunk
  and line metadata so the UI can inspect add/remove/context ranges without
  reparsing raw patches. Hunk discard is server-validated against the current
  diff and uses reverse Git patch apply after a dry-run check; line and
  change-block discard validate the current hunk, line text, and working-tree
  line number before removing an eligible added line or restoring the removed
  side of a replacement block. These endpoints do not accept arbitrary
  client-supplied patches.
  Remote sync requires a clean Vault; pull is fast-forward-only and push
  establishes upstream on first use. Status responses may include the preferred
  remote name and a redacted remote URL; Buddy never echoes plaintext URL
  credentials back to the UI. Discard only accepts paths already reported by Git
  status and rejects absolute or parent-traversing paths.
- Successful Buddy Vault Git mutations are appended to the local audit log
  `.clawwiki/vault-git-log.jsonl` and exposed through
  `GET /api/wiki/git/audit`. The audit file is ignored through seeded
  `.gitignore` and `.git/info/exclude`, so Git bookkeeping never dirties the
  user's checkpoint state.
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

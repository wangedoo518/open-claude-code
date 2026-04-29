---
title: Buddy Tolaria Deep Product Design Implementation Plan
doc_type: plan
status: implemented
owner: desktop-shell
last_verified: 2026-04-30
source_of_truth: false
related:
  - docs/desktop-shell/specs/2026-04-29-buddy-tolaria-deep-product-design.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/operations/README.md
---

# Buddy Tolaria Deep Product Design Implementation Plan

This plan records the main-only implementation slices for the
Tolaria-inspired Buddy design.

## Implemented Slice 1

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

## Implemented Slice 2

- Added live Buddy Vault Git endpoints for status, diff, and checkpoint commit:
  `GET /api/wiki/git/status`, `GET /api/wiki/git/diff`, and
  `POST /api/wiki/git/commit`.
- Replaced placeholder Git badges in Home/Pulse, BuddyStatusBar, and
  Connections with live dirty/ahead/behind/remote state.
- Connections now exposes a Vault checkpoint panel with changed files, tracked
  diff preview, and a commit-message input.
- Added external AI controlled-write policy persistence under
  `.clawwiki/external-ai-write-policy.json`, with add/revoke endpoints for
  session grants and permanent rules.
- Home/Pulse, BuddyStatusBar, and Connections now read the external AI policy
  instead of showing static authorization copy.
- Added CodeMirror 6 as the concrete editor for Wiki Markdown/frontmatter and
  Rules Studio Advanced editing.
- Added browser smoke coverage for Home/Pulse, Rules Studio folded Advanced
  state, Connections, and Knowledge.

## Implemented Slice 3

- Expanded `GET /api/wiki/git/diff` so unstaged previews include untracked
  files as diff-like sections and tracked/staged changes are split into
  file-level sections.
- Connections now lets users switch between unstaged and staged Vault diff
  previews, with a compact section list for changed paths.
- The Buddy Tolaria browser smoke now seeds a real wiki page fixture and
  verifies the CodeMirror edit/save path through `/wiki/{slug}`.

## Implemented Slice 4

- Added remote Buddy Vault sync endpoints:
  `POST /api/wiki/git/pull` and `POST /api/wiki/git/push`.
- Pull uses fast-forward-only Git sync and Push sets the upstream on first
  push when a remote exists but no upstream is configured.
- Remote sync is blocked while the Vault is dirty, forcing the user to create
  a checkpoint before pull/push.
- Connections exposes Pull/Push controls beside refresh and surfaces remote
  sync success or failure in the Vault panel.
- Rust coverage now exercises push-to-bare-remote, fast-forward pull, and
  dirty-worktree sync rejection.

## Implemented Slice 5

- Added `POST /api/wiki/git/remote` to add or replace the Buddy Vault `origin`
  remote through Git config.
- `GET /api/wiki/git/status` now returns the preferred remote name and a
  redacted remote URL, avoiding plaintext credential echo in the UI.
- Connections now includes an origin URL field, saves the remote without
  requiring a clean worktree, and keeps Pull/Push gated behind a clean
  checkpoint state.
- Rust coverage now verifies remote add/update, URL redaction, and invalid
  remote rejection.
- Browser smoke now asserts the Connections remote setup controls render.

## Implemented Slice 6

- Added `POST /api/wiki/git/discard` for discarding one dirty Vault path.
- Discard is constrained to paths already reported by Git status and rejects
  absolute paths, parent traversal, and clean/unknown paths.
- Tracked paths are restored from `HEAD`; untracked files or files inside an
  untracked directory are removed from the Vault.
- Connections now lets users select a file-level diff section and discard that
  selected file after a confirmation prompt.
- Rust coverage now verifies tracked restore, untracked removal, and unsafe
  path rejection.

## Implemented Slice 7

- Expanded `GET /api/wiki/git/diff` file sections with hunk and line metadata:
  each hunk carries old/new ranges and each line reports add/remove/context
  kind plus old/new line numbers where Git provides them.
- Connections now lets users narrow a file diff preview to an individual hunk
  and shows per-selection added/removed line counts before checkpoint or
  discard actions.
- This slice is intentionally review-only: file discard remains the only
  mutating discard operation, and line-level patch mutation stays behind a
  separate interaction model.
- Rust coverage now verifies tracked edits and untracked previews expose
  line-level metadata.

## Implemented Slice 8

- Added `POST /api/wiki/git/discard-hunk` for discarding one tracked,
  unstaged Buddy Vault diff hunk.
- Hunk discard validates the path, requires the path to be dirty and tracked,
  finds the requested hunk in the current server-side diff, checks the optional
  hunk header for staleness, then runs `git apply --reverse --check` before
  applying the reverse patch.
- Connections now exposes a separate `丢弃 Hunk` action only when an individual
  unstaged tracked hunk is selected; file-level discard remains available as
  the broader rollback operation.
- Rust coverage now verifies selected-hunk discard preserves other hunks in the
  same file and rejects untracked or stale hunk requests.

## Implemented Slice 9

- Expanded the Buddy Tolaria browser smoke with a real Buddy Vault hunk-discard
  API exercise before UI navigation.
- The smoke now creates a tracked fixture, checkpoints it, creates two
  separated hunks, discards only the first hunk through
  `POST /api/wiki/git/discard-hunk`, and verifies the unrelated hunk remains
  on disk.
- This keeps the Git quality loop closer to Tolaria discipline: mutation
  endpoints must be covered both by Rust unit tests and by the real
  desktop-server/Vite smoke path.

## Implemented Slice 10

- Added a local Buddy Vault Git audit log at
  `.clawwiki/vault-git-log.jsonl` for successful commit, pull, push, remote,
  file discard, and hunk discard operations.
- The audit log is written only after successful mutations and is ignored via
  both seeded `.gitignore` and `.git/info/exclude`, so audit bookkeeping does
  not dirty checkpoint state.
- Added `GET /api/wiki/git/audit` and surfaced recent Git operations in
  Connections.
- Browser smoke now expects the recent Git operation surface after exercising
  the hunk-discard API, and Rust coverage verifies audit entries do not dirty
  the Vault.

## Implemented Slice 11

- Connections now renders hunk/line metadata as a structured diff table with
  old/new line numbers, hunk headers, and add/remove/context styling backed by
  the existing diff design tokens.
- Raw patch text remains the fallback for loading, empty, or metadata-free
  sections, so the UI degrades safely if a future diff source cannot be parsed
  into hunks.

## Implemented Slice 12

- Home/Pulse now reads `GET /api/wiki/git/audit?limit=1` and surfaces the
  latest local Buddy Vault Git operation in the first-viewport health check.
- This extends the Tolaria-style "what happened today" posture beyond the
  Connections operator surface: Git remains first-class for operators, while
  casual users can still see whether the Vault has recently been checkpointed
  or rolled back without opening a dedicated Git page.
- Browser smoke coverage now requires the Home/Pulse route to render the
  `最近 Git 操作` section after the smoke creates a real commit and hunk
  discard through the API.

## Implemented Slice 13

- Browser smoke now directly calls `GET /api/wiki/git/audit?limit=5` after the
  real hunk-discard API exercise.
- The smoke asserts the latest audit entry is `discard-hunk`, carries the
  expected path and hunk index, and that the baseline `commit` entry is present.
  This keeps the Git audit log covered as a backend contract, not only as text
  rendered by Connections or Home/Pulse.

## Implemented Slice 14

- Wiki direct edit mode now reads live Buddy Vault Git status and shows a
  compact `Vault diff` / `Checkpoint` block next to schema validation and
  lineage copy.
- A successful human edit invalidates all `wiki/git` queries, so Home/Pulse,
  StatusBar, and Connections see the new checkpoint pressure after the save.
- Browser smoke now verifies the Wiki edit side panel renders the Git/Lineage
  status before saving the edited page.

## Implemented Slice 15

- Inbox review now reads live Buddy Vault Git status and adds a fourth metrics
  card for Vault checkpoint pressure next to create, merge, and processed-today
  counts.
- Inbox quick accept, reject, batch resolve, and combined apply flows now
  invalidate `wiki/git` queries after mutation so Home/Pulse, StatusBar,
  Connections, and Inbox converge on the same Git state.
- Browser smoke now visits `/inbox` and asserts the Vault checkpoint metric is
  visible.

## Implemented Slice 16

- Rules Studio now reads live Buddy Vault Git status and shows a compact
  `Git checkpoint` block before the Advanced YAML / CodeMirror editor.
- Saving `schema/CLAUDE.md` now invalidates `wiki/git` queries so Rules,
  Home/Pulse, StatusBar, Connections, and Inbox converge on the same
  checkpoint pressure.
- Browser smoke now asserts Rules Studio renders the Git checkpoint surface
  while keeping Advanced YAML / CodeMirror folded by default.

## Implemented Slice 17

- Rules Studio now renders the `schema/templates/*.md` catalog by default,
  including each template's display name, category path, required field count,
  and first body hint.
- The frontend `SchemaTemplate` type now matches the Rust
  `SchemaTemplateInfo` API shape instead of the older name/content placeholder.
- Personal and research templates now get localized display names in
  `wiki_store`, and Rust coverage verifies the six built-in template
  categories.
- Browser smoke now asserts Rules Studio exposes concrete concept and research
  template paths before the Advanced YAML / CodeMirror panel is opened.

## Implemented Slice 18

- Added `GET /api/wiki/guidance` for Rules Studio to inspect root and schema
  guidance files without opening the Advanced editor.
- The endpoint reports `AGENTS.md`, `CLAUDE.md`, `schema/AGENTS.md`, and
  `schema/CLAUDE.md` existence, byte size, absolute path, relative path, and
  first heading.
- Rules Studio now renders a default Guidance catalog so users can see the
  root shims that external AI and CLI agents read before writing.
- Rust coverage verifies the four seeded guidance files exist after
  `init_wiki`, and browser smoke asserts the Guidance catalog appears on
  `/rules`.

## Implemented Slice 19

- Added `GET /api/wiki/policies` for Rules Studio to inspect
  `schema/policies/*.md` without opening Advanced YAML.
- Rules Studio now renders the policy catalog by default, including each
  policy heading, relative path, and byte size.
- Rust coverage verifies the seeded maintenance, conflict, deprecation, and
  naming policies, and browser smoke asserts concrete policy paths render on
  `/rules`.

## Implemented Slice 20

- Added `GET/PUT /api/wiki/rules/file` as the human Rules Studio edit path for
  allowlisted rule files.
- The backend only accepts root `AGENTS.md` / `CLAUDE.md`, schema guidance,
  `schema/purpose-lenses.yml`, existing `schema/templates/*.md`, and existing
  `schema/policies/*.md`; absolute paths, parent traversal, unknown files, and
  empty content are rejected.
- Rules Studio now includes a `Rule file editor` section with a file selector,
  CodeMirror editing, save/cancel controls, and Git query invalidation after a
  successful save.
- Browser smoke now edits `schema/policies/naming.md` through the real API and
  asserts the Rules Studio editor surface renders.

## Implemented Slice 21

- Rules Studio now reads the existing patrol report and shows a default
  `Validation snapshot` beside templates, guidance, policies, rule editing,
  and Git checkpoint state.
- The snapshot exposes schema violations, orphan pages, stale pages, stubs,
  oversized pages, confidence decay, issue count, and checked-at time without
  requiring users to open the legacy Dashboard patrol view.
- Rules Studio can trigger patrol directly through the existing
  `POST /api/wiki/patrol` endpoint, then invalidates patrol and Inbox queries
  so health signals converge across Home/Pulse, StatusBar, Inbox, and Rules.
- Browser smoke now asserts the Rules Studio validation panel and patrol
  trigger render by default.

## Implemented Slice 22

- Added `POST /api/wiki/git/discard-line` for discarding one tracked,
  unstaged, standalone added line from a Buddy Vault diff.
- The backend validates path safety, tracked/unstaged state, current hunk
  header, line index, line text, and working-tree line number before writing,
  and rejects replacement edits so line-level rollback cannot accidentally
  turn a modified line into a deletion.
- Connections now lets users select an eligible added line in the structured
  diff table and run `丢弃新增行` beside the existing hunk and file rollback
  controls.
- Git audit entries now include optional `line_index` metadata, and browser
  smoke exercises the real discard-line API while checking that unrelated
  added lines remain on disk.

## Implemented Slice 23

- Added `POST /api/wiki/git/discard-change-block` for restoring one tracked,
  unstaged replacement block from a selected added line.
- The backend validates the current hunk, selected line text, line number, and
  contiguous add/remove change block, then swaps the working-tree added lines
  back to the removed lines from the current diff.
- Connections now exposes `丢弃替换块` separately from `丢弃新增行`, keeping
  pure additions and replacement edits visually and behaviorally distinct.
- Browser smoke now exercises the real change-block API and verifies an
  unrelated replacement hunk remains dirty after the selected block is restored.

## Implemented Slice 24

- Added a testable Command Manifest for the command palette under
  `features/palette/command-manifest.ts`.
- The manifest records stable command ids, route coverage, menu/palette
  visibility, shortcut metadata, and route drift diagnostics derived from
  `CLAWWIKI_ROUTES`.
- Route palette rows now carry `commandId`, so future menu/native shortcut
  wiring can reuse the same registry instead of inventing another command map.
- Added ambient Vitest-style contract tests that type-check today and will run
  directly once the desktop-shell test harness is wired.

## Implemented Slice 25

- BuddyStatusBar items now behave as Tolaria-style one-click workbench
  entrypoints instead of passive labels.
- Health opens Home/Pulse, Inbox opens `/inbox`, Git opens the Connections Git
  section, permission opens Settings permissions, external-AI/session badges
  open the Connections authorization section, and the page/raw count opens
  Knowledge.
- Connections now exposes an `external-ai` anchor for status-bar deep links.
- Browser smoke verifies the StatusBar renders actionable links for Inbox,
  Git, and external AI.

## Implemented Slice 26

- Ask now exposes a compact Purpose Lens selector in the Composer, defaulting
  to automatic cross-purpose behavior and allowing the user to constrain a turn
  to writing, building, operating, learning, personal, or research.
- `POST /api/desktop/sessions/{id}/messages` accepts optional `purpose`
  values, normalizes and deduplicates them server-side, writes them into
  `ContextBasis.purpose_lenses`, and injects a short Purpose Lens instruction
  into OpenAI-compatible, agentic, and fallback prompt paths without polluting
  the persisted user message.
- Assistant context labels now surface selected purpose values, including
  follow-up turns that would otherwise hide the context-basis chip.
- Browser smoke now verifies the Ask Purpose Lens UI, and the API smoke checks
  normalization plus non-leakage of hidden purpose instructions into session
  history.

## Implemented Slice 27

- Home/Pulse now renders a weekly Purpose Lens digest directly below the
  external-brain health cards, showing what each purpose absorbed this week,
  how many pages are ready to express, and the latest absorbed page titles.
- The digest uses the same `PURPOSE_LENSES` vocabulary as frontmatter,
  Knowledge filters, and Ask Purpose mode, keeping `writing`, `building`,
  `operating`, `learning`, `personal`, and `research` aligned across the app.
- Pages without a valid `purpose` are surfaced as a Knowledge follow-up item
  from the Home screen instead of being hidden inside patrol details.
- Browser smoke now seeds a current-week learning page and verifies the Home
  Purpose Lens digest renders that absorbed wiki page.

## Implemented Slice 28

- Wiki page summaries now parse and expose optional `expressed_in` frontmatter
  refs, allowing Buddy to represent Tolaria's capture -> organize -> express
  loop as local Markdown/YAML instead of an external database concept.
- Home/Pulse now renders a `最近表达` pulse from `expressed_in`, while Purpose
  Lens cards split `可表达` pages from `已表达` pages so the health check shows
  whether knowledge is being used.
- Built-in Rules Studio templates now include `expressed_in: []`, and Wiki
  article metadata shows an `已表达 N` badge when a page has output refs.
- Browser smoke seeds an expressed wiki page and verifies the Home/Pulse
  expression signal renders end to end.

## Implemented Slice 29

- Ask turns with an explicit wiki source binding now mark that page as
  expressed by appending `ask:<session-id>` to its `expressed_in`
  frontmatter, deduplicated per session.
- The wiki-store write helper preserves full Markdown/YAML content, supports
  existing block or inline-empty `expressed_in` frontmatter, and rejects unsafe
  expression refs.
- Ask send success invalidates wiki page and Git queries when a wiki binding
  participated, so Home/Pulse, Wiki, and Vault checkpoint pressure converge
  after expression writes.
- Browser smoke now exercises the real bind -> Ask append -> wiki page read
  flow and verifies the automatic `expressed_in` mark.

## Implemented Slice 30

- Inbox Maintain decisions now expose a Purpose Lens picker so reviewers can
  confirm whether an entry serves writing, building, operating, learning,
  personal, or research before applying the action.
- `POST /api/wiki/inbox/{id}/maintain` accepts optional `purpose_lenses` and
  passes them through the maintainer path for both create and update decisions.
- Maintainer-created wiki pages now write the reviewed purpose values into
  frontmatter; update-existing flows merge reviewed purpose values into the
  target page while preserving existing `expressed_in` refs.
- Rust coverage verifies purpose normalization/deduping and maintainer
  create/update writes, while browser smoke verifies the Inbox Purpose Lens
  review surface renders from a real seeded inbox entry.

## Implemented Slice 31

- Ask wiki query crystallization now completes the Tolaria
  `capture -> organize -> express` loop: substantive answers still write
  `raw/query`, and now also append a pending NewRaw Inbox task for human
  review.
- `query_wiki` returns the created `raw_id`, `inbox_id`, and display title in
  its final result; the desktop SSE `query_done` payload forwards that
  `crystallized` object to the UI.
- Ask renders a compact crystallization receipt after a completed wiki query,
  with direct entrypoints to the Raw Library record and the Inbox review task.
- Rust coverage verifies long answers create both raw and Inbox records, short
  answers do not crystallize, and the SSE payload preserves crystallization
  ids.

## Implemented Slice 32

- Wiki direct edit now treats frontmatter as fully editable but guarded:
  `type`, `status`, `schema`, `source_raw_id`, `purpose`, `expressed_in`, and
  `source_refs` are validated before save.
- The frontend save panel now shows schema-aware errors for unsupported page
  types/statuses, invalid numeric raw ids, invalid schema versions, and unsafe
  reference values before the user can submit.
- The backend `PUT /api/wiki/pages/{slug}` path enforces the same critical
  field checks while preserving custom frontmatter fields, so UI bypasses still
  cannot write broken key metadata.
- Rust coverage verifies valid custom frontmatter with `source_refs` survives
  and invalid critical fields are rejected.

## Implemented Slice 33

- Connections now makes the staged diff tab explicitly read-only: hunk, line,
  replacement-block, and file discard controls remain scoped to unstaged
  tracked changes.
- The staged diff preview shows a compact notice explaining that staged diffs
  are for review and that rollback controls only affect unstaged changes.
- Backend coverage now pins the boundary by verifying staged hunks are visible
  through `vault_git_diff(staged=true)` but rejected by
  `vault_git_discard_hunk`.

## Implemented Slice 34

- Wiki `source_refs` are now first-class summary metadata rather than only a
  save-time validation field: list, backlink, search, graph-adjacent reads, and
  page detail parsing all preserve the frontmatter lineage refs.
- Wiki Article displays `source_refs` as compact source chips near the page
  metadata, giving users a low-friction way to see where a knowledge page came
  from without opening YAML.
- Direct-edit fallback Markdown now includes `source_refs` alongside
  `expressed_in`, so pages created or repaired through the editor keep both
  Tolaria-style lineage directions visible.
- Rust coverage verifies `source_refs` parsing and confirms full-frontmatter
  edits expose the preserved source refs in `WikiPageSummary`.

## Implemented Slice 35

- Wiki search now treats `source_refs` as a weighted searchable field, so users
  can find knowledge pages by raw/source lineage ids from the Knowledge search
  box.
- Knowledge page rows now preview `source_refs` or legacy `source_raw_id`
  inline, making the source trail visible from the main knowledge list without
  opening a page or YAML editor.
- Search results label source-ref matches as `命中：来源`, preserving the same
  low-friction lineage signal in both normal browsing and query mode.
- Browser smoke now seeds a page with `source_refs`, verifies the Knowledge
  list renders that lineage, and searches by the source ref through the real
  API/UI path.

## Implemented Slice 36

- Home/Pulse now includes `来源可追溯` in the first health-stat band, counting
  wiki pages that carry either legacy `source_raw_id` or modern `source_refs`
  lineage.
- This keeps Tolaria's source-trail discipline visible on the default homepage:
  users can see whether organized knowledge is traceable without opening
  Knowledge or editing YAML.
- Browser smoke now requires the Home/Pulse route to render the lineage health
  label after seeding a page with `source_refs`.

## Implemented Slice 37

- Knowledge now has a dedicated Source filter with `全部来源`, `有来源`, and
  `缺来源`, backed by shareable `?source=sourced|missing` query params.
- The Home/Pulse `来源可追溯` health stat now links directly to
  `/wiki?source=sourced`, turning the homepage signal into a concrete Knowledge
  workbench view.
- The filter recognizes both modern `source_refs` and legacy `source_raw_id`,
  so older maintained pages and newer Tolaria-style lineage pages behave
  consistently.
- Browser smoke now exercises the real Source filter select and verifies the
  hash route carries `source=sourced` before searching by a seeded source ref.

## Implemented Slice 38

- Home/Pulse now treats missing source lineage as an actionable health risk:
  pages without `source_refs` or `source_raw_id` contribute to the headline risk
  count.
- The Top 3 action list now includes `补齐 N 页来源线索` when missing-source
  pages exist, linking directly to `/wiki?source=missing`.
- This complements the positive `来源可追溯` metric: Home now shows both
  traceable knowledge and the cleanup queue needed to improve lineage quality.
- Browser smoke now requires the Home/Pulse route to render the source-lineage
  follow-up text in the real seeded Vault.

## Implemented Slice 39

- Wiki relation scoring now normalizes legacy `source_raw_id` and modern
  `source_refs` into the same lineage refs, so pages sharing `raw:00042` relate
  even when one uses old frontmatter and the other uses the Tolaria-style list.
- `build_wiki_graph` now emits `derived-from` edges for `raw:<id>` values in
  `source_refs`, not only for `source_raw_id`.
- Related-page reasons keep the existing user-facing copy (`共享来源: raw
  #00042`) after normalization, so Wiki Article relations do not expose schema
  implementation details.
- Rust coverage verifies source-ref graph edges and source-ref related scoring;
  browser smoke checks the real page graph endpoint returns a related peer via
  shared `source_refs`.

## Verification

- `cd apps/desktop-shell && npm run build`
- `cd rust && cargo test -p wiki_store source_refs`
- `cd apps/desktop-shell/src-tauri && cargo check`
- `cd rust && cargo check --workspace`
- `cd rust && cargo test -p wiki_store`
- `cd rust && cargo test -p wiki_maintainer`
- `cd rust && cargo test -p desktop-server query_done_payload`
- `cd apps/desktop-shell && BUDDY_API_BASE=http://127.0.0.1:4358 BUDDY_SMOKE_URL=http://127.0.0.1:5174/ npm run test:buddy:smoke`

## Future Hardening

- Keep partial-line and staged-hunk mutation out of scope until rollback tests
  cover those cases.

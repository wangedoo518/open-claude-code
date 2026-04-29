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

## Verification

- `cd apps/desktop-shell && npm run build`
- `cd apps/desktop-shell/src-tauri && cargo check`
- `cd rust && cargo check --workspace`
- `cd rust && cargo test -p wiki_store`
- `cd apps/desktop-shell && BUDDY_API_BASE=http://127.0.0.1:4358 BUDDY_SMOKE_URL=http://127.0.0.1:5174/ npm run test:buddy:smoke`

## Future Hardening

- Add line-level patch apply/discard after hunk-level discard has enough
  reviewer and dogfood feedback. Keep partial-line and staged-hunk mutation out
  of scope until conflict behavior and rollback tests are explicit.

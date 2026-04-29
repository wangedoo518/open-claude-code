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

## Verification

- `cd apps/desktop-shell && npm run build`
- `cd apps/desktop-shell/src-tauri && cargo check`
- `cd rust && cargo check --workspace`
- `cd rust && cargo test -p wiki_store`
- `cd apps/desktop-shell && BUDDY_API_BASE=http://127.0.0.1:4358 BUDDY_SMOKE_URL=http://127.0.0.1:5174/ npm run test:buddy:smoke`

## Future Hardening

- Add line-level diff selection and apply/discard actions after the Vault
  checkpoint model has reviewer-approved interaction rules.

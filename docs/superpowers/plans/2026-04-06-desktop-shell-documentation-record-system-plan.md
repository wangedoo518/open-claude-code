---
title: Desktop Shell Documentation Record System Plan
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-04-06
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/plans/README.md
  - docs/superpowers/specs/2026-04-06-desktop-shell-documentation-record-system-design.md
---

# Desktop Shell Documentation Record System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a durable `desktop-shell` documentation record system with a product hub, categorized docs, migrated token references, and root-level `AGENTS.md` guidance that points contributors to canonical product knowledge.

**Architecture:** The implementation creates a new `docs/desktop-shell/` knowledge surface as the product entrypoint, migrates durable `desktop-shell` docs into typed subdirectories with YAML frontmatter, and updates `AGENTS.md` to act as a map instead of a knowledge dump. Legacy docs remain in place only when needed for transition and are marked so they do not compete with the new source-of-truth documents.

**Tech Stack:** Markdown, YAML frontmatter, repository docs conventions, git

---

## File Structure

**Create**

- `docs/desktop-shell/README.md`
- `docs/desktop-shell/architecture/overview.md`
- `docs/desktop-shell/decisions/README.md`
- `docs/desktop-shell/specs/README.md`
- `docs/desktop-shell/plans/README.md`
- `docs/desktop-shell/tokens/design-tokens.md`
- `docs/desktop-shell/tokens/functional-tokens.md`
- `docs/desktop-shell/operations/README.md`

**Modify**

- `AGENTS.md`
- `apps/desktop-shell/DESIGN_TOKENS.md`
- `apps/desktop-shell/FUNCTIONAL_TOKENS.md`
- `docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md`
- `docs/superpowers/plans/2026-04-06-desktop-shell-architecture-refactor-plan.md`

**Verification**

- `rg -n "source_of_truth|last_verified|doc_type|supersedes" docs/desktop-shell AGENTS.md apps/desktop-shell`
- `git diff --check`

### Task 1: Create the product documentation hub

**Files:**
- Create: `docs/desktop-shell/README.md`

- [ ] **Step 1: Write the hub document**

```md
---
title: Desktop Shell Documentation Hub
doc_type: architecture
status: active
owner: desktop-shell
last_verified: 2026-04-06
source_of_truth: true
related:
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/tokens/design-tokens.md
  - docs/desktop-shell/tokens/functional-tokens.md
  - docs/desktop-shell/operations/README.md
---

# Desktop Shell Documentation Hub

This page is the entrypoint for `apps/desktop-shell` documentation.

## Current Truth

- [Architecture Overview](./architecture/overview.md)
- [Design Tokens](./tokens/design-tokens.md)
- [Functional Tokens](./tokens/functional-tokens.md)
- [Operations](./operations/README.md)

## Decisions

- [Decision Index](./decisions/README.md)

## Change Process

- [Spec Index](./specs/README.md)
- [Plan Index](./plans/README.md)

## Rules

- Update `architecture/` when product structure changes.
- Add to `decisions/` when a durable technical choice changes.
- Add to `specs/` and `plans/` before implementation for substantial work.
- Do not treat old specs and plans as current product truth.
```

- [ ] **Step 2: Apply the document**

Run:

```bash
mkdir -p docs/desktop-shell/{architecture,decisions,specs,plans,tokens,operations}
```

Then create the file with `apply_patch`.

Expected: `docs/desktop-shell/README.md` exists with frontmatter and active links.

- [ ] **Step 3: Verify the hub renders as the product entrypoint**

Run:

```bash
sed -n '1,120p' docs/desktop-shell/README.md
```

Expected: frontmatter appears first and the "Current Truth" section lists canonical docs.

- [ ] **Step 4: Commit**

```bash
git add docs/desktop-shell/README.md
git commit -m "docs(desktop-shell): add documentation hub"
```

### Task 2: Add architecture and category index documents

**Files:**
- Create: `docs/desktop-shell/architecture/overview.md`
- Create: `docs/desktop-shell/decisions/README.md`
- Create: `docs/desktop-shell/specs/README.md`
- Create: `docs/desktop-shell/plans/README.md`
- Create: `docs/desktop-shell/operations/README.md`

- [ ] **Step 1: Write the architecture overview**

```md
---
title: Desktop Shell Architecture Overview
doc_type: architecture
status: active
owner: desktop-shell
last_verified: 2026-04-06
source_of_truth: true
related:
  - docs/desktop-shell/README.md
  - docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md
---

# Desktop Shell Architecture Overview

This document answers: how `desktop-shell` is currently organized.

## Application Layers

- App shell and routing
- Feature modules
- Shared UI and utility layer
- Desktop integration layer

## State Ownership

- Router owns navigational identity.
- TanStack Query owns remote state.
- Redux owns narrow local UI state.

## Change Policy

If these boundaries change, update this document in the same change set.
```

- [ ] **Step 2: Write the category indexes**

```md
---
title: Desktop Shell Decisions Index
doc_type: decision
status: active
owner: desktop-shell
last_verified: 2026-04-06
related:
  - docs/desktop-shell/README.md
---

# Desktop Shell Decisions Index

This page lists durable technical decisions for `desktop-shell`.

- Add one ADR-style document per durable choice.
- Link only active decisions from this index.
```

```md
---
title: Desktop Shell Spec Index
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-06
related:
  - docs/desktop-shell/README.md
  - docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md
---

# Desktop Shell Spec Index

Active and historical specs for `desktop-shell` should be referenced from here.
```

```md
---
title: Desktop Shell Plan Index
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-04-06
related:
  - docs/desktop-shell/README.md
  - docs/superpowers/plans/2026-04-06-desktop-shell-architecture-refactor-plan.md
---

# Desktop Shell Plan Index

Approved implementation plans for `desktop-shell` should be referenced from here.
```

```md
---
title: Desktop Shell Operations
doc_type: operation
status: active
owner: desktop-shell
last_verified: 2026-04-06
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

## Verification Commands

- `cd apps/desktop-shell && npm run build`
- `cd apps/desktop-shell/src-tauri && cargo check`
- `git diff --check`
```

- [ ] **Step 3: Apply the new files**

Run:

```bash
sed -n '1,120p' docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md
```

Then create all five files with `apply_patch`, preserving exact frontmatter fields shown above.

Expected: `docs/desktop-shell/architecture/overview.md` and the four index files exist.

- [ ] **Step 4: Verify category docs**

Run:

```bash
rg -n "^title:|^doc_type:|^status:|^last_verified:" docs/desktop-shell
```

Expected: every new file reports the metadata fields without omissions.

- [ ] **Step 5: Commit**

```bash
git add docs/desktop-shell/architecture/overview.md docs/desktop-shell/decisions/README.md docs/desktop-shell/specs/README.md docs/desktop-shell/plans/README.md docs/desktop-shell/operations/README.md
git commit -m "docs(desktop-shell): add architecture and index documents"
```

### Task 3: Migrate token references into canonical token docs

**Files:**
- Create: `docs/desktop-shell/tokens/design-tokens.md`
- Create: `docs/desktop-shell/tokens/functional-tokens.md`
- Modify: `apps/desktop-shell/DESIGN_TOKENS.md`
- Modify: `apps/desktop-shell/FUNCTIONAL_TOKENS.md`

- [ ] **Step 1: Write the canonical token docs**

```md
---
title: Desktop Shell Design Tokens
doc_type: token
status: active
owner: desktop-shell
last_verified: 2026-04-06
source_of_truth: true
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/operations/README.md
supersedes:
  - apps/desktop-shell/DESIGN_TOKENS.md
---
```

```md
---
title: Desktop Shell Functional Tokens
doc_type: token
status: active
owner: desktop-shell
last_verified: 2026-04-06
source_of_truth: true
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/operations/README.md
supersedes:
  - apps/desktop-shell/FUNCTIONAL_TOKENS.md
---
```

After each frontmatter block, copy the existing body from the legacy token document unchanged.

- [ ] **Step 2: Replace legacy token docs with forwarding notes**

```md
# Moved

Canonical document moved to:

- `docs/desktop-shell/tokens/design-tokens.md`
```

```md
# Moved

Canonical document moved to:

- `docs/desktop-shell/tokens/functional-tokens.md`
```

- [ ] **Step 3: Apply the migration**

Run:

```bash
sed -n '1,40p' apps/desktop-shell/DESIGN_TOKENS.md
sed -n '1,40p' apps/desktop-shell/FUNCTIONAL_TOKENS.md
```

Then create the canonical token docs and replace the legacy files with the forwarding notes above.

Expected: token content now lives under `docs/desktop-shell/tokens/`, while the old locations remain as explicit redirects.

- [ ] **Step 4: Verify no ambiguity remains**

Run:

```bash
rg -n "Canonical document moved to|source_of_truth: true" docs/desktop-shell apps/desktop-shell
```

Expected: the new token docs are marked as canonical and the old files are visibly redirects.

- [ ] **Step 5: Commit**

```bash
git add docs/desktop-shell/tokens/design-tokens.md docs/desktop-shell/tokens/functional-tokens.md apps/desktop-shell/DESIGN_TOKENS.md apps/desktop-shell/FUNCTIONAL_TOKENS.md
git commit -m "docs(desktop-shell): migrate token references"
```

### Task 4: Connect legacy spec and plan docs to the new system

**Files:**
- Modify: `docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md`
- Modify: `docs/superpowers/plans/2026-04-06-desktop-shell-architecture-refactor-plan.md`
- Modify: `docs/desktop-shell/specs/README.md`
- Modify: `docs/desktop-shell/plans/README.md`

- [ ] **Step 1: Add frontmatter and related links to the legacy spec**

```md
---
title: Desktop Shell Architecture Refactor Design
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-06
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/plans/README.md
---
```

Add this block above the existing title without changing the body text.

- [ ] **Step 2: Add frontmatter and related links to the legacy plan**

```md
---
title: Desktop Shell Architecture Refactor Plan
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-04-06
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/specs/README.md
---
```

Add this block above the existing title without changing the body text.

- [ ] **Step 3: Populate the spec and plan indexes**

```md
# Desktop Shell Spec Index

- [Architecture Refactor Design](../../superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md)
- [Documentation Record System Design](../../superpowers/specs/2026-04-06-desktop-shell-documentation-record-system-design.md)
```

```md
# Desktop Shell Plan Index

- [Architecture Refactor Plan](../../superpowers/plans/2026-04-06-desktop-shell-architecture-refactor-plan.md)
- [Documentation Record System Plan](../../superpowers/plans/2026-04-06-desktop-shell-documentation-record-system-plan.md)
```

Append these links below the intro paragraphs while keeping the existing frontmatter intact.

- [ ] **Step 4: Verify the cross-links**

Run:

```bash
rg -n "desktop-shell/README|Architecture Refactor|Documentation Record System" docs/superpowers docs/desktop-shell
```

Expected: both legacy docs now declare metadata and both index pages link into the active spec/plan set.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md docs/superpowers/plans/2026-04-06-desktop-shell-architecture-refactor-plan.md docs/desktop-shell/specs/README.md docs/desktop-shell/plans/README.md
git commit -m "docs(desktop-shell): connect specs and plans to product hub"
```

### Task 5: Update root `AGENTS.md` to act as the map

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: Add a `desktop-shell` documentation section**

```md
## Desktop Shell Documentation Map

Use `docs/desktop-shell/README.md` as the entrypoint for `apps/desktop-shell` knowledge.

Document categories:

- `architecture/`: current product structure and boundaries
- `decisions/`: durable technical choices and rationale
- `specs/`: approved designs before implementation
- `plans/`: implementation sequencing
- `tokens/`: shared design and functional vocabulary
- `operations/`: maintenance and verification workflows

Update rules:

- If structure changes, update `architecture/`.
- If a durable technical choice changes, add or update `decisions/`.
- If shared vocabulary changes, update `tokens/`.
- If maintenance workflow changes, update `operations/`.
- Keep `AGENTS.md` short; do not duplicate product knowledge here.
```

Insert this below the managed tool-mapping block and above any repo-specific bug-handling instruction so the map is visible early.

- [ ] **Step 2: Verify the section placement**

Run:

```bash
sed -n '1,120p' AGENTS.md
```

Expected: the new `Desktop Shell Documentation Map` section appears near the top and does not break the managed block.

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md
git commit -m "docs: point agents to desktop-shell documentation hub"
```

### Task 6: Final consistency verification

**Files:**
- Verify: `docs/desktop-shell/**/*`
- Verify: `AGENTS.md`
- Verify: `apps/desktop-shell/DESIGN_TOKENS.md`
- Verify: `apps/desktop-shell/FUNCTIONAL_TOKENS.md`

- [ ] **Step 1: Run metadata verification**

Run:

```bash
rg -n "^(title|doc_type|status|owner|last_verified|source_of_truth|related|supersedes):" docs/desktop-shell docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md docs/superpowers/plans/2026-04-06-desktop-shell-architecture-refactor-plan.md
```

Expected: all canonical docs and linked legacy docs expose the required metadata.

- [ ] **Step 2: Run redirect and link verification**

Run:

```bash
rg -n "Canonical document moved to|Desktop Shell Documentation Map|Architecture Refactor Design|Documentation Record System Plan" AGENTS.md docs/desktop-shell apps/desktop-shell docs/superpowers
```

Expected: forwarding notes, root map, and hub references all resolve from repo text search.

- [ ] **Step 3: Run whitespace and patch safety checks**

Run:

```bash
git diff --check
```

Expected: no trailing whitespace, merge markers, or malformed patch output.

- [ ] **Step 4: Review the changed file list**

Run:

```bash
git status --short
```

Expected: only the planned documentation files appear modified.

- [ ] **Step 5: Commit**

```bash
git add AGENTS.md docs/desktop-shell docs/superpowers/specs/2026-04-06-desktop-shell-architecture-refactor-design.md docs/superpowers/plans/2026-04-06-desktop-shell-architecture-refactor-plan.md apps/desktop-shell/DESIGN_TOKENS.md apps/desktop-shell/FUNCTIONAL_TOKENS.md
git commit -m "docs(desktop-shell): establish documentation record system"
```

## Self-Review

### Spec coverage

- Information architecture is covered by Tasks 1 and 2.
- Frontmatter, truth, and freshness rules are covered by Tasks 1 through 4 and Task 6 verification.
- `AGENTS.md` role is covered by Task 5.
- Migration of existing token docs is covered by Task 3.
- Migration connection for existing spec and plan docs is covered by Task 4.

No spec sections are left without a task.

### Placeholder scan

- The plan contains exact file paths.
- Every content-writing step includes concrete markdown content.
- Every verification step contains explicit commands and expected outcomes.
- No `TBD`, `TODO`, or "implement later" placeholders remain.

### Type consistency

- `doc_type` values are consistent with the design: `architecture`, `decision`, `spec`, `plan`, `token`, `operation`.
- `status` values are consistently `active` for canonical or connected documents in this migration.
- Canonical token docs use `source_of_truth: true`, while legacy redirect files do not.

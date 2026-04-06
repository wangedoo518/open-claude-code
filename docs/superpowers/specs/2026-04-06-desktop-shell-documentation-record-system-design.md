---
title: Desktop Shell Documentation Record System Design
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-06
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/specs/README.md
  - docs/superpowers/plans/2026-04-06-desktop-shell-documentation-record-system-plan.md
---

# Desktop Shell Documentation Record System Design

**Goal**

Build a documentation record system for `apps/desktop-shell` that treats the repository as a durable knowledge surface instead of a pile of disconnected notes. The system should make it clear which documents describe current truth, which documents record design and delivery history, and how contributors should update documentation as the product evolves.

This design is informed by two inputs:

- the repository's existing `AGENTS.md` operating instructions
- OpenAI's "Harness engineering" article, which argues that `AGENTS.md` should be a lightweight map while durable knowledge lives in structured repository documents

Reference:

- <https://openai.com/zh-Hans-CN/index/harness-engineering/>

**Scope**

This design applies only to the `desktop-shell` product line and its related documentation.

In scope:

- establishing a canonical documentation structure for `desktop-shell`
- defining document categories and ownership boundaries
- defining frontmatter and freshness rules
- defining the role of `AGENTS.md` in this system
- migrating existing `desktop-shell` docs into the new structure

Out of scope for this iteration:

- creating a repository-wide documentation record system for all products
- building a runtime logging or telemetry system
- redesigning implementation architecture in `apps/desktop-shell`
- introducing an external docs site or knowledge base platform

## Current Problems

### 1. `desktop-shell` documentation is split across multiple locations

Relevant product documents currently live in at least three places:

- `apps/desktop-shell/*.md`
- `apps/desktop-shell/docs/*`
- `docs/superpowers/specs/*` and `docs/superpowers/plans/*`

This makes it hard to answer simple questions:

- Where should a contributor start?
- Which document is current truth versus historical context?
- Which docs are product docs versus workflow docs?

### 2. Document types are mixed together

Architecture snapshots, migration notes, token references, and implementation plans are all present, but they do not follow a single classification model. As a result, a reader cannot quickly distinguish between:

- current architecture truth
- a design proposal
- an execution plan
- shared design or functional language
- operational guidance

### 3. Existing docs do not expose freshness clearly

Some docs are valuable and current, but the repository has no standard way to indicate:

- whether a doc is still active
- when it was last checked against code
- whether it supersedes an older document
- whether it is a source of truth or just historical context

### 4. `AGENTS.md` currently acts as a behavior guide, but not yet as a documentation map

The repo already uses `AGENTS.md` for operating instructions, but `desktop-shell` still lacks a dedicated product-level documentation entrypoint. That means an agent or human reader must infer where product knowledge lives instead of being pointed to a stable map.

## Design Goals

The documentation record system should:

1. make `desktop-shell` product knowledge discoverable from one obvious entrypoint
2. separate current truth from in-flight design and historical records
3. make document freshness and replacement explicit
4. minimize duplication between `AGENTS.md` and the real docs
5. support both humans and coding agents with the same structure
6. be small enough to adopt immediately without blocking product work

## Information Architecture

The system uses two entry layers and six document categories.

### 1. Entry Layer 1: Root `AGENTS.md`

The root `AGENTS.md` should remain short and directive.

Its role in this system is to:

- point readers to the `desktop-shell` documentation hub
- explain what each document category means
- define the update rules at a high level
- provide a short documentation hygiene checklist

It should not become the place where product knowledge is stored.

### 2. Entry Layer 2: `desktop-shell` documentation hub

Create a product documentation hub at:

- `docs/desktop-shell/README.md`

This page becomes the default landing page for `desktop-shell` knowledge. It should:

- explain the documentation structure
- link only to active or canonical docs
- identify which pages are current truth
- separate stable references from change-process documents

### 3. Core document categories

Create six stable directories under `docs/desktop-shell/`:

- `architecture/`
- `decisions/`
- `specs/`
- `plans/`
- `tokens/`
- `operations/`

Their responsibilities are:

#### `architecture/`

Purpose:

- describe how `desktop-shell` is currently structured

Examples:

- architecture overview
- module boundaries
- state ownership model
- desktop integration boundary

This is current-state documentation, not proposal documentation.

#### `decisions/`

Purpose:

- preserve architecture and product decisions with rationale

Examples:

- why Redux scope was reduced instead of replaced
- why feature APIs live near feature modules
- why `AGENTS.md` stays thin

This category stores "why", not implementation steps.

#### `specs/`

Purpose:

- describe intended changes before implementation

Examples:

- refactor design
- feature design
- documentation system design

This category is process-stage documentation and should not masquerade as current architecture truth after the work lands.

#### `plans/`

Purpose:

- define approved implementation sequencing

Examples:

- migration phases
- verification checklist per phase
- rollout order

This category records execution intent, not enduring truth.

#### `tokens/`

Purpose:

- define the shared language used by product, design, and implementation

Examples:

- design tokens
- functional tokens
- terminology references

This category should hold the stable shared vocabulary currently split across `apps/desktop-shell/FUNCTIONAL_TOKENS.md` and `apps/desktop-shell/DESIGN_TOKENS.md`.

#### `operations/`

Purpose:

- document repeatable maintenance and delivery workflows

Examples:

- local development workflow
- verification commands
- release checks
- documentation update checklist

This category is the "how we maintain this product line" layer.

## Document Contract

Every formal `desktop-shell` document in this system should use YAML frontmatter.

Minimum contract:

```md
---
title: Desktop Shell Architecture Overview
doc_type: architecture
status: active
owner: desktop-shell
last_verified: 2026-04-06
source_of_truth: true
related:
  - docs/desktop-shell/decisions/...
  - docs/desktop-shell/operations/...
supersedes:
  - docs/superpowers/specs/...
---
```

### Required fields

- `title`
- `doc_type`
- `status`
- `owner`
- `last_verified`
- `related`

### Recommended fields

- `source_of_truth`
- `supersedes`

### Allowed `doc_type` values

- `architecture`
- `decision`
- `spec`
- `plan`
- `token`
- `operation`

### Allowed `status` values

- `draft`
- `active`
- `superseded`
- `archived`

## Truth and Freshness Rules

The system must make it obvious which doc wins when multiple documents exist.

### Rule 1: `source_of_truth: true` is reserved

Only a small number of stable reference docs should use `source_of_truth: true`.

Expected examples:

- architecture overview
- token overview
- operations overview

Specs and plans should usually not be marked as source of truth.

### Rule 2: only `active` docs represent current guidance

If a document is `draft`, `superseded`, or `archived`, it should not be treated as current policy or current architecture.

### Rule 3: supersession must be explicit

When a new doc replaces an old one, the replacement should point to the old doc via `supersedes`, and the old doc should be downgraded to `superseded`.

### Rule 4: indexes should prefer active truth

The `docs/desktop-shell/README.md` hub should link primarily to:

- active docs
- source-of-truth docs

Older materials can remain in the repo, but they should not dominate the main navigation.

### Rule 5: verification must be date-based

`last_verified` should be updated when a document is explicitly checked against current code or workflow.

This creates a lightweight freshness signal without requiring a heavy governance process.

## Relationship Between Document Types

The system distinguishes between stable knowledge and change history:

- `architecture`, `tokens`, and `operations` describe current truth
- `decisions` explain why truth looks the way it does
- `specs` and `plans` describe how truth changed or is about to change

When work is completed:

- the implementation history remains in `specs/` and `plans/`
- the durable outcome must be reflected back into `architecture/`, `tokens/`, or `operations/`

This prevents old specs from becoming accidental architecture documentation.

## Role of `AGENTS.md`

In this system, `AGENTS.md` is a map and a rules file, not a product encyclopedia.

It should include:

- the path to the `desktop-shell` docs hub
- a short definition of each document category
- the rule that architecture changes must update architecture docs
- the rule that new initiatives should start with spec and plan docs
- a short documentation hygiene checklist

It should not include:

- long architecture narratives
- large token tables
- historical design text
- implementation detail that is likely to drift quickly

This follows the central insight from the OpenAI article: the index should stay short, and the durable knowledge should live in real documents close to the work.

## Update Lifecycle

The expected documentation lifecycle for `desktop-shell` changes should be:

1. identify a problem, opportunity, or refactor target
2. create or update a `spec`
3. create or update a `plan` after design approval
4. implement the change in code
5. update `architecture`, `tokens`, or `operations` to reflect the landed state
6. mark replaced docs as `superseded` when appropriate
7. refresh the hub index and `last_verified` dates where needed

This ensures process docs remain process docs, and stable docs remain stable docs.

## Migration Strategy

The migration should be incremental and limited to `desktop-shell`.

### Phase 1: establish the structure

Create:

- `docs/desktop-shell/README.md`
- `docs/desktop-shell/architecture/`
- `docs/desktop-shell/decisions/`
- `docs/desktop-shell/specs/`
- `docs/desktop-shell/plans/`
- `docs/desktop-shell/tokens/`
- `docs/desktop-shell/operations/`

Add minimal index pages or seed documents where needed.

### Phase 2: move the highest-value current docs

Prioritize:

- `apps/desktop-shell/FUNCTIONAL_TOKENS.md`
- `apps/desktop-shell/DESIGN_TOKENS.md`
- the existing `desktop-shell` architecture spec
- the existing `desktop-shell` implementation plan

Migration may initially preserve old files with short forwarding notes if needed to avoid breaking local habits.

### Phase 3: normalize metadata and cross-links

For the migrated docs:

- add frontmatter
- add `related` links
- mark canonical docs with `source_of_truth: true` where appropriate
- mark replaced documents as `superseded`

### Phase 4: reduce ambiguity in legacy locations

Review `apps/desktop-shell/docs/` and other `desktop-shell`-related documents in root `docs/`:

- keep documents that are still useful
- migrate those that are durable product knowledge
- leave purely historical records in place but clearly marked

## Non-Goals and Constraints

- Do not attempt a whole-repo documentation migration in this pass.
- Do not require a documentation generator, site build, or database.
- Do not force every note or draft into strict formal structure.
- Do not block active engineering work on a perfect taxonomy.

The design should improve clarity immediately with a minimal structural commitment.

## Success Criteria

This design is successful if, after implementation:

- a contributor can find the `desktop-shell` documentation hub in one step
- the current architecture truth is distinguishable from old specs and plans
- token references live in one obvious area
- `AGENTS.md` points to knowledge instead of duplicating it
- it becomes obvious which document should be updated when product changes land

---
title: Buddy Entropy, Priority, And Lifecycle Implementation Plan
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-05-07
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/specs/2026-05-07-buddy-entropy-priority-lifecycle-design.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/operations/README.md
---

# Buddy Entropy, Priority, And Lifecycle Implementation Plan

This plan turns the entropy/priority/lifecycle design into reviewable main-only
slices. Future work in this product area should follow this plan instead of
continuing from ad-hoc conversation notes.

## Execution Rules

- Do not add a new capture source unless the same slice or its prerequisite plan
  defines filtering, priority signals, and archive behavior.
- Do not make users classify every item. Buddy should propose; users confirm.
- Do not auto-delete raw material in MVP slices. Archive/down-rank only.
- Every priority or vitality recommendation must have a user-visible reason.
- Every cross-domain inference must preserve source domain, inferred use domain,
  and a reason the user can correct.
- Keep default product language neutral. Aesthetic/emotional lenses are optional.
- Update `architecture/`, `tokens/`, or `operations/` when current behavior
  changes.
- Run the minimum quality gates for every slice: `git diff --check` plus the
  build/test commands for touched surfaces.

## Dependency Gates

- E1 is the data contract gate. Do not ship durable Home/Inbox priority UI
  before old pages and new frontmatter fields are proven compatible.
- E2 may use derived priority only after E1 DTOs compile. Before that, Home
  can only use existing `purpose`, `source_refs`, and `expressed_in`.
- E3 must land before any high-volume connector, because Inbox needs grouped
  judgments before more material enters the system.
- E5 must land before Taobao/music-specific work, because those sources require
  cross-domain interpretation rather than native-category import.
- E8 must land before maintainer prompts consume user curation preferences.
- E9 must land before any default "safe archive" recommendation appears outside
  explicit test fixtures.
- E10 must approve each high-volume source plan. Capture-only source slices are
  blocked.

## Slice Acceptance Template

Every slice must answer these fields in its implementation notes:

- Goal.
- In scope.
- Out of scope.
- Data contract.
- Backend changes.
- Frontend changes.
- Documentation updates.
- Tests and smoke.
- Done means.
- Rollback notes.

## Slice E0 — Close Current UI And Documentation Drift

Scope:

- Prevent fresh-vault onboarding actions from hiding dirty Vault checkpoint work.
- Remove/ignore local `.env.local` API overrides.
- Lazy-load Connections diff only when advanced diff is open.
- Sync Sidebar architecture/token documentation after Knowledge/Rules moved
  navigation in-page.
- Keep current usability wins: Wiki simple body editing, Raw add deep links,
  Inbox empty-state capture path, Palette user-facing route hints.

Verification:

- `git diff --check`
- `cd apps/desktop-shell && npm run build`

Done means:

- `.env.local` no longer appears in `git status` and is ignored.
- Dirty empty Vault shows a checkpoint/save action before onboarding-only
  capture actions.
- Connections does not fetch diff until advanced diff is opened.
- Architecture/tokens docs match the shell Sidebar behavior.

Implementation note (2026-05-07):

- `.gitignore` now lists `.env.local` and `apps/*/.env.local` so local
  port/API overrides do not leak into commits.
- `DashboardPage.tsx` `isFreshVault` guard adds `git?.dirty !== true`
  so the onboarding override only fires when the Vault is also clean;
  the dirty path keeps "保存 X 个 Vault 改动" as the leading action.
- `ConnectionsPage.tsx` `diffQuery.enabled` is gated on
  `showAdvancedDiff`, so the raw `+`/`-`/`@@`/hunks payload is only
  fetched when the user explicitly opens advanced diff. Default view
  shows "X 项改动等待保存" + "查看高级 diff →" instead.
- Sidebar truth synced into `architecture/overview.md` (Knowledge and
  Rules no longer get shell-level secondary columns) and
  `tokens/design-tokens.md` (`.ds-workspace-sidebar` marked legacy with
  no active mount). Wiki simple-mode body editing, Raw `?add=` deep
  links, Inbox empty-state capture path, and Palette path-style hints
  all carry forward as Slice 51 baseline behavior.

## Slice E1 — Priority / Vitality Data Contract

Scope:

- Add frontmatter-compatible optional fields:
  - `priority: low | medium | high`
  - `vitality: spark | seed | growing | stable | cooling | archived | noise`
  - `priority_reason: string`
  - `last_revisited_at`
  - `next_review_at`
- Keep `priority_reason` as string in MVP; do not introduce nested frontmatter
  objects until schema and UI rendering are reviewed.
- Extend TypeScript DTOs and Rust frontmatter structs without breaking existing
  pages.
- Ensure `expressed_in` can contribute to priority in derived summaries.

Backend tests:

- `cargo test -p wiki_store`
- Maintainer/frontmatter merge tests for preserving existing fields.

Frontend tests:

- `cd apps/desktop-shell && npm run build`

Docs:

- Update architecture once fields are stable current truth.

Done means:

- Existing wiki pages with no new fields list/read/edit successfully.
- New fields round-trip through `wiki_store` without dropping `purpose`,
  `source_refs`, or `expressed_in`.
- `update_existing` preserves user-corrected fields unless the user accepts a
  replacement recommendation.
- The UI can render missing fields as "unknown" without errors.

Implementation note (2026-05-07):

- The data contract was delivered jointly with E3 (Inbox grouped
  workbench) and E5 (cross-domain extraction); E1 does not have a
  separate landing slice. Despite the shared landing, the dependency
  gate held: every consumer below already expects the optional fields.
- TypeScript DTOs in `apps/desktop-shell/src/api/wiki/types.ts` add
  `priority`, `vitality`, `priority_reason`, `last_revisited_at`,
  `next_review_at`, `source_domain`, `inferred_use_domain`, and
  `cross_domain_reason` as optional fields on raw entries, wiki page
  summaries, and inbox proposals (lines 47–51, 188–201, 294–310,
  745–748).
- Rust frontmatter parsing/serialization in `wiki_store/src/lib.rs`
  carries the same optional fields end-to-end; existing pages without
  them parse and round-trip unchanged.
- `desktop-server` `wiki_crud.rs` accepts the lifecycle and
  cross-domain fields on the maintain and proposal-apply endpoints
  (L1382–1437), forwarding them into the maintainer pipeline.
- `wiki_store::overwrite_wiki_page_content` is the human edit path and
  is documented to "deliberately preserve every user-supplied
  frontmatter field" (L4115–4118), satisfying the
  user-corrected-fields preservation done-mean.
- `priority_reason` stays a flat string per the MVP rule; nested
  schemas remain out of scope until a UI design exists.

## Slice E2 — Home Entropy Pulse MVP

Scope:

- Add Home/Pulse sections for:
  - growing themes/pages,
  - cooling/archive candidates,
  - recently expressed pages,
  - "today only review these" follow-up actions.
- Reframe empty/fresh states as safe first capture plus checkpoint safety.
- Keep Git/Vault status visible but default copy user-facing.

Verification:

- Empty vault fixture.
- Dirty empty vault fixture.
- Pages with `expressed_in`.
- Pages with `priority/vitality`.
- Narrow viewport and long Chinese copy.

Done means:

- Home shows entropy outcomes, not only raw queue counts.
- Fresh empty state invites first capture without hiding dirty Vault safety.
- `expressed_in` and priority/vitality pages appear in separate, understandable
  Home sections.
- No Home card implies automatic deletion.

Implementation note (2026-05-07):

- `features/dashboard/entropy-pulse.ts` derives the Pulse summary from
  wiki page `priority`/`vitality`/`next_review_at`, exposing
  `reviewToday`, `growing`, and `cooling` lists plus
  `highPriorityCount`, `dueReviewCount`, `coolingCount`, and a
  `hasLifecycleSignals` flag for the empty-state copy.
- `DashboardPage.tsx` renders the "减熵脉冲" section
  (`id="entropy-pulse"`, L701–751) with three columns: 今天只看这些 /
  正在增长 / 冷却 / 可归档. The section copy explicitly says
  "归档是注意力状态，不会删除证据" so no card implies deletion.
- "最近表达" (L469–472) reuses the existing `expressed_in` ranking from
  Slice 47 so expressed pages stay in their own section.
- Fresh empty state behavior is owned jointly with E0: when the Vault
  is clean and empty, Top 3 swaps to onboarding affordances (连接微信 /
  粘贴链接 / 上传文件); when the Vault is dirty, "保存 X 个 Vault 改动"
  takes precedence so checkpoint safety is never hidden.
- E9 lifecycle suggestions (`stale-spark` / `cooling-page` /
  `unexpressed-high-priority` / `noise-candidate`) plug into the same
  Pulse surface through `buildPatrolLifecycleSuggestions`, but the
  underlying Pulse contract is independent of Patrol availability.

## Slice E3 — Inbox Grouped Entropy Workbench

Scope:

- Reframe Inbox list groups around Buddy judgments:
  - Today only review these,
  - Growing,
  - Worth crystallizing,
  - Duplicate / mergeable,
  - Cooling / safe archive,
  - Needs decision.
- Keep raw item drill-down available but not first-screen dominant.
- Carry purpose and priority/vitality through single and batch accept paths.
- Add reason text to grouped recommendations.

Tests:

- Unit tests for grouping/ranking rules.
- Existing combined proposal tests remain green.
- Smoke verifies visible row controls and grouped empty/large states.

Done means:

- Inbox default view groups judgments before raw item detail.
- Large queues surface a small "today only review these" set.
- Single and batch accept paths carry purpose plus priority/vitality choices.
- Each group has a reason; users can drill into raw evidence.
- Empty Inbox copy communicates reduced entropy rather than a blank task list.

Implementation note (2026-05-07):

- `queue-intelligence.ts` owns `groupByEntropyJudgment` and lifecycle defaults
  for the six visible Inbox groups.
- The redesigned Inbox row now exposes Purpose Lens plus priority/vitality
  choices before accepting. Cooling and needs-decision groups are not bulk
  accepted.
- `POST /api/wiki/inbox/{id}/maintain` and
  `POST /api/wiki/inbox/{id}/proposal/apply` accept reviewed lifecycle fields;
  wiki writes preserve existing metadata unless a reviewed field is provided.

## Slice E4 — Raw Safe Capture And Entropy Status

Scope:

- Make Raw copy emphasize "safe capture first, Buddy will sift later".
- Add derived entropy status chips:
  - retained,
  - observing,
  - duplicate candidate,
  - crystallizable,
  - safe to archive.
- Store source-domain separately from inferred use-domain once the backend data
  contract exists.
- Keep source/visual evidence visible where available.
- Avoid mandatory tagging at capture time.

Verification:

- `?add=url|file|text` deep links.
- Duplicate URL behavior.
- File ingest.
- Empty raw library.

Done means:

- Capture does not require tags, purpose, priority, or lifecycle decisions.
- Raw cards keep source identity visible.
- Derived status chips are conservative and reversible.
- Deep-linked add modes still clear URL params after opening the panel.

Implementation note (2026-05-07):

- `features/raw/raw-entropy.ts` derives reversible raw status from
  `last_ingest_decision` plus pending Inbox raw ids.
- Raw cards now show source identity and an entropy status chip without adding
  required capture fields.
- Raw header/empty copy frames the page as safe capture first, with Inbox doing
  later sifting.

## Slice E5 — Cross-Domain Extraction MVP

Scope:

- Add source-domain and inferred-use-domain fields to DTO/frontmatter where
  stable storage is needed:
  - `source_domain`
  - `inferred_use_domain`
  - `cross_domain_reason`
- Start with conservative, evidence-bound inference:
  - shopping source used as aesthetic/decision material,
  - music source used as social/healing/creative material,
  - article source used as writing/research/building material,
  - image/screenshot source used as aesthetic/decision material.
- Surface cross-domain candidates in Inbox as "source is X, likely use is Y"
  with accept/correct/ignore actions.
- Keep native source identity visible. Buddy should say "来自购物，但可能是设计灵感",
  not silently rewrite the item into another category.
- Add Ask templates:
  - "这些购物收藏里哪些其实是设计灵感？"
  - "哪些素材的真实用途和来源 App 不一致？"
  - "把这组跨界素材提炼成一个 brief。"

Tests:

- Source domain and inferred domain remain independently serializable.
- Inbox correction updates inferred use without changing raw source identity.
- Ask answers cite original source refs.

Done means:

- Buddy can say "source is X, likely use is Y" with a reason.
- User correction changes only inferred use fields, not native source fields.
- Unknown/weak evidence stays `unknown` rather than forcing a cross-domain label.
- At least one Inbox fixture and one Ask fixture cover corrected inference.

Implementation note (2026-05-07):

- `features/cross-domain/cross-domain.ts` owns the conservative inference
  rules. It separates native source, source domain, inferred use domain, reason,
  and confidence; weak evidence stays `unknown`.
- The redesigned Inbox row now shows a visible cross-domain row with
  accept/correct/ignore controls. Corrections call
  `applyCrossDomainCorrection`, which changes inferred use without mutating
  native source/source domain.
- `POST /api/wiki/inbox/{id}/maintain` and
  `POST /api/wiki/inbox/{id}/proposal/apply` accept `source_domain`,
  `inferred_use_domain`, and `cross_domain_reason`; wiki writes preserve these
  fields during create, append, and proposal apply paths.
- Raw and Wiki DTO/frontmatter structs carry the same optional fields so future
  Taobao/music/bookmark connectors have a storage contract before any
  high-volume import is added.

## Slice E6 — Wiki Inspiration Page Support

Scope:

- Add `type: inspiration` as an allowed page type after schema review.
- Provide a Markdown template for inspiration pages:
  - insight,
  - evidence,
  - repeated elements,
  - use cases,
  - representative samples,
  - archive candidates,
  - next actions.
- Ensure WikiArticle simple mode preserves all new frontmatter.
- Add Knowledge filters for inspiration/priority/vitality.

Tests:

- Schema validation.
- Wiki edit save path.
- Search/list filters.

Done means:

- `type: inspiration` is schema-valid only after rule/template updates.
- WikiArticle simple mode preserves all new frontmatter.
- Knowledge filters can show inspiration pages without hiding existing concept
  pages.
- Inspiration page templates cite representative source refs and next actions.

Implementation note (2026-05-07):

- `wiki_store::init_wiki` now seeds `wiki/inspiration/` plus
  `schema/templates/inspiration.md`.
- `validate_wiki_page_markdown_content` accepts `type: inspiration`, and the
  seeded template includes insight, evidence, repeated elements, use cases,
  representative samples, archive candidates, and next actions.
- Knowledge classification and the left filter sidebar now include an
  inspiration type filter plus priority/vitality filters.

## Slice E7 — Ask Reflection Prompts

Scope:

- Add reflection templates:
  - What is worth continuing?
  - What can be safely archived?
  - What themes are recurring?
  - Why is this priority high?
  - Turn this cluster into a brief.
- Ensure answers include source refs and reasons.
- If evidence is weak, Ask must say it is observing.

Tests:

- Source-bound Ask still works.
- Wiki query crystallization still creates raw/inbox links.
- SSE payload remains compatible.

Done means:

- Ask reflection answers include citations plus priority reasons.
- Weak evidence answers say "still observing" instead of asserting certainty.
- Existing source binding and URL enrichment behavior remains unchanged.
- Reflection prompts can be reached without adding a new top-level route.

Implementation note (2026-05-07):

- `features/ask/ask-reflection-prompts.ts` defines the five reflection prompt
  templates and `SlashCommandPalette` exposes them through the existing
  Composer slash menu.
- `wiki_maintainer::query_wiki` now includes lifecycle and cross-domain
  metadata in the RAG context so Ask answers can cite `priority_reason` and
  explain source-domain/inferred-use-domain judgments.
- `wiki_store::read_wiki_page`, `read_wiki_page_content`, and
  `overwrite_wiki_page_content` resolve slugs across all wiki categories, which
  lets Ask retrieve `type: inspiration` pages instead of silently skipping them.

## Slice E8 — Rules Curation Preferences

Scope:

- Add editable rules for:
  - cooling windows,
  - archive thresholds,
  - high/low priority signals,
  - enabled purpose lenses,
  - source-specific filters.
- Keep default rules neutral and conservative.
- Maintainer consumes rules only after tests prove compatibility.

Tests:

- Rules file allowlist.
- Rule save invalidates Git.
- Patrol flags missing or malformed curation rules.

Done means:

- Rules can configure cooling windows, archive thresholds, and enabled optional
  lenses.
- Malformed rules are rejected or surfaced by patrol without breaking maintainer
  runs.
- Saves dirty Buddy Vault and appear in Git/checkpoint surfaces.
- Defaults remain conservative and source-neutral.

Implementation note (2026-05-07):

- `schema/curation-preferences.yml` is now seeded on vault init with
  conservative cooling, archive, priority, purpose-lens, and source-filter
  defaults.
- Rules Studio exposes the curation preferences file through the same
  allowlisted `GET/PUT /api/wiki/rules/file` editor used for guidance,
  templates, and policies. Saving it dirties Buddy Vault through the existing
  Git status surface.
- `wiki_maintainer::build_absorb_system_prompt` reads the preferences into the
  maintainer prompt only after the file passes required top-level-key
  validation. `wiki_patrol` reports a schema violation when the file is missing
  or malformed.

## Slice E9 — Lifecycle Patrol And Archive Suggestions

Scope:

- Extend patrol to identify:
  - stale sparks,
  - cooling pages,
  - unexpressed high-priority pages,
  - duplicate/noise candidates.
- Surface suggestions in Home and Inbox.
- Archive remains reversible and traceable through Buddy Vault Git.

Tests:

- `cargo test -p wiki_store`
- Patrol report unit tests.
- Home/Inbox smoke fixtures.

Done means:

- Patrol identifies stale sparks, cooling pages, unexpressed high-priority pages,
  and duplicate/noise candidates.
- Suggestions are reversible archive/down-rank actions, not deletions.
- Home and Inbox can show archive candidates with source links and reasons.
- Git audit/checkpoint behavior remains intact after archive decisions.

Implementation note:

- `wiki_patrol` now emits `stale-spark`, `cooling-page`,
  `unexpressed-high-priority`, and `noise-candidate` issues, and
  `PatrolSummary` carries counts for all four lifecycle buckets.
- Applying patrol issues creates Inbox review tasks through the existing
  idempotent path: stale sparks and high-priority unused pages use stale review;
  cooling/noise candidates use deprecate review. No action deletes raw or wiki
  evidence automatically.
- Home/Pulse ranks these Patrol lifecycle issues through
  `features/dashboard/entropy-pulse.ts`, adds them to the knowledge-quality
  risk count, and shows a visible "大浪淘沙" path to Inbox/wiki pages.
- Verification run for the slice:
  `cargo test -p wiki_patrol lifecycle_suggestions_flag_stale_cooling_unexpressed_and_noise`,
  `cargo test -p wiki_patrol full_patrol_counts_lifecycle_suggestions`,
  `cargo test -p wiki_store lifecycle_patrol_issues_map_to_reviewable_inbox_actions`,
  and `npm run build`.

## Slice E10 — External Source Readiness Gate

Scope:

- Define source readiness checklist before Taobao, music, browser bookmark, or
  other high-volume connectors:
  - source evidence,
  - dedupe strategy,
  - priority signals,
  - cross-domain extraction strategy,
  - archive behavior,
  - user-visible reason language,
  - privacy constraints.
- No high-volume connector should land before E1-E4 are stable.

Verification:

- Spec/plan update for each source.
- No connector accepted with capture-only behavior.

Done means:

- Every proposed connector includes a source readiness checklist.
- Each connector states what will be filtered, down-ranked, grouped, and
  preserved visually.
- Cross-domain assumptions are explicit and user-correctable.
- The connector has at least one smoke or integration test plan before landing.

Implementation note:

- Added `features/connections/source-readiness.ts` as the gate contract for
  high-volume sources. It requires source evidence, dedupe, priority signals,
  cross-domain extraction, reversible archive behavior, user-visible reason
  language, and privacy constraints before a connector is considered ready.
- Connections now renders the gate directly. Taobao, music, and browser
  bookmarks remain blocked because their current plans would be capture-only;
  the existing WeChat path is listed as ready because it already routes through
  Inbox review, dedupe/priority, cross-domain metadata, reversible archive
  suggestions, and Git trace.
- `source-readiness.test.ts` pins the policy that capture-only connectors are
  blocked even if they can preserve source evidence.
- Verification run for the slice: `npm run build`.

## Risk Matrix

| Risk | Mitigation |
|---|---|
| UI turns entropy work into another task list | E3 grouped judgments and "today only review these" are required before high-volume source work |
| AI makes overconfident emotional/aesthetic claims | Optional lenses only, evidence-bound reasons, weak evidence stays observing |
| Priority hides valuable material | Raw/source refs remain preserved; priority has reason and correction path |
| Archive feels destructive | MVP archive is reversible down-rank; no auto-delete |
| Cross-domain inference mislabels a source | Source domain and inferred use domain are stored separately and can be corrected |
| New frontmatter breaks old wiki pages | E1 makes fields optional and tests missing-field compatibility |
| Source connectors flood Buddy | E10 blocks capture-only connectors |
| Technical Git language overwhelms users | Default copy says save/recover; advanced diff remains behind explicit controls |

## Slice Matrix

| Slice | Primary surface | Must not regress | Minimum gate |
|---|---|---|---|
| E0 | Current UI/docs | build, env isolation, Sidebar truth | `git diff --check`, `npm run build` |
| E1 | data contract | old pages, expressed/source refs | Rust tests + build |
| E2 | Home | Git safety, long Chinese copy | build + Home fixture |
| E3 | Inbox | maintain accept paths, combined proposal | unit tests + smoke |
| E4 | Raw | low-friction capture | build + add deep links |
| E5 | cross-domain | source identity | serialization + Inbox/Ask fixtures |
| E6 | Wiki | page editing/schema | schema tests + edit smoke |
| E7 | Ask | SSE/source binding | build + session compatibility |
| E8 | Rules | allowlist/Git dirty | rules tests + build |
| E9 | Patrol | reversible archive | Rust tests + Home/Inbox fixtures |
| E10 | Connectors | no capture-only import | source spec/plan gate |

## Quality Matrix

| Surface | Required verification |
|---|---|
| Frontend-only UI | `git diff --check`, `npm run build`, relevant smoke if route-level |
| Frontmatter/schema | Rust unit tests, frontend build, architecture doc update |
| Inbox ranking/grouping | Unit tests plus smoke fixture |
| Ask runtime | frontend build, session/SSE compatibility check |
| Rules behavior | allowlist tests, Git dirty/audit verification |
| Source connector | source-specific spec/plan, no capture-only landing |

## Completion Definition

The plan is complete when Buddy can show a user:

- what has been safely captured,
- what was filtered or merged,
- what is growing,
- what deserves attention now,
- what can be safely archived,
- and which knowledge has been expressed or transformed into action.

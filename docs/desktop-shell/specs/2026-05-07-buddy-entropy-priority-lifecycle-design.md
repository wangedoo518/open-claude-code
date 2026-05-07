---
title: Buddy Entropy, Priority, And Lifecycle Design
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-05-07
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/plans/2026-05-07-buddy-entropy-priority-lifecycle-implementation-plan.md
  - docs/desktop-shell/specs/2026-04-29-buddy-tolaria-deep-product-design.md
---

# Buddy Entropy, Priority, And Lifecycle Design

This spec defines the next product direction for Buddy after the Tolaria
desktop-shell slices: Buddy must not become a larger collection box. It should
help users safely capture scattered material, continuously filter noise, surface
priority, and turn durable signals into knowledge, inspiration, decisions, and
actions.

## User Feedback Synthesis

Recent feedback highlighted a real but persona-shaped problem:

- Users often overload WeChat favorites, shopping carts, music libraries, link
  lists, and files until those surfaces become warehouses.
- Many saved items are not literal tasks. A shopping item can be design
  inspiration; a song can be an emotional marker; a quote can be identity or
  social memory.
- Users need visual/source confirmation before they can safely let material
  recede.
- A useful external brain should filter, prioritize, and reveal patterns. It
  should not just offer more folders.

The broad product insight is generalizable. The exact examples are not universal.
Buddy should therefore adopt neutral mechanisms first, then expose aesthetic,
emotional, shopping, or creative interpretation as optional purpose lenses.

## User Segments

The design must serve multiple user types with the same underlying mechanics:

| Segment | Typical captured material | Generalized need |
|---|---|---|
| Research users | papers, articles, quotes, notes | find durable research directions and stale evidence |
| Creative users | images, products, music, screenshots, references | extract patterns, cross-domain use, moodboards, briefs |
| Life-decision users | shopping, travel, health, finance, home planning | separate true decisions from impulse or expired context |
| Engineering users | docs, issues, code snippets, logs, design notes | identify reusable systems, decisions, and knowledge debt |
| Everyday collectors | chats, links, files, screenshots | reduce overload and safely archive low-attention material |

Default UI language should work for all segments. Segment-specific interpretation
belongs in enabled lenses, source strategies, or Rules preferences.

## Generalization Rules

| Feedback element | Broadly useful? | Buddy treatment |
|---|---:|---|
| Digital overload / only-save-never-use | Yes | Core default problem |
| Need to know what matters now | Yes | Priority and lifecycle metadata |
| Visual/source confirmation | Yes | Keep raw/source refs visible |
| "Big wave sifts sand" discovery process | Yes | Capture first, filter continuously |
| Cross-domain use of source material | Yes | Extract user intent beyond the source app's native category |
| Aesthetic and emotional interpretation | Partly | Optional lenses, evidence-bound |
| Shopping cart as design inspiration | Persona-specific | Optional source strategy, not default |
| Music as private social/emotional space | Persona-specific | Optional lens/source strategy |
| Automatic deletion | Risky | Not in MVP; archive/down-rank only |

## Product Principle

Buddy follows the "big wave sifts sand" principle:

> Buddy does not require users to know what they want at capture time. It first
> helps them safely retain fragments, then repeatedly filters, groups, decays,
> and prioritizes them until users can see what is worth continuing and what can
> be safely set aside.

In product language:

```text
Capture -> Filter -> Prioritize -> Crystallize -> Express -> Decay / Archive
```

In user language:

```text
先留住 -> 帮我淘洗 -> 看见模式 -> 判断要/不要 -> 结晶成行动 -> 安心放下
```

## Core Concepts

### Entropy

Entropy is the user's attention cost: duplicate material, stale links, low-signal
items, unreviewed queues, and unprioritized inspiration all increase entropy.
Buddy should report reductions in entropy, not just counts of captured items.

### Priority

Priority means "how much this item or theme deserves attention now." It is not a
task deadline. Priority must always have a reason derived from evidence.

Recommended values:

```yaml
priority: low | medium | high
priority_reason: string
```

MVP storage uses a plain string for compatibility. UI and maintainer prompts
should still produce reasons from a structured framework so the field can later
be upgraded without changing the product semantics.

### Vitality

Vitality means "whether this idea is still alive and growing." It describes the
lifecycle of a fragment or theme.

Recommended values:

```yaml
vitality: spark | seed | growing | stable | cooling | archived | noise
```

Meaning:

- `spark`: newly captured; not enough evidence yet.
- `seed`: worth observing.
- `growing`: repeated or cross-source evidence shows active interest.
- `stable`: useful but not urgent.
- `cooling`: no recent evidence or user expression.
- `archived`: safe to keep out of attention.
- `noise`: duplicate, stale, promotional, or low information density.

### Purpose

Purpose explains why a fragment matters. Existing default lenses remain:

```text
writing, building, operating, learning, personal, research
```

Optional lenses may be introduced later:

```text
aesthetic, healing, identity, shopping, social, creative, decision, archive
```

Default Buddy copy should remain neutral. Optional lenses must not produce
psychological claims without source evidence.

### Cross-Domain Extraction

Cross-domain extraction means Buddy should not equate a source's native product
category with the user's actual intent. A shopping item may be a visual design
sample; a playlist may be an emotional or social trace; a technical article may
be a writing prompt; a screenshot may be a decision reference.

Buddy should extract the hidden cross-domain use only when there is evidence:

```yaml
source_domain: shopping | music | chat | article | image | file | web | unknown
inferred_use_domain: aesthetic | research | decision | creative | social | healing | archive | unknown
cross_domain_reason: string
```

This capability is broadly useful because many users bend tools beyond their
native app boundaries. It should remain evidence-bound and reversible: the user
can accept, correct, or ignore the inferred use domain.

### Expression

Expression is evidence that knowledge has been used. Existing `expressed_in`
frontmatter refs such as `ask:<session-id>` are priority signals. Used knowledge
is more valuable than untouched collection.

## Priority Reason Framework

Buddy should evaluate priority through evidence-bound dimensions. The first MVP
uses four durable dimensions from the "key moment" framework; a fifth dimension
can be added only after the source is available and validated.

| Dimension | Question | Example evidence |
|---|---|---|
| Trend potential | Could this keep developing over a longer horizon? | repeated captures, cross-source recurrence, external trend notes |
| Personal resonance | Does the user keep returning to it or show intrinsic interest? | revisits, edits, Ask bindings, explicit keep decisions |
| System leverage | Does it help the user understand a broader system or key problem? | links multiple pages, resolves a recurring question, improves taxonomy |
| Compounding leverage | Does future reuse get easier or more valuable? | reusable brief/template, source cluster, repeated expression, low marginal cost |

Reason output should be concise:

```yaml
priority: high
vitality: growing
priority_reason: "Trend + compounding: 9 related sources in 14 days, reused in Ask twice, can become a reusable brief."
```

Buddy must not present weak signals as certainty. When evidence is thin, use
`spark` or `seed` and explain that the item is being observed.

## Data Contract

The stable MVP fields are optional and backward-compatible. Missing fields mean
"unknown", not an error.

### Raw metadata

Raw entries should preserve the native source identity and may later receive
derived entropy metadata in raw frontmatter or `.clawwiki` metadata:

```yaml
source_domain: shopping | music | chat | article | image | file | web | unknown
inferred_use_domain: aesthetic | research | decision | creative | social | healing | archive | unknown
cross_domain_reason: string
entropy_status: retained | observing | duplicate_candidate | crystallizable | safe_archive | noise
```

### Wiki frontmatter

Crystallized pages may carry:

```yaml
priority: low | medium | high
vitality: spark | seed | growing | stable | cooling | archived | noise
priority_reason: string
last_revisited_at: string
next_review_at: string
source_domain: shopping | music | chat | article | image | file | web | unknown
inferred_use_domain: aesthetic | research | decision | creative | social | healing | archive | unknown
cross_domain_reason: string
```

### Merge rules

- Existing `purpose`, `expressed_in`, and `source_refs` must never be dropped
  when priority fields are added.
- `update_existing` should preserve user-corrected `priority`, `vitality`, and
  domain fields unless the user explicitly accepts a new recommendation.
- Derived cooling can be computed without persisting until a user accepts an
  archive/cooling decision.
- `archived` is an attention state, not deletion. Raw evidence remains.

## Signal Model

Priority and vitality should be derived from transparent signals:

- Repeated appearance across time.
- Cross-source resonance.
- Source count and source diversity.
- User revisits, edits, Ask bindings, and `expressed_in`.
- Recent raw evidence.
- Similarity to existing pages.
- Staleness and time decay.
- Duplicate, promotional, or low-density content.
- Source-domain / use-domain mismatch, such as a shopping item repeatedly used
  as design inspiration.
- User confirmations: keep, observe, crystallize, archive.

Every AI-derived recommendation must include a short reason. If the evidence is
weak, Buddy should say it is observing rather than assert a conclusion.

## Module Contracts

### Raw

Raw remains the safe capture layer. It should reduce capture-time friction:

- Do not force tags or priority at capture time.
- Preserve source URL, title, visual evidence when available, and original
  context.
- Preserve the source domain separately from any inferred use domain.
- Show lightweight status such as "已留住", "观察中", "可能重复", or "可安心归档".

### Inbox

Inbox becomes the entropy workbench. It should not present a debt queue by
default. It should group Buddy's judgments:

- Today only review these.
- Growing themes.
- Worth crystallizing.
- Cross-domain signals worth extracting.
- Duplicate or mergeable.
- Cooling / safe to archive.
- Needs user decision.

Users should confirm a few judgments, not manually classify every raw item.

### Wiki / Knowledge

Wiki stores crystallized results, not every fragment. It supports two broad page
families:

- Knowledge pages: facts, concepts, people, topics, comparisons.
- Inspiration pages: themes, repeated patterns, source evidence, reusable briefs,
  and action suggestions.
- Cross-domain pages: themes whose source material comes from one domain but is
  valuable in another, such as shopping-to-aesthetic or music-to-social.

The MVP can represent inspiration pages as Markdown with frontmatter; a visual
moodboard is not required in the first implementation.

### Home / Pulse

Home should summarize entropy outcomes:

- How many fragments were captured.
- How many were merged, down-ranked, or archived.
- Which themes are growing.
- Which items need only a small user decision.
- Which knowledge was recently expressed.

Git/Vault safety remains visible, but copy should emphasize saving and recovery
instead of raw Git jargon for default users.

### Ask

Ask becomes a reflection and action surface in addition to retrieval:

- What is worth continuing?
- What can I safely archive?
- What themes are recurring?
- What source material is actually serving another purpose?
- Why is this priority high?
- Turn this cluster into a brief, shortlist, or next action.

Answers must cite source refs and priority reasons.

### Rules

Rules becomes the place for curation preference:

- Cooling windows.
- Archive thresholds.
- Signals that raise or lower priority.
- Enabled purpose lenses.
- Source-specific filters.
- Do-not-delete policy.

### Connections

New external source connections must not be accepted unless they define a
filtering strategy. "Import more" is not sufficient. Each source must answer:

- What is captured?
- What gets filtered?
- What priority signals exist?
- Which cross-domain uses are plausible and how they are evidenced?
- What visual/source confirmation is preserved?
- What is the safe archive behavior?

## Safety And Privacy

- Buddy must not auto-delete captured material in the MVP.
- Archive means down-rank and remove from active attention, while retaining raw
  evidence and Git traceability.
- Emotional or aesthetic labels are suggestions with evidence, not judgments
  about the user.
- Local-first storage and Buddy Vault Git remain the trust foundation.
- External AI write behavior remains gated by existing authorization policy.

## Risk Controls

| Risk | Control |
|---|---|
| Buddy becomes another inbox of work | Show grouped judgments and "today only review these", not raw queue debt |
| AI over-interprets emotion or identity | Keep optional lenses evidence-bound and user-correctable |
| Priority hides important material | Preserve raw evidence, expose reasons, support correction and source links |
| Archive feels like loss | Use reversible archive/down-rank language, never delete in MVP |
| Cross-domain inference is wrong | Store source domain and inferred use separately; show reason and correction controls |
| Data fields break old pages | All new fields optional; missing means unknown |
| New source connectors flood the system | Require source readiness gate before high-volume capture |
| Git/checkpoint burden leaks into default UX | Keep safety visible, but default copy should say save/recover rather than raw Git operations |

## Non-Goals

- No immediate Taobao or music integration before entropy/priority primitives
  exist.
- No automatic psychological profiling.
- No forced manual priority tagging at capture time.
- No unsupported inference that a source item has hidden emotional/aesthetic
  meaning without evidence or user confirmation.
- No replacement of Raw with a vector-only RAG store.
- No broad UI rewrite that bypasses current `raw -> inbox -> wiki -> ask -> git`
  architecture.

## Acceptance Criteria

The direction is considered product-ready when:

- Home can explain entropy outcomes, not only queue size.
- Inbox can show grouped judgments with reasons.
- Wiki pages can carry priority/vitality without breaking existing pages.
- Ask can answer "what matters now / what can be set aside" with citations.
- Rules can hold curation preference.
- New source plans include filter and lifecycle behavior before capture volume.

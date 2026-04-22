# `components/ds/` — ClawWiki Design System shared primitives

Editorial, warm, Claude-aligned primitives extracted from the
`ClawWiki Design System/ui_kits/desktop-shell-v2/` reference kit.
Created in DS1.7-B (post DS1.6 token audit) to retire inline
component duplicates that had spread across Dashboard / Settings /
WeChat / Knowledge Hub pages.

All components in this directory:

- are consumed from multiple Pages (or planned to be) — single-
  consumer helpers should stay inline in their page for now
- render with only DS tokens (CSS vars + `.ds-*` utility classes
  defined in `apps/desktop-shell/src/globals.css`); **no arbitrary
  hex or px outside of what the token layer already sanctions**
- carry a JSDoc header pointing at the v2 kit source file + line
  range that they align with
- expose typed `interface XxxProps` — **never** `any`

## Catalog

### `SkillCard`

Pastel action card used on the Dashboard home hero grid.

| | |
|---|---|
| **v2 kit source** | `ui_kits/desktop-shell-v2/Home.jsx:4-12` |
| **DS class** | `.ds-skill-card .ds-skill-c1..c5` |
| **Props** | `{ variant: "c1"\|"c2"\|"c3"\|"c4"\|"c5"; title: string; sub: string; icon: LucideIcon; href?: string; onClick?: () => void }` |
| **Consumers** | `features/dashboard/DashboardPage.tsx` (× 4 cards) |

```tsx
import { SkillCard } from "@/components/ds/SkillCard";
import { MessageCircle } from "lucide-react";

<SkillCard
  variant="c1"
  title="问一个问题"
  sub="让 AI 基于你喂的内容回答"
  icon={MessageCircle}
  href="/ask"
/>
```

### `StatCard`

Shared stat chip with two layouts: `"row"` (horizontal, clickable,
Dashboard default) and `"compact"` (vertical grid cell, Settings).

| | |
|---|---|
| **v2 kit source** | n/a — merges two pre-DS1.7 local impls |
| **Migrated from** | `features/dashboard/DashboardPage.tsx` (`SlimStat`) + `features/settings/sections/private-cloud/SubscriptionCodexPool.tsx` (`StatCard`) |
| **DS class** | `.shadow-warm-ring` + plain Tailwind surface tokens |
| **Props** | `{ icon: LucideIcon; label: string; value: string\|number; hint?: string; to?: string; onClick?: () => void; tone?: "default"\|"warn"\|"ok"; tint?: string; layout?: "row"\|"compact" }` |
| **Consumers** | DashboardPage × 3 (layout="row") · SubscriptionCodexPool × 5 (layout="compact") |

```tsx
import { StatCard } from "@/components/ds/StatCard";
import { InboxIcon } from "lucide-react";

<StatCard
  icon={InboxIcon}
  label="待审阅"
  value={inbox.pending_count}
  hint={`共 ${inbox.total_count} 条任务`}
  to="/inbox"
  tone={inbox.error ? "warn" : "default"}
/>
```

### `StepRow`

Onboarding step primitive for the WeChat bridge's three-step flow
(and future multi-step surfaces).

| | |
|---|---|
| **v2 kit source** | `ui_kits/desktop-shell-v2/Connect.jsx:17-51` (hand-written 3 steps) |
| **DS class** | `.ds-step-row` + `data-state="pending\|active\|done"` |
| **Props** | `{ n: number; title: string; desc?: string; state: "pending"\|"active"\|"done"; children?: ReactNode; icon?: LucideIcon }` |
| **Consumers** | `features/wechat/WeChatBridgePage.tsx` (`OnboardingSteps` × 3 steps) |

```tsx
import { StepRow } from "@/components/ds/StepRow";

<StepRow n={1} title="扫码绑定微信小号" state="active"
  desc="用你的主号扫一下，就能和外脑小号建立连接。">
  <button onClick={onStartBind}>开始扫码绑定</button>
</StepRow>
```

### `PillTabs`

Editorial pill-style tablist with ARIA tablist / tab / tabpanel
wiring. Accepts an `idPrefix` so consumers can colocate the
tabpanel `id` + `aria-labelledby` correctly.

| | |
|---|---|
| **v2 kit source** | `ui_kits/desktop-shell-v2/Shell.jsx:46-52` (TopBarV2) |
| **DS class** | `.ds-pill-tabs` + `.ds-pill-tab[data-active]` |
| **Props** | `{ tabs: PillTab[]; active: string; onChange: (id) => void; ariaLabel: string; idPrefix?: string }` |
| **Consumers** | `features/wiki/KnowledgeHubPage.tsx` (Pages / 关系图 / 素材库) |

```tsx
import { PillTabs } from "@/components/ds/PillTabs";

<PillTabs
  tabs={HUB_TABS}
  active={view}
  onChange={(id) => setView(id)}
  ariaLabel="知识库视图"
  idPrefix="knowledge-hub"
/>
```

### `ListItem`

Editorial 3-column row for knowledge-base-style lists (icon | title
+ summary + meta | chevron). Optional `category` prop drives the
first badge of the meta row.

| | |
|---|---|
| **v2 kit source** | `ui_kits/desktop-shell-v2/KnowledgeBase.jsx:30-44` |
| **DS class** | `.ds-kb-item` + `.ds-kb-icon / .ds-kb-title / .ds-kb-summary / .ds-kb-meta-row / .ds-kb-badge-<cat> / .ds-kb-chevron` |
| **Props** | `{ icon: LucideIcon; title: string; summary?: string; meta?: ReactNode; onClick?: () => void; category?: "concept"\|"person"\|"topic"\|"compare"\|"unknown"; href?: string }` |
| **Consumers** | `features/wiki/KnowledgePagesList.tsx` |

```tsx
import { ListItem } from "@/components/ds/ListItem";
import { FileText, Hash } from "lucide-react";

<ListItem
  icon={FileText}
  title={page.title}
  summary={page.summary}
  category="concept"
  onClick={() => navigate(`/wiki/${page.slug}`)}
  meta={
    <>
      <span><Hash className="size-3" /> 来自素材 #{page.source_raw_id}</span>
      <span>更新 · {page.updatedLabel}</span>
    </>
  }
/>
```

## Adding a new shared component

Before you drop a new file in this directory, confirm:

1. **There are at least 2 consumers.** If only one Page needs the
   markup, keep it inline. The historical DS1.1–1.5 experiment was
   "draft visuals inline first, extract later" — that remains the
   right sequence.
2. **The visual is defined in `ClawWiki Design System/ui_kits/desktop-shell-v2/`**
   OR has a path-risk-matrix rationale to diverge. Every file in
   this folder has a JSDoc top comment pointing at the v2 kit
   source line range it implements. Keep that discipline.
3. **Props carry a named TypeScript interface.** No `any`,
   no inline `({ a: number; b: string }: {...})` signatures on a
   reused component.
4. **Styling comes through `.ds-*` classes or CSS tokens**,
   not hardcoded hex / px. DS1.6-B pruned drift; don't re-seed it
   from `components/ds/`.
5. **Dead-code hygiene.** If a component stops being consumed,
   delete the file in the next sprint (DS1.7-B-δ removed
   `features/ask/QuickActionsBar.tsx` for this reason).

## Out of scope for DS1.7-B

These were identified in the DS1.7-A audit but left for later:

- **InboxPage row** (2000+ LOC page, 1 consumer, complex state) —
  defer to DS2.x, treat as Pages-as-components
- **RawLibraryPage row** — same reason
- **SettingsPage `.ds-settings-nav-item`** — vertical-nav pattern
  is intentional, not a `PillTabs` candidate
- **Ask core** (`Message`/`StreamingMessage`/`Composer`/`AskMarkdown`/
  `AskCodeBlock`/`openai_compat_streaming.rs`) — hard-protected
  from batch 1 onward
- **TopBarV2** unification — each Page's header shape is
  differentiation signal; don't force a shared TopBar yet

See `memory/corrections.jsonl` (DS1.6-B / DS1.7 entries once they
land) for the historical context of each exclusion.

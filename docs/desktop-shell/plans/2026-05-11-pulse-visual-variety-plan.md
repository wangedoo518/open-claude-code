# Pulse Visual Variety (E28 — Option B) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add 3 visual-variety elements to the Home (Pulse) page so it stops feeling like 6 identical-shaped white cards stacked. Specifically: (1) a real 7-day sparkline next to the inbox stat (the only metric with time-series-quality data on the client today), (2) Top 3 action rows get category icons + a 4px tone-tinted left bar so each row reads as "a kind of action", (3) a "今日观察" hero callout card between the page header and Top 3 — surfaces one in-context observation each visit (e.g. "27 个 Vault 改动堆了 2 天了，要 checkpoint 吗？").

**Architecture:** All three additions are layered on top of E23's existing layout — the structure stays the same, only the rendered atoms change. New `Sparkline` component is pure inline SVG (no charting library — keeps bundle clean and gives us terracotta-themed control). The 7-day inbox bucket comes from `listInboxEntries` data we already pull. The observation generator is a pure function over the existing `useQuery` state — easy to unit-test, easy to expand with new rules. Top 3 gets a small pure helper `categorizeAction(href)` that maps URL fragments to `{icon, tone}`.

**Tech Stack:** React 19 + TypeScript + Tailwind 4 + lucide-react. **No new dependencies.** All visualizations are inline SVG.

**Slicing:** 3 epics, 6 tasks. Epics independent — can ship in any order. Recommended sequence:
- **E28.1 — Sparkline** (T1+T2): foundation for any future sparkline work
- **E28.2 — Top 3 visual variety** (T3): smallest scope, fastest user-visible payoff
- **E28.3 — 今日观察 hero** (T4+T5): biggest single-element impact

**Why this design (over alternatives):**
- **Inline SVG sparkline, not recharts/chartjs** — 3KB total, full theme-token control, no async-import cost. Sparkline is too small for any of recharts' heavy features to matter.
- **Sparkline only for inbox, not all 3 stats** — the other 2 stats are snapshot-only in the current backend; faking sparklines would lie about the data. v1 ships honest variety: 1 chart + 2 numbers is itself a visual contrast.
- **Observation generator is pure + table-driven** — adding a new observation kind is just appending a rule to an array, not editing the rendering code.
- **Hero card position: between header and Top 3** — the observation IS context-setting for the actions below it; placing it above naturally directs attention.

**Out of scope (defer to E29+):**
- Sparklines for 知识质量 / Git 改动 (waiting for backend time-series support)
- Collapsed-section preview viz (the §4-§5 items from the original B/C menu)
- Recent activity strip (bottom of page)
- Animation / transition rules
- "Streak / 健康 calendar heatmap"
- Per-purpose mini-charts inside the Pulse digest section

---

## Slice E28.1 — Sparkline component + inbox 7-day chart

After this slice ships: the inbox row in the compact stat strip shows an inline 7-day sparkline next to the count.

### Task 1: Sparkline pure-SVG component + test

**Files:**
- Create: `apps/desktop-shell/src/components/Sparkline.tsx`
- Create: `apps/desktop-shell/src/components/sparkline.test.ts` (ambient-vitest contract test)

**Step 1: Write the failing test**

`apps/desktop-shell/src/components/sparkline.test.ts`:

```typescript
/**
 * Slice E28.1 — Sparkline pure-helper test.
 *
 * Ambient-vitest contract: type-checks via `tsc --noEmit`, runs
 * verbatim once vitest is wired up. Mirrors `roi-pulse.test.ts`.
 */

import { computeSparklinePath, type SparklinePoint } from "./Sparkline";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toContain(expected: string): void;
  toEqual(expected: unknown): void;
};

describe("computeSparklinePath", () => {
  it("emits an SVG path string starting with M for non-empty data", () => {
    const path = computeSparklinePath([1, 2, 3], { width: 60, height: 16 });
    expect(path.startsWith("M")).toBe(true);
  });

  it("returns empty string for empty data (caller hides the chart)", () => {
    expect(computeSparklinePath([], { width: 60, height: 16 })).toBe("");
  });

  it("stays inside the bounding box even for flat data", () => {
    // All-equal data should produce a horizontal line, not NaN.
    const path = computeSparklinePath([5, 5, 5, 5], { width: 60, height: 16 });
    // Should NOT contain NaN or undefined.
    expect(path.includes("NaN")).toBe(false);
  });

  it("handles single-point data without crashing", () => {
    const path = computeSparklinePath([7], { width: 60, height: 16 });
    expect(path.startsWith("M")).toBe(true);
  });
});
```

**Step 2: Run to confirm failure**

```bash
cd apps/desktop-shell && npx tsc --noEmit 2>&1 | head -10
```
Expected: error — `Cannot find module './Sparkline'`.

**Step 3: Implement Sparkline + computeSparklinePath**

`apps/desktop-shell/src/components/Sparkline.tsx`:

```tsx
/**
 * Slice E28.1 — inline-SVG sparkline.
 *
 * Pure-SVG implementation, no charting library. Lives in
 * components/ so any future surface (e.g. a draft list, an
 * inbox row) can drop one in without bundle cost.
 *
 * Renders nothing when `data.length === 0` — caller doesn't need
 * to gate the mount. Flat data (all values equal) renders as a
 * horizontal line at the y-midpoint.
 *
 * Width / height default to a compact 60×16 — adjust at the call
 * site for taller / wider variants. Color defaults to the primary
 * terracotta token; pass `color` to override.
 */

export interface SparklinePoint {
  /** Numeric value to plot. */
  value: number;
}

export interface SparklineProps {
  /** Series to plot. Order = oldest → newest, left → right. */
  data: number[];
  /** SVG width in px. Default 60. */
  width?: number;
  /** SVG height in px. Default 16. */
  height?: number;
  /** Stroke color. Default `var(--color-primary)`. */
  color?: string;
  /** Optional aria-label. Default "趋势". */
  ariaLabel?: string;
}

export function Sparkline({
  data,
  width = 60,
  height = 16,
  color = "var(--color-primary)",
  ariaLabel = "趋势",
}: SparklineProps) {
  const path = computeSparklinePath(data, { width, height });
  if (!path) return null;
  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={ariaLabel}
      style={{ display: "inline-block", verticalAlign: "middle" }}
    >
      <path
        d={path}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/**
 * Pure helper — compute the SVG path `d` attribute from a series.
 * Exported so tests can verify the path geometry without rendering
 * to the DOM. Empty input → empty string.
 *
 * Uses 1px padding on all sides so stroke isn't clipped at edges.
 */
export function computeSparklinePath(
  data: number[],
  opts: { width: number; height: number },
): string {
  if (data.length === 0) return "";
  const { width, height } = opts;
  const pad = 1;
  const w = width - pad * 2;
  const h = height - pad * 2;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1; // avoid div-by-zero on flat data
  const stepX = data.length > 1 ? w / (data.length - 1) : 0;
  const points = data.map((v, i) => {
    const x = pad + i * stepX;
    // Invert y so larger values render higher.
    const y = pad + h - ((v - min) / range) * h;
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });
  return `M${points.join(" L")}`;
}
```

**Step 4: Run to confirm pass**

```bash
cd apps/desktop-shell && npx tsc --noEmit
```
Expected: clean (the ambient-vitest declarations + signature both compile).

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/components/Sparkline.tsx apps/desktop-shell/src/components/sparkline.test.ts
git commit -m "$(cat <<'EOF'
feat(components): inline-SVG Sparkline + computeSparklinePath (E28.1)

Pure-SVG implementation, no charting library. 60×16 default size
fits inline next to a stat number without disrupting line height.
computeSparklinePath is exported as a pure fn so tests can verify
geometry without DOM. Handles empty / single-point / flat-data
edge cases. Defaults to var(--color-primary) so it picks up the
terracotta theme automatically.

Per docs/desktop-shell/plans/2026-05-11-pulse-visual-variety-plan.md
§E28.1.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: 7-day inbox bucket + Sparkline integration into compact stat row

**Files:**
- Create: `apps/desktop-shell/src/features/dashboard/inbox-trend.ts` (pure helper)
- Create: `apps/desktop-shell/src/features/dashboard/inbox-trend.test.ts`
- Modify: `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx` (compute trend + render Sparkline next to inbox stat)

**Step 1: Write the failing tests**

`apps/desktop-shell/src/features/dashboard/inbox-trend.test.ts`:

```typescript
import { bucketInboxByDay, type InboxTimestamped } from "./inbox-trend";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
};

const NOW = Date.parse("2026-05-11T12:00:00Z"); // arbitrary fixed "now"
const DAY = 86_400_000;

describe("bucketInboxByDay", () => {
  it("returns 7 buckets, oldest first", () => {
    const buckets = bucketInboxByDay([], { now: NOW, days: 7 });
    expect(buckets.length).toBe(7);
  });

  it("counts entries falling into each daily bucket", () => {
    // 3 entries today, 2 entries yesterday, 0 the rest.
    const entries: InboxTimestamped[] = [
      { created_at: new Date(NOW).toISOString() },
      { created_at: new Date(NOW - 1000).toISOString() },
      { created_at: new Date(NOW - 2000).toISOString() },
      { created_at: new Date(NOW - DAY).toISOString() },
      { created_at: new Date(NOW - DAY - 1000).toISOString() },
    ];
    const buckets = bucketInboxByDay(entries, { now: NOW, days: 7 });
    // buckets[6] is "today", buckets[5] is "yesterday", etc.
    expect(buckets[6]).toBe(3);
    expect(buckets[5]).toBe(2);
    expect(buckets[4]).toBe(0);
  });

  it("ignores entries older than the window", () => {
    const entries: InboxTimestamped[] = [
      { created_at: new Date(NOW - 100 * DAY).toISOString() },
    ];
    const buckets = bucketInboxByDay(entries, { now: NOW, days: 7 });
    expect(buckets.reduce((a, b) => a + b, 0)).toBe(0);
  });

  it("ignores entries with unparseable created_at", () => {
    const entries: InboxTimestamped[] = [
      { created_at: "not a date" },
      { created_at: "" },
      { created_at: new Date(NOW).toISOString() },
    ];
    const buckets = bucketInboxByDay(entries, { now: NOW, days: 7 });
    expect(buckets[6]).toBe(1);
  });
});
```

**Step 2: Run to confirm failure**

```bash
cd apps/desktop-shell && npx tsc --noEmit 2>&1 | head -10
```
Expected: error — `Cannot find module './inbox-trend'`.

**Step 3: Implement**

`apps/desktop-shell/src/features/dashboard/inbox-trend.ts`:

```typescript
/**
 * Slice E28.1 — pure helper to bucket inbox entries into daily
 * counts for the inbox-stat sparkline. Lives next to other
 * dashboard pure helpers (entropy-pulse / roi-pulse) so all "data
 * → tiny chart" derivations sit together.
 *
 * Why pure (no JSX): tests don't need React, and the same fn can
 * later feed e.g. a tooltip "3 today / 2 yesterday / 0 / 0 / ..."
 * without recomputation.
 */

const DAY_MS = 86_400_000;

/** Minimal subset of InboxEntry needed for bucketing. Loose interface
 *  so tests don't need to import the full DTO. */
export interface InboxTimestamped {
  created_at: string;
}

/** Bucket entries into daily counts ending at `now`. Returns an
 *  array of length `days`, oldest bucket first. Entries with
 *  unparseable timestamps or outside the window are ignored. */
export function bucketInboxByDay(
  entries: ReadonlyArray<InboxTimestamped>,
  opts: { now: number; days: number },
): number[] {
  const buckets = new Array<number>(opts.days).fill(0);
  // Day 0 (oldest bucket) starts at midnight (now - days*DAY_MS),
  // bucket index = floor((entry_t - day0_start) / DAY_MS).
  // Use the start of "now's day" so buckets align to local-midnight.
  const dayMidnightMs = opts.now - (opts.now % DAY_MS);
  const day0Start = dayMidnightMs - (opts.days - 1) * DAY_MS;
  for (const entry of entries) {
    const t = Date.parse(entry.created_at);
    if (!Number.isFinite(t)) continue;
    if (t < day0Start || t >= dayMidnightMs + DAY_MS) continue;
    const idx = Math.floor((t - day0Start) / DAY_MS);
    if (idx >= 0 && idx < opts.days) {
      buckets[idx] += 1;
    }
  }
  return buckets;
}
```

**Step 4: Wire into DashboardPage**

In `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx`:

1. Add imports near the top (look for the existing component imports):

```tsx
import { Sparkline } from "@/components/Sparkline";
import { bucketInboxByDay } from "./inbox-trend";
```

2. Inside `DashboardPage()`, after `pendingInbox` is computed (~line 156), add:

```tsx
const inboxTrend = useMemo(
  () =>
    bucketInboxByDay(inboxQuery.data?.entries ?? [], {
      now: Date.now(),
      days: 7,
    }),
  [inboxQuery.data?.entries],
);
```

3. In the compact stat row JSX (look for `<Inbox className="size-3.5" />` on the inbox stat), insert the sparkline AFTER the existing label (`<span>待审阅</span>`) and BEFORE the warning pill:

```tsx
<Sparkline
  data={inboxTrend}
  width={48}
  height={14}
  ariaLabel="过去 7 天 inbox 趋势"
/>
```

The result row reads visually: `[icon] [count] 待审 [tiny-chart] [warning pill if pending > 0]`.

**Step 5: Run to confirm pass**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: clean, build succeeds.

**Step 6: Visual smoke**

`npm run tauri:dev`. Open Home. The inbox stat row should now show a small line chart between "待审" and the orange "需要处理" pill. With non-zero inbox data, the line shows the recent 7-day trend.

**Step 7: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/inbox-trend.ts \
        apps/desktop-shell/src/features/dashboard/inbox-trend.test.ts \
        apps/desktop-shell/src/features/dashboard/DashboardPage.tsx
git commit -m "$(cat <<'EOF'
feat(dashboard): 7-day inbox sparkline in compact stat row (E28.1)

bucketInboxByDay is a pure helper next to entropy-pulse / roi-pulse;
tests cover empty / counted / out-of-window / unparseable
timestamps. Sparkline (Task 1) renders 48×14 inline next to the
inbox count. Other 2 stats (知识质量 / Git 改动) stay number-only
because the backend only exposes snapshot data for them — single
chart + two numbers is itself visual contrast.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Slice E28.2 — Top 3 actions: category icons + tone-tinted bars

After this slice ships: each Top 3 row shows a category icon (Vault / Inbox / Wiki / Express / Source / etc.) plus a 4px left-edge color bar matching the action's category. Solves "every row looks the same".

### Task 3: categorizeAction pure helper + Top 3 row visual update

**Files:**
- Create: `apps/desktop-shell/src/features/dashboard/action-category.ts`
- Create: `apps/desktop-shell/src/features/dashboard/action-category.test.ts`
- Modify: `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx` (Top 3 rendering)

**Step 1: Write the failing tests**

`apps/desktop-shell/src/features/dashboard/action-category.test.ts`:

```typescript
import { categorizeAction } from "./action-category";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
};

describe("categorizeAction", () => {
  it("recognises /inbox as inbox category", () => {
    expect(categorizeAction("/inbox").id).toBe("inbox");
  });

  it("recognises /connections#git as vault category", () => {
    expect(categorizeAction("/connections#git").id).toBe("vault");
  });

  it("recognises /wiki and /wiki?... as wiki category", () => {
    expect(categorizeAction("/wiki").id).toBe("wiki");
    expect(categorizeAction("/wiki?source=missing").id).toBe("wiki");
  });

  it("recognises /raw?add=... as raw-ingest category", () => {
    expect(categorizeAction("/raw?add=url").id).toBe("ingest");
    expect(categorizeAction("/raw?add=file").id).toBe("ingest");
  });

  it("recognises /wechat as wechat category", () => {
    expect(categorizeAction("/wechat").id).toBe("wechat");
  });

  it("recognises /ask as express category", () => {
    expect(categorizeAction("/ask").id).toBe("express");
  });

  it("recognises /drafts as express category", () => {
    expect(categorizeAction("/drafts").id).toBe("express");
  });

  it("recognises /rules as patrol category", () => {
    expect(categorizeAction("/rules#validation").id).toBe("patrol");
  });

  it("falls back to default for unrecognised hrefs", () => {
    const cat = categorizeAction("/something-new");
    expect(cat.id).toBe("default");
  });
});
```

**Step 2: Run to confirm failure**

```bash
cd apps/desktop-shell && npx tsc --noEmit 2>&1 | head -5
```
Expected: error — `Cannot find module './action-category'`.

**Step 3: Implement**

`apps/desktop-shell/src/features/dashboard/action-category.ts`:

```typescript
/**
 * Slice E28.2 — categorize a Top 3 action by its href so the row
 * can render a category icon + tone-tinted left bar. Pure fn so
 * tests can pin the contract without DOM.
 *
 * Adding a new category: append to CATEGORY_RULES (order matters —
 * first match wins). Each rule has a substring matcher and the
 * resulting category. Tone is the CSS color token used for the
 * row's 4px left bar.
 */

import type { LucideIcon } from "lucide-react";
import {
  BookOpen,
  FileSearch,
  FileStack,
  GitBranch,
  Inbox,
  MessageCircle,
  Send,
  ShieldCheck,
  Sparkles,
} from "lucide-react";

export type ActionCategoryId =
  | "inbox"
  | "vault"
  | "wiki"
  | "express"
  | "ingest"
  | "wechat"
  | "patrol"
  | "default";

export interface ActionCategory {
  id: ActionCategoryId;
  icon: LucideIcon;
  /** Tone token name (CSS variable). Used as `var(--{tone})` for
   *  the row's 4px left bar. Kept warm to match the cream + terra-
   *  cotta palette — no jarring blues / pinks. */
  tone:
    | "color-primary"
    | "color-warning"
    | "color-success"
    | "color-muted-foreground";
}

const CATEGORY_RULES: ReadonlyArray<{
  test: (href: string) => boolean;
  category: ActionCategory;
}> = [
  {
    test: (h) => h.startsWith("/inbox"),
    category: { id: "inbox", icon: Inbox, tone: "color-warning" },
  },
  {
    test: (h) => h.startsWith("/connections") && h.includes("git"),
    category: { id: "vault", icon: GitBranch, tone: "color-primary" },
  },
  {
    test: (h) => h.startsWith("/raw"),
    category: { id: "ingest", icon: FileStack, tone: "color-muted-foreground" },
  },
  {
    test: (h) => h.startsWith("/wechat"),
    category: { id: "wechat", icon: MessageCircle, tone: "color-success" },
  },
  {
    test: (h) => h.startsWith("/wiki"),
    category: { id: "wiki", icon: BookOpen, tone: "color-primary" },
  },
  {
    test: (h) => h.startsWith("/ask") || h.startsWith("/drafts"),
    category: { id: "express", icon: Send, tone: "color-primary" },
  },
  {
    test: (h) => h.startsWith("/rules") || h.includes("patrol"),
    category: { id: "patrol", icon: ShieldCheck, tone: "color-warning" },
  },
];

const DEFAULT_CATEGORY: ActionCategory = {
  id: "default",
  icon: Sparkles,
  tone: "color-muted-foreground",
};

export function categorizeAction(href: string): ActionCategory {
  for (const rule of CATEGORY_RULES) {
    if (rule.test(href)) return rule.category;
  }
  // Use FileSearch fallback only if href contains "source" (lifecycle
  // patrol surface common pattern); otherwise generic.
  if (href.includes("source")) {
    return { id: "patrol", icon: FileSearch, tone: "color-warning" };
  }
  return DEFAULT_CATEGORY;
}
```

**Step 4: Update Top 3 rendering in DashboardPage**

In `DashboardPage.tsx`, find the `topActions.slice(1).map((action, idx) => ...)` block (~line 425). Replace the existing row JSX with:

```tsx
{topActions.slice(1).map((action, idx) => {
  const category = categorizeAction(action.href);
  const Icon = category.icon;
  return (
    <Link
      key={`${action.href}-${action.label}`}
      to={action.href}
      className="group relative flex items-center justify-between gap-3 overflow-hidden rounded-md border border-border/60 bg-background py-2.5 pl-4 pr-3 text-[13px] transition-colors hover:border-primary/40 hover:bg-muted/30"
    >
      {/* E28.2 — 4px left tone bar (per category). Soft tint via
          color-mix so it never overwhelms the row. */}
      <span
        aria-hidden="true"
        className="absolute inset-y-0 left-0 w-[3px]"
        style={{
          backgroundColor: `color-mix(in srgb, var(--${category.tone}) 70%, transparent)`,
        }}
      />
      <span className="flex min-w-0 items-center gap-3">
        <span
          className="grid size-6 shrink-0 place-items-center rounded text-[11px] text-muted-foreground"
          style={{
            backgroundColor: `color-mix(in srgb, var(--${category.tone}) 12%, transparent)`,
          }}
          aria-hidden="true"
        >
          {idx + 2}
        </span>
        <Icon
          className="size-3.5 shrink-0"
          style={{ color: `var(--${category.tone})` }}
          aria-hidden="true"
        />
        <span className="truncate">{action.label}</span>
      </span>
      <ArrowRight className="size-3.5 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5" />
    </Link>
  );
})}
```

Add the import at the top:

```tsx
import { categorizeAction } from "./action-category";
```

**Step 5: Verify**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: clean.

`npm run tauri:dev` → Home. Each Top 3 row should now show:
- A 3px tinted left bar (color matches category)
- A small numbered chip (with category-tinted background)
- A category icon (e.g. GitBranch for "保存 27 个 Vault 改动")
- The label

**Step 6: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/action-category.ts \
        apps/desktop-shell/src/features/dashboard/action-category.test.ts \
        apps/desktop-shell/src/features/dashboard/DashboardPage.tsx
git commit -m "$(cat <<'EOF'
feat(dashboard): Top 3 actions get category icon + tone-tinted bar (E28.2)

Each Top 3 row now reads as "a kind of action" instead of yet
another text+arrow. categorizeAction is a pure fn that pattern-
matches the action's href against rule order — first match wins,
fall through to a generic Sparkles icon. Tones are restricted to
the warm palette (terracotta / warning gold / success green /
muted gray) — no jarring blues or pinks.

The 3px left bar uses color-mix at 70% opacity so it never
overwhelms; the numbered chip background is the same tone at 12%.

Per docs/desktop-shell/plans/2026-05-11-pulse-visual-variety-plan.md
§E28.2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Slice E28.3 — 今日观察 hero callout card

After this slice ships: a soft-tinted card sits between the page header and Top 3, showing one in-context observation per visit. Eye sees "today's reality" first, before the action list.

### Task 4: pickObservation pure helper + tests

**Files:**
- Create: `apps/desktop-shell/src/features/dashboard/today-observation.ts`
- Create: `apps/desktop-shell/src/features/dashboard/today-observation.test.ts`

**Step 1: Write the failing tests**

`apps/desktop-shell/src/features/dashboard/today-observation.test.ts`:

```typescript
import { pickObservation, type ObservationInputs } from "./today-observation";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toContain(expected: string): void;
  toEqual(expected: unknown): void;
};

const NOW = Date.parse("2026-05-11T12:00:00Z");
const DAY = 86_400_000;

function inputs(over: Partial<ObservationInputs> = {}): ObservationInputs {
  return {
    now: NOW,
    pendingInbox: 0,
    gitDirty: false,
    gitChangedCount: 0,
    lastGitCommitMs: NOW - DAY,
    lifecyclePatrolCount: 0,
    recentExpressionCount7d: 0,
    totalPages: 0,
    ...over,
  };
}

describe("pickObservation", () => {
  it("flags large-stale-vault when ≥20 changes + ≥1 day since commit", () => {
    const obs = pickObservation(
      inputs({ gitDirty: true, gitChangedCount: 27, lastGitCommitMs: NOW - 2 * DAY }),
    );
    expect(obs?.id).toBe("vault-stale");
    expect(obs?.message.includes("27")).toBe(true);
  });

  it("flags inbox backlog when pending ≥ 10", () => {
    const obs = pickObservation(inputs({ pendingInbox: 13 }));
    expect(obs?.id).toBe("inbox-backlog");
    expect(obs?.message.includes("13")).toBe(true);
  });

  it("celebrates expression activity when ≥3 in 7 days", () => {
    const obs = pickObservation(inputs({ recentExpressionCount7d: 4 }));
    expect(obs?.id).toBe("express-streak");
  });

  it("nudges toward expression when many pages but 0 recent expressions", () => {
    const obs = pickObservation(
      inputs({ totalPages: 30, recentExpressionCount7d: 0 }),
    );
    expect(obs?.id).toBe("knowledge-no-export");
  });

  it("falls back to default observation when nothing notable", () => {
    const obs = pickObservation(inputs());
    expect(obs?.id).toBe("default");
  });

  it("priority — vault-stale beats inbox-backlog", () => {
    const obs = pickObservation(
      inputs({
        gitDirty: true,
        gitChangedCount: 30,
        lastGitCommitMs: NOW - 2 * DAY,
        pendingInbox: 30,
      }),
    );
    expect(obs?.id).toBe("vault-stale");
  });
});
```

**Step 2: Run to confirm failure**

```bash
cd apps/desktop-shell && npx tsc --noEmit 2>&1 | head -5
```
Expected: error — `Cannot find module './today-observation'`.

**Step 3: Implement**

`apps/desktop-shell/src/features/dashboard/today-observation.ts`:

```typescript
/**
 * Slice E28.3 — pure observation generator for the Home hero card.
 *
 * Scans current Pulse state and picks ONE observation worth
 * surfacing. Rules are table-driven + ordered by priority: first
 * match wins. Adding a new observation = appending to OBSERVATIONS.
 *
 * Why pure: tests pin the contract without React; same fn could
 * later feed e.g. a status-bar tooltip or the command palette
 * empty state.
 */

const DAY_MS = 86_400_000;

export interface ObservationInputs {
  now: number;
  pendingInbox: number;
  gitDirty: boolean;
  gitChangedCount: number;
  /** ISO ms of latest git audit entry; null if never. */
  lastGitCommitMs: number | null;
  lifecyclePatrolCount: number;
  /** Count of pages with `expressed_in.length > 0 && created_at < 7 days`. */
  recentExpressionCount7d: number;
  /** Total wiki pages. Used for the "knowledge but no export" nudge. */
  totalPages: number;
}

export type ObservationId =
  | "vault-stale"
  | "inbox-backlog"
  | "patrol-issues"
  | "express-streak"
  | "knowledge-no-export"
  | "default";

export interface Observation {
  id: ObservationId;
  /** Display message, plain text. May contain interpolated counts. */
  message: string;
  /** Optional CTA route. */
  href?: string;
  /** Tone for the hero card tint. */
  tone: "warning" | "primary" | "success" | "neutral";
}

const OBSERVATIONS: ReadonlyArray<{
  id: ObservationId;
  match: (i: ObservationInputs) => boolean;
  build: (i: ObservationInputs) => Observation;
}> = [
  {
    id: "vault-stale",
    match: (i) => {
      if (!i.gitDirty || i.gitChangedCount < 20) return false;
      if (i.lastGitCommitMs == null) return true;
      return i.now - i.lastGitCommitMs >= DAY_MS;
    },
    build: (i) => {
      const hours = i.lastGitCommitMs
        ? Math.floor((i.now - i.lastGitCommitMs) / (60 * 60 * 1000))
        : null;
      const stale = hours == null
        ? ""
        : hours < 48
          ? `堆了 ${hours} 小时`
          : `堆了 ${Math.floor(hours / 24)} 天`;
      return {
        id: "vault-stale",
        message: `${i.gitChangedCount} 个 Vault 改动${stale ? " " + stale : ""}，要 checkpoint 吗？`,
        href: "/connections#git",
        tone: "warning",
      };
    },
  },
  {
    id: "inbox-backlog",
    match: (i) => i.pendingInbox >= 10,
    build: (i) => ({
      id: "inbox-backlog",
      message: `Inbox 有 ${i.pendingInbox} 条待审了，先收一下`,
      href: "/inbox",
      tone: "warning",
    }),
  },
  {
    id: "patrol-issues",
    match: (i) => i.lifecyclePatrolCount >= 5,
    build: (i) => ({
      id: "patrol-issues",
      message: `${i.lifecyclePatrolCount} 个页面建议进 patrol 看一下生命周期`,
      href: "/wiki",
      tone: "warning",
    }),
  },
  {
    id: "express-streak",
    match: (i) => i.recentExpressionCount7d >= 3,
    build: (i) => ({
      id: "express-streak",
      message: `这周你已经表达了 ${i.recentExpressionCount7d} 次！可以试试结晶一篇`,
      href: "/drafts",
      tone: "success",
    }),
  },
  {
    id: "knowledge-no-export",
    match: (i) => i.totalPages >= 10 && i.recentExpressionCount7d === 0,
    build: (i) => ({
      id: "knowledge-no-export",
      message: `积了 ${i.totalPages} 页知识，但本周还没表达过，试试导出一篇 draft`,
      href: "/drafts",
      tone: "primary",
    }),
  },
];

const DEFAULT_OBSERVATION: Observation = {
  id: "default",
  message: "今天没有特别需要看的事，继续摄入或表达 ✨",
  tone: "neutral",
};

export function pickObservation(input: ObservationInputs): Observation {
  for (const rule of OBSERVATIONS) {
    if (rule.match(input)) return rule.build(input);
  }
  return DEFAULT_OBSERVATION;
}
```

**Step 4: Run to confirm pass**

```bash
cd apps/desktop-shell && npx tsc --noEmit
```
Expected: clean.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/today-observation.ts \
        apps/desktop-shell/src/features/dashboard/today-observation.test.ts
git commit -m "$(cat <<'EOF'
feat(dashboard): pickObservation pure helper for Home hero card (E28.3a)

Table-driven rule resolver that scans Pulse state and picks ONE
observation to surface. Ordered priority: vault-stale →
inbox-backlog → patrol-issues → express-streak →
knowledge-no-export → default. Adding a new observation kind is
appending to OBSERVATIONS; no rendering changes needed.

Tests cover each rule's positive path + the priority ordering
(vault-stale beats inbox-backlog when both apply) + the default
fallback.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Hero card component + mount in DashboardPage

**Files:**
- Create: `apps/desktop-shell/src/features/dashboard/TodayObservationCard.tsx`
- Modify: `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx`

**Step 1: Build the component**

`apps/desktop-shell/src/features/dashboard/TodayObservationCard.tsx`:

```tsx
import { ArrowRight, Sparkles } from "lucide-react";
import { Link } from "react-router-dom";
import type { Observation } from "./today-observation";

/**
 * Slice E28.3 — Home hero observation card.
 *
 * Sits between the page header and Top 3 actions. Single soft-
 * tinted card that surfaces ONE in-context observation per visit
 * (computed by pickObservation). The tint is kept restrained:
 * 6% opacity on the bg, 50% on the border, full on the icon — so
 * the card reads as "a friendly note", not "an alert".
 *
 * Renders nothing for the "default" observation when caller wants
 * to omit a placeholder; pass `hideDefault` to opt in.
 */
export function TodayObservationCard({
  observation,
  hideDefault = false,
}: {
  observation: Observation;
  hideDefault?: boolean;
}) {
  if (hideDefault && observation.id === "default") return null;

  // Map observation tone to CSS color tokens.
  const toneToken =
    observation.tone === "warning"
      ? "color-warning"
      : observation.tone === "success"
        ? "color-success"
        : observation.tone === "primary"
          ? "color-primary"
          : "color-muted-foreground";

  const content = (
    <>
      <div
        className="grid size-9 shrink-0 place-items-center rounded-full"
        style={{
          backgroundColor: `color-mix(in srgb, var(--${toneToken}) 18%, transparent)`,
          color: `var(--${toneToken})`,
        }}
        aria-hidden="true"
      >
        <Sparkles className="size-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
          今日观察
        </div>
        <div className="mt-0.5 text-[13px] leading-snug text-foreground">
          {observation.message}
        </div>
      </div>
      {observation.href && (
        <ArrowRight
          className="size-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5"
          aria-hidden="true"
        />
      )}
    </>
  );

  const className =
    "group flex items-center gap-3 rounded-lg border px-4 py-3";
  const style = {
    borderColor: `color-mix(in srgb, var(--${toneToken}) 35%, var(--color-border))`,
    backgroundColor: `color-mix(in srgb, var(--${toneToken}) 6%, var(--color-card))`,
  };

  if (observation.href) {
    return (
      <Link to={observation.href} className={`${className} hover:shadow-sm`} style={style}>
        {content}
      </Link>
    );
  }
  return (
    <div className={className} style={style}>
      {content}
    </div>
  );
}
```

**Step 2: Mount in DashboardPage**

In `DashboardPage.tsx`, add imports:

```tsx
import { TodayObservationCard } from "./TodayObservationCard";
import { pickObservation } from "./today-observation";
```

Inside `DashboardPage()`, after the `inboxTrend` calculation, add:

```tsx
const observation = useMemo(
  () =>
    pickObservation({
      now: Date.now(),
      pendingInbox,
      gitDirty: Boolean(git?.dirty),
      gitChangedCount: git?.changed_count ?? 0,
      lastGitCommitMs: latestGitAudit?.timestamp_ms ?? null,
      lifecyclePatrolCount,
      recentExpressionCount7d: recentExpressions.length, // approx — top-3 already filtered to recent
      totalPages: pagesQuery.data?.pages?.length ?? 0,
    }),
  [
    pendingInbox,
    git?.dirty,
    git?.changed_count,
    latestGitAudit?.timestamp_ms,
    lifecyclePatrolCount,
    recentExpressions.length,
    pagesQuery.data?.pages?.length,
  ],
);
```

In the JSX, insert the card BETWEEN the loading spinner block (~line 405) and the Top 3 section (~line 410):

```tsx
{isLoading && (
  <div className="inline-flex items-center gap-2 text-[12px] text-muted-foreground">
    <Loader2 className="size-3.5 animate-spin" />
    正在体检…
  </div>
)}

{/* E28.3 — hero observation card */}
<TodayObservationCard observation={observation} hideDefault={false} />

<section className="rounded-lg border border-border bg-card px-4 py-4">
  ...Top 3 section...
```

**Step 3: Verify**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: clean.

**Step 4: Visual smoke**

`npm run tauri:dev`. Open Home. A new soft-tinted card with a Sparkles icon and "今日观察" caption should appear between the page header and Top 3. The text should match an in-context observation:
- If you have ≥20 Git changes + ≥1 day stale: warning-tinted "X 个 Vault 改动堆了 Y 小时/天，要 checkpoint 吗？"
- If you have ≥10 inbox pending: warning-tinted "Inbox 有 N 条待审了..."
- Etc.

Click the card → navigate to the observation's `href` if set.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/TodayObservationCard.tsx \
        apps/desktop-shell/src/features/dashboard/DashboardPage.tsx
git commit -m "$(cat <<'EOF'
feat(dashboard): 今日观察 hero card on Home (E28.3b)

A soft-tinted callout sits between the page header and Top 3
actions; surfaces ONE observation chosen by pickObservation
(vault-stale > inbox-backlog > patrol-issues > express-streak >
knowledge-no-export > default). Card tint comes from color-mix
on warning/primary/success/muted tokens — restrained enough to
read as "friendly note", not "system alert".

When the observation has an href, the whole card is a Link with
hover-shadow + arrow shift. Default observation can be hidden via
hideDefault prop (default: shown).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Bump v0.1.16 + plan link + push

**Files:**
- Modify: `apps/desktop-shell/src-tauri/tauri.conf.json` (version)
- Modify: `apps/desktop-shell/src-tauri/Cargo.toml` (version)
- Modify: `docs/desktop-shell/plans/README.md` (link this plan)

**Step 1: Final verification**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: tsc clean, build succeeds.

**Step 2: Bump**

```bash
sed -i 's/"version": "0.1.15"/"version": "0.1.16"/' apps/desktop-shell/src-tauri/tauri.conf.json
sed -i 's/^version = "0.1.15"$/version = "0.1.16"/' apps/desktop-shell/src-tauri/Cargo.toml
cd apps/desktop-shell/src-tauri && cargo check 2>&1 | tail -2
```

**Step 3: Update plan index**

In `docs/desktop-shell/plans/README.md`:

```markdown
- [Pulse Visual Variety (E28) Implementation Plan](./2026-05-11-pulse-visual-variety-plan.md)
```

**Step 4: Commit + tag + push**

```bash
git add docs/desktop-shell/plans/README.md docs/desktop-shell/plans/2026-05-11-pulse-visual-variety-plan.md \
  apps/desktop-shell/src-tauri/tauri.conf.json apps/desktop-shell/src-tauri/Cargo.toml \
  apps/desktop-shell/src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
chore: bump desktop-shell to v0.1.16 + Pulse Visual Variety plan

E28 (Option B) ships 3 visual-variety additions to Home:
  - 7-day inbox sparkline (pure-SVG, 48×14 inline next to count)
  - Top 3 actions: per-category icon + tone-tinted left bar
  - 今日观察 hero card: rule-driven observation between header
    and Top 3

No new dependencies. Pure helpers (computeSparklinePath,
bucketInboxByDay, categorizeAction, pickObservation) sit next to
the existing dashboard helpers (entropy-pulse / roi-pulse) so the
"data → small viz / decision" pattern is consistent across the
feature folder.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git tag -a v0.1.16 -m "v0.1.16: Pulse Visual Variety (E28 Option B)"
git push origin main
git push origin v0.1.16
```

---

## Done criteria

After all 6 tasks ship:

1. **Inbox sparkline**: Home compact stat row shows a small line chart between the inbox count and the warning pill. With non-zero inbox data the line shows recent 7-day trend.
2. **Top 3 visual variety**: each row shows a category icon + a 3px tone-tinted left bar; categories visually distinguishable at a glance.
3. **Hero observation**: a soft-tinted card with "今日观察" caption sits between the page header and Top 3. Content matches the most-relevant rule for current state. Default observation reads "今天没有特别需要看的事，继续摄入或表达 ✨".
4. **Tests**: 4 ambient-vitest test files type-check (sparkline, inbox-trend, action-category, today-observation). No runtime tests added.
5. **Build**: tsc clean; vite build clean; bundle size unchanged ±5KB.
6. **Visual smoke**: Home no longer reads as "6 identical cards" — there's a chart, a colored callout, and category-tinted action rows. The eye has visual rhythm.

## Risks called out

1. **Sparkline only on inbox** — the other 2 stats (知识质量 / Git 改动) stay number-only because backend doesn't expose time-series for them. Asymmetric, but intentionally honest. If users complain "why doesn't 知识质量 have a chart?", file an E29 to extend the patrol report API with a 7-day series.
2. **`recentExpressionCount7d` approximation** — current `recentExpressions` is the top-3 expressed pages by `created_at` desc. Using `.length` as the 7-day count understates anything beyond 3. For v1 acceptable; if observations feel wrong (always 0/1/2/3), promote to a real 7-day filter.
3. **Hero card color-mix support** — `color-mix(in srgb, ...)` requires Chromium 111+ / Safari 16.2+. Tauri webview is Chromium 130+ (per project — verify), so safe. If a user runs an old WebView2 runtime on Windows, the card falls back to the literal first arg color (warning gold full opacity). Acceptable degradation.
4. **Observation rule order is opinionated** — vault-stale beats inbox-backlog. If a user's mental model is different ("I care about Inbox first"), the rule order will feel wrong. Easy fix: reorder OBSERVATIONS array. Hard fix: per-user preference. Defer hard fix to E29+.
5. **Click-through navigation** — hero card with `href` becomes a clickable Link. If the user accidentally clicks the card while reading, they navigate away. Mitigation: card stays visually distinct from action rows (no chevron, no hover-blue), but if reports come in we can make the card non-clickable and instead show an "→ 处理" inline button.

## Out of scope (defer to E29+)

- Sparklines for 知识质量 / Git 改动 (waiting on backend time-series APIs)
- Collapsed-section preview viz (the original §4 from option C — "本周流动 ⌄ ▆▃▁▇▂▁ 18")
- "最近活动 strip" at bottom of Home
- Streak / 健康 calendar heatmap
- WikiArticle / Drafts / Ask page visual-variety passes
- Per-user observation rule reordering / opt-out
- Animation / transition rules

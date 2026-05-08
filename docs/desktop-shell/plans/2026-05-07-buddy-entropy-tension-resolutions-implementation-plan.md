---
title: Buddy Entropy Tension Resolutions Implementation Plan
doc_type: plan
status: draft
owner: desktop-shell
last_verified: 2026-05-07
related:
  - docs/desktop-shell/plans/2026-05-07-buddy-entropy-priority-lifecycle-implementation-plan.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/specs/2026-05-07-buddy-entropy-priority-lifecycle-design.md
---

# Buddy Entropy Tension Resolutions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Resolve six tensions surfaced in the post-E10 audit so the entropy/lifecycle thesis stays coherent through long-term use.

**Architecture:** Six independent slices (E11–E16). Each slice keeps all eight invariants from E0–E10: source ≠ use, no batch on irreversible actions, capture is free, AI cannot override facts, AI must explain, user style is artifact, AI cannot secretly mutate, gate before acquisition.

**Tech Stack:** TypeScript 5.8 / React 19 / Vite 6 / Tailwind 4 / Zustand 5 / React Query 5 (frontend), Rust workspace + Tauri 2 (backend), Vitest + Cargo test (testing).

---

## Execution Rules

- **TDD per task:** write the failing test first, run it, see it fail, then write the minimum code to pass.
- **Commit after each green test.** No batch commits.
- **Run `cd apps/desktop-shell && npm run build`** + relevant `cargo test -p <crate>` before the slice's last commit.
- **Backwards compatible:** existing `wiki_store` data must keep working without migration.
- **No new dependencies** without justification — extend existing modules where possible.
- **Reuse existing primitives:** `EmptyState`, `useDeepLinkState`, `namespacedStorage`, `useQuery`, `wiki_store::WikiPaths`. Do not invent parallels.
- **Update `architecture/overview.md`** when behavior visibly changes.

---

## Dependency Gates

- **E13 (cross-domain telemetry)** is independent; can land first.
- **E11 (inspiration cluster)** benefits from E13's feedback signal but does not block on it.
- **E14 (cooling calibration)** uses the existing E9 patrol audit log — already shipped.
- **E15 (re-emergence)** needs E5 (cross-domain) + E9 (Patrol routing) — both shipped.
- **E16 (synthesis-to-page)** needs E6 (inspiration page) + E7 (reflection prompts) — both shipped.
- **E11 + E12** are presentation-layer; can land any order.
- **Recommended landing order:** E13 → E14 → E15 → E11 → E16 → E12.

---

## Tension → Slice Mapping

| # | Tension (from audit) | Slice |
|---|---|---|
| 1 | Inspiration page vs cross-domain annotation 边界不清 | E11 |
| 2 | 美学 / 情感 lens 是差异化但 opt-in 难发现 | E12 |
| 3 | 跨界推断质量没有反馈回路 | E13 |
| 4 | Patrol cooling 阈值默认值无依据 | E14 |
| 5 | 归档页再涌现 (re-emergence) 没机制 | E15 |
| 6 | `/theme-brief` 只产生答案不沉淀 wiki | E16 |

---

## Slice E11 — Inspiration ↔ Cross-Domain Decision UX

**Tension:** A user with 3+ raw entries sharing the same `inferred_use_domain` faces an unclear choice: tag each as cross-domain (E5) or aggregate them as an `inspiration` wiki page (E6). The plan does not offer a decision affordance, so users default to neither, and the differentiator is lost.

**Resolution:** When ≥ 3 unconverted Inbox entries share an `inferred_use_domain`, surface a single "把这 N 条聚合成 inspiration 页" suggestion at the top of Inbox. Accepting creates a draft inspiration page with the entries pre-populated as `source_refs` and stamped `crystallized_into: <slug>` so they no longer cluster.

**Done means:**
- Inbox shows a cluster suggestion when ≥ 3 entries share `inferred_use_domain`.
- Suggestion creates a draft `type: inspiration` page; raw entries get `crystallized_into: <slug>`.
- Same entry never appears in two clusters.
- Empty `inferred_use_domain` is never clustered (`unknown` does not aggregate).

### Task E11.1: Detect inspiration cluster in queue-intelligence

**Files:**
- Modify: `apps/desktop-shell/src/features/inbox/queue-intelligence.ts` (append exports)
- Test: `apps/desktop-shell/src/features/inbox/queue-intelligence.test.ts` (append `describe` block)

**Step 1: Write the failing test**

```typescript
// In queue-intelligence.test.ts — add inside the existing describe scaffold
describe("inspiration cluster detection", () => {
  it("groups 3+ entries sharing inferred_use_domain", () => {
    const entries = [
      { id: 1, inferred_use_domain: "design-reference" },
      { id: 2, inferred_use_domain: "design-reference" },
      { id: 3, inferred_use_domain: "design-reference" },
      { id: 4, inferred_use_domain: "writing" },
    ];
    const clusters = detectInspirationClusters(entries);
    expect(clusters).toHaveLength(1);
    expect(clusters[0].domain).toBe("design-reference");
    expect(clusters[0].entryIds).toEqual([1, 2, 3]);
  });

  it("does not cluster when fewer than 3 share a domain", () => {
    expect(
      detectInspirationClusters([
        { id: 1, inferred_use_domain: "design-reference" },
        { id: 2, inferred_use_domain: "design-reference" },
      ]),
    ).toEqual([]);
  });

  it("ignores entries already crystallized", () => {
    expect(
      detectInspirationClusters([
        { id: 1, inferred_use_domain: "design-reference", crystallized_into: "moodboard-1" },
        { id: 2, inferred_use_domain: "design-reference" },
        { id: 3, inferred_use_domain: "design-reference" },
      ]),
    ).toEqual([]);
  });

  it("ignores unknown / null inferred_use_domain", () => {
    expect(
      detectInspirationClusters([
        { id: 1, inferred_use_domain: null },
        { id: 2, inferred_use_domain: "unknown" },
        { id: 3, inferred_use_domain: undefined },
      ]),
    ).toEqual([]);
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/features/inbox/queue-intelligence.test.ts -t "inspiration cluster"`
Expected: FAIL with `detectInspirationClusters is not defined`.

**Step 3: Write minimal implementation**

```typescript
// In queue-intelligence.ts (append after the existing exports)
export interface InspirationClusterEntry {
  id: number;
  inferred_use_domain?: string | null;
  crystallized_into?: string | null;
}

export interface InspirationCluster {
  domain: string;
  entryIds: number[];
}

const MIN_INSPIRATION_CLUSTER_SIZE = 3;

export function detectInspirationClusters(
  entries: ReadonlyArray<InspirationClusterEntry>,
): InspirationCluster[] {
  const byDomain = new Map<string, number[]>();
  for (const entry of entries) {
    if (entry.crystallized_into) continue;
    const domain = entry.inferred_use_domain;
    if (!domain || domain === "unknown") continue;
    const list = byDomain.get(domain) ?? [];
    list.push(entry.id);
    byDomain.set(domain, list);
  }
  const clusters: InspirationCluster[] = [];
  for (const [domain, entryIds] of byDomain) {
    if (entryIds.length >= MIN_INSPIRATION_CLUSTER_SIZE) {
      clusters.push({ domain, entryIds: [...entryIds].sort((a, b) => a - b) });
    }
  }
  return clusters;
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/features/inbox/queue-intelligence.test.ts -t "inspiration cluster"`
Expected: PASS (4/4).

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/inbox/queue-intelligence.ts apps/desktop-shell/src/features/inbox/queue-intelligence.test.ts
git commit -m "feat(inbox): detect inspiration clusters from inferred_use_domain"
```

### Task E11.2: Render inspiration cluster banner in InboxPage

**Files:**
- Create: `apps/desktop-shell/src/features/inbox/InspirationClusterBanner.tsx`
- Modify: `apps/desktop-shell/src/features/inbox/InboxPage.tsx` (above grouped list, after empty-state branch)
- Test: `apps/desktop-shell/src/features/inbox/InspirationClusterBanner.test.tsx`

**Step 1: Write the failing test**

```tsx
// InspirationClusterBanner.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { InspirationClusterBanner } from "./InspirationClusterBanner";

describe("InspirationClusterBanner", () => {
  it("shows N count and domain label", () => {
    render(
      <InspirationClusterBanner
        cluster={{ domain: "design-reference", entryIds: [1, 2, 3] }}
        onAccept={() => {}}
        onDismiss={() => {}}
      />,
    );
    expect(screen.getByText(/3/)).toBeInTheDocument();
    expect(screen.getByText(/design-reference|设计参考/)).toBeInTheDocument();
  });

  it("calls onAccept with entryIds when '聚合成灵感页' clicked", () => {
    const onAccept = vi.fn();
    render(
      <InspirationClusterBanner
        cluster={{ domain: "design-reference", entryIds: [1, 2, 3] }}
        onAccept={onAccept}
        onDismiss={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /聚合/ }));
    expect(onAccept).toHaveBeenCalledWith([1, 2, 3], "design-reference");
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/features/inbox/InspirationClusterBanner.test.tsx`
Expected: FAIL with `Cannot find module './InspirationClusterBanner'`.

**Step 3: Write minimal implementation**

```tsx
// InspirationClusterBanner.tsx
import { Sparkles, X } from "lucide-react";
import type { InspirationCluster } from "./queue-intelligence";

const DOMAIN_LABELS: Record<string, string> = {
  "design-reference": "设计参考",
  "writing": "写作素材",
  "research": "研究参考",
};

export interface InspirationClusterBannerProps {
  cluster: InspirationCluster;
  onAccept: (entryIds: number[], domain: string) => void;
  onDismiss: () => void;
}

export function InspirationClusterBanner({
  cluster,
  onAccept,
  onDismiss,
}: InspirationClusterBannerProps) {
  const label = DOMAIN_LABELS[cluster.domain] ?? cluster.domain;
  return (
    <div className="ds-inspiration-banner flex items-center gap-3 rounded-md border border-border bg-accent/30 px-4 py-3">
      <Sparkles className="size-4 text-primary" aria-hidden />
      <div className="flex-1 text-[12px]">
        <span className="font-medium">{cluster.entryIds.length}</span> 条素材都标了「{label}」——
        要把它们聚合成一个 <span className="font-medium">灵感页</span> 吗？
      </div>
      <button
        type="button"
        className="rounded-md bg-primary px-3 py-1 text-[12px] text-primary-foreground"
        onClick={() => onAccept(cluster.entryIds, cluster.domain)}
      >
        聚合成灵感页
      </button>
      <button
        type="button"
        className="rounded p-1 text-muted-foreground hover:bg-muted"
        onClick={onDismiss}
        aria-label="关闭建议"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/features/inbox/InspirationClusterBanner.test.tsx`
Expected: PASS (2/2).

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/inbox/InspirationClusterBanner.tsx apps/desktop-shell/src/features/inbox/InspirationClusterBanner.test.tsx
git commit -m "feat(inbox): inspiration cluster banner component"
```

### Task E11.3: Wire banner into Inbox + accept handler creates inspiration page

**Files:**
- Modify: `apps/desktop-shell/src/features/inbox/InboxPage.tsx` (after empty-state branch, before grouped list)
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (add `crystallizeInspirationCluster` API call)
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (add POST `/api/wiki/inspiration/crystallize`)
- Modify: `rust/crates/wiki_store/src/lib.rs` (add `crystallize_inspiration_cluster` helper)
- Test: `rust/crates/wiki_store/src/lib.rs` test module — `crystallize_cluster_creates_inspiration_page_and_marks_entries`

**Step 1: Write the failing Rust test**

```rust
// In wiki_store/src/lib.rs (test module)
#[test]
fn crystallize_cluster_creates_inspiration_page_and_marks_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = init_wiki(tmp.path()).unwrap();
    let id1 = write_raw_entry(&paths, &fixture_raw("entry 1")).unwrap();
    let id2 = write_raw_entry(&paths, &fixture_raw("entry 2")).unwrap();
    let id3 = write_raw_entry(&paths, &fixture_raw("entry 3")).unwrap();

    let slug = crystallize_inspiration_cluster(
        &paths,
        "design-reference",
        &[id1, id2, id3],
    )
    .unwrap();

    let (summary, _body) = read_wiki_page(&paths, &slug).unwrap();
    assert_eq!(summary.category, "inspiration");
    assert_eq!(summary.source_refs.as_deref(), Some(&[
        format!("raw:{id1:05}"), format!("raw:{id2:05}"), format!("raw:{id3:05}"),
    ][..]));

    let raw = read_raw_entry(&paths, id1).unwrap();
    assert_eq!(raw.crystallized_into.as_deref(), Some(slug.as_str()));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p wiki_store crystallize_cluster_creates_inspiration_page_and_marks_entries`
Expected: FAIL with `cannot find function crystallize_inspiration_cluster`.

**Step 3: Write minimal implementation**

```rust
// In wiki_store/src/lib.rs (append near other inspiration helpers)
pub fn crystallize_inspiration_cluster(
    paths: &WikiPaths,
    domain: &str,
    entry_ids: &[u32],
) -> Result<String> {
    let slug = format!("inspiration-{}-{}", slugify(domain), now_compact_id());
    let source_refs: Vec<String> = entry_ids.iter().map(|id| format!("raw:{id:05}")).collect();
    let title = format!("{domain} 灵感页 (草稿)");
    let body = format!(
        "---\ntype: inspiration\nstatus: active\ntitle: {title}\nsummary: \
         {domain} 主题灵感聚合\npriority: medium\nvitality: seed\n\
         source_refs:\n{}\ncreated_at: {}\n---\n\n## Insight\n\n_Buddy 收集到的 {n} 条素材都标了「{domain}」用途。_\n\n\
         ## Evidence\n\n_请填写_\n",
        source_refs.iter().map(|r| format!("  - {r}")).collect::<Vec<_>>().join("\n"),
        chrono::Utc::now().to_rfc3339(),
        n = entry_ids.len(),
    );
    overwrite_wiki_page_content(paths, &slug, "inspiration", &body)?;
    for id in entry_ids {
        mark_raw_crystallized_into(paths, *id, &slug)?;
    }
    Ok(slug)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p wiki_store crystallize_cluster_creates_inspiration_page_and_marks_entries`
Expected: PASS.

**Step 5: Wire frontend (no separate commit yet)**

```typescript
// In repository.ts
export async function crystallizeInspirationCluster(
  domain: string,
  entryIds: number[],
): Promise<{ slug: string }> {
  return fetchJson("/api/wiki/inspiration/crystallize", {
    method: "POST",
    body: JSON.stringify({ domain, entry_ids: entryIds }),
  });
}

// In InboxPage.tsx (above grouped list)
const clusters = useMemo(
  () => detectInspirationClusters(pendingEntries),
  [pendingEntries],
);
const dismissedClusters = useInboxDismissedClustersStore();
const visibleCluster = clusters.find((c) => !dismissedClusters.has(c.domain));

const acceptCluster = useMutation({
  mutationFn: (ids: number[]) =>
    crystallizeInspirationCluster(currentDomain, ids),
  onSuccess: ({ slug }) => {
    queryClient.invalidateQueries({ queryKey: ["wiki"] });
    navigate(`/wiki/${slug}`);
  },
});

{visibleCluster && (
  <InspirationClusterBanner
    cluster={visibleCluster}
    onAccept={(ids, domain) => acceptCluster.mutate(ids)}
    onDismiss={() => dismissedClusters.dismiss(visibleCluster.domain)}
  />
)}
```

**Step 6: Run full slice verification**

Run: `cd apps/desktop-shell && npm run build && cargo test -p wiki_store`
Expected: build clean, all wiki_store tests pass.

**Step 7: Commit**

```bash
git add apps/desktop-shell/src/features/inbox/InboxPage.tsx apps/desktop-shell/src/api/wiki/repository.ts rust/crates/desktop-server/src/handlers/wiki_crud.rs rust/crates/wiki_store/src/lib.rs
git commit -m "feat(inspiration): wire cluster banner to crystallize endpoint"
```

---

## Slice E12 — Aesthetic Lens Onboarding Discoverability

**Tension:** Plan keeps default product language neutral; aesthetic / emotional lenses are opt-in. But aesthetic lenses are exactly what differentiates Buddy from Notion/Obsidian. Hidden behind opt-in, new users never find the differentiator.

**Resolution:** A one-time Home callout that fires only after the user has built up a small base (≥ 3 wiki pages OR ≥ 1 inspiration page). Offers "试一周" toggle for 3 sample lenses (色彩 / 情绪 / 反复出现). After 7 days, auto-asks 保留 / 关掉. Never appears again once dismissed or after the 7-day decision.

**Done means:**
- Callout appears on Home only when threshold met AND not dismissed AND no aesthetic lens enabled.
- Callout offers 3 sample lenses; toggle writes to `curation-preferences.yml`.
- 7-day timer triggers `保留 / 关掉` follow-up.
- All state persists across reloads via `namespacedStorage`.
- Callout never auto-shows after explicit dismissal.

### Task E12.1: Aesthetic lens onboarding store with phase machine

**Files:**
- Create: `apps/desktop-shell/src/state/aesthetic-lens-onboarding-store.ts`
- Test: `apps/desktop-shell/src/state/aesthetic-lens-onboarding-store.test.ts`

**Step 1: Write the failing test**

```typescript
import {
  useAestheticLensOnboardingStore,
  computeOnboardingPhase,
} from "./aesthetic-lens-onboarding-store";

describe("aesthetic lens onboarding phases", () => {
  it("returns 'hidden' when threshold not met", () => {
    expect(
      computeOnboardingPhase({
        wikiCount: 0,
        inspirationCount: 0,
        dismissedAt: null,
        startedAt: null,
        decisionAt: null,
        now: 1730000000000,
      }),
    ).toBe("hidden");
  });

  it("returns 'invite' when threshold met and never started", () => {
    expect(
      computeOnboardingPhase({
        wikiCount: 3,
        inspirationCount: 0,
        dismissedAt: null,
        startedAt: null,
        decisionAt: null,
        now: 1730000000000,
      }),
    ).toBe("invite");
  });

  it("returns 'in-trial' inside 7-day window", () => {
    const startedAt = 1730000000000;
    expect(
      computeOnboardingPhase({
        wikiCount: 3,
        inspirationCount: 0,
        dismissedAt: null,
        startedAt,
        decisionAt: null,
        now: startedAt + 3 * 24 * 60 * 60 * 1000,
      }),
    ).toBe("in-trial");
  });

  it("returns 'follow-up' after 7-day window", () => {
    const startedAt = 1730000000000;
    expect(
      computeOnboardingPhase({
        wikiCount: 3,
        inspirationCount: 0,
        dismissedAt: null,
        startedAt,
        decisionAt: null,
        now: startedAt + 8 * 24 * 60 * 60 * 1000,
      }),
    ).toBe("follow-up");
  });

  it("returns 'hidden' permanently after dismissal", () => {
    expect(
      computeOnboardingPhase({
        wikiCount: 999,
        inspirationCount: 999,
        dismissedAt: 1730000000000,
        startedAt: null,
        decisionAt: null,
        now: 1740000000000,
      }),
    ).toBe("hidden");
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/state/aesthetic-lens-onboarding-store.test.ts`
Expected: FAIL with `Cannot find module './aesthetic-lens-onboarding-store'`.

**Step 3: Write minimal implementation**

```typescript
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { namespacedStorage } from "./store-helpers";

const TRIAL_DURATION_MS = 7 * 24 * 60 * 60 * 1000;

export type OnboardingPhase = "hidden" | "invite" | "in-trial" | "follow-up";

export interface OnboardingComputeInput {
  wikiCount: number;
  inspirationCount: number;
  dismissedAt: number | null;
  startedAt: number | null;
  decisionAt: number | null;
  now: number;
}

export function computeOnboardingPhase(input: OnboardingComputeInput): OnboardingPhase {
  if (input.dismissedAt !== null) return "hidden";
  if (input.decisionAt !== null) return "hidden";
  const meetsThreshold = input.wikiCount >= 3 || input.inspirationCount >= 1;
  if (!meetsThreshold) return "hidden";
  if (input.startedAt === null) return "invite";
  return input.now - input.startedAt < TRIAL_DURATION_MS ? "in-trial" : "follow-up";
}

interface AestheticLensOnboardingStore {
  startedAt: number | null;
  dismissedAt: number | null;
  decisionAt: number | null;
  start: () => void;
  dismiss: () => void;
  decide: () => void;
}

export const useAestheticLensOnboardingStore = create<AestheticLensOnboardingStore>()(
  persist(
    (set) => ({
      startedAt: null,
      dismissedAt: null,
      decisionAt: null,
      start: () => set({ startedAt: Date.now() }),
      dismiss: () => set({ dismissedAt: Date.now() }),
      decide: () => set({ decisionAt: Date.now() }),
    }),
    {
      name: "state",
      storage: namespacedStorage("aesthetic-lens-onboarding"),
    },
  ),
);
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/state/aesthetic-lens-onboarding-store.test.ts`
Expected: PASS (5/5).

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/state/aesthetic-lens-onboarding-store.ts apps/desktop-shell/src/state/aesthetic-lens-onboarding-store.test.ts
git commit -m "feat(state): aesthetic-lens onboarding phase machine"
```

### Task E12.2: AestheticLensCallout component

**Files:**
- Create: `apps/desktop-shell/src/features/dashboard/AestheticLensCallout.tsx`
- Test: `apps/desktop-shell/src/features/dashboard/AestheticLensCallout.test.tsx`

**Step 1: Write the failing test**

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { AestheticLensCallout } from "./AestheticLensCallout";

describe("AestheticLensCallout", () => {
  it("renders 3 sample lenses in 'invite' phase", () => {
    render(<AestheticLensCallout phase="invite" onStart={() => {}} onDismiss={() => {}} onDecide={() => {}} />);
    expect(screen.getByText(/色彩/)).toBeInTheDocument();
    expect(screen.getByText(/情绪/)).toBeInTheDocument();
    expect(screen.getByText(/反复出现/)).toBeInTheDocument();
  });

  it("calls onStart when '试一周' clicked", () => {
    const onStart = vi.fn();
    render(<AestheticLensCallout phase="invite" onStart={onStart} onDismiss={() => {}} onDecide={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /试一周/ }));
    expect(onStart).toHaveBeenCalled();
  });

  it("shows '保留 / 关掉' in follow-up phase", () => {
    render(<AestheticLensCallout phase="follow-up" onStart={() => {}} onDismiss={() => {}} onDecide={() => {}} />);
    expect(screen.getByRole("button", { name: /保留/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /关掉/ })).toBeInTheDocument();
  });

  it("renders nothing when phase is 'hidden'", () => {
    const { container } = render(<AestheticLensCallout phase="hidden" onStart={() => {}} onDismiss={() => {}} onDecide={() => {}} />);
    expect(container).toBeEmptyDOMElement();
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/features/dashboard/AestheticLensCallout.test.tsx`
Expected: FAIL — module missing.

**Step 3: Write minimal implementation**

```tsx
import { Palette, X } from "lucide-react";
import type { OnboardingPhase } from "@/state/aesthetic-lens-onboarding-store";

const SAMPLE_LENSES = [
  { id: "color", label: "色彩" },
  { id: "mood", label: "情绪" },
  { id: "recurring", label: "反复出现" },
];

export interface AestheticLensCalloutProps {
  phase: OnboardingPhase;
  onStart: () => void;
  onDismiss: () => void;
  onDecide: (keep: boolean) => void;
}

export function AestheticLensCallout({ phase, onStart, onDismiss, onDecide }: AestheticLensCalloutProps) {
  if (phase === "hidden" || phase === "in-trial") return null;
  return (
    <div className="ds-aesthetic-callout flex items-start gap-3 rounded-md border border-border bg-accent/30 px-4 py-3">
      <Palette className="mt-0.5 size-4 text-primary" aria-hidden />
      <div className="flex-1 text-[12px] leading-5">
        <div className="font-medium">试一下美学 / 情感 lens</div>
        <p className="mt-1 text-muted-foreground">
          Buddy 默认是中性的；如果你的素材里有审美 / 情绪 / 反复出现的方向，开几条 lens 看一周再决定。
        </p>
        <div className="mt-2 flex flex-wrap gap-1">
          {SAMPLE_LENSES.map((lens) => (
            <span key={lens.id} className="rounded-full bg-muted px-2 py-0.5 text-[11px]">{lens.label}</span>
          ))}
        </div>
        <div className="mt-3 flex gap-2">
          {phase === "invite" && (
            <button type="button" className="rounded-md bg-primary px-3 py-1 text-[12px] text-primary-foreground" onClick={onStart}>
              试一周
            </button>
          )}
          {phase === "follow-up" && (
            <>
              <button type="button" className="rounded-md bg-primary px-3 py-1 text-[12px] text-primary-foreground" onClick={() => onDecide(true)}>
                保留
              </button>
              <button type="button" className="rounded-md border border-border px-3 py-1 text-[12px]" onClick={() => onDecide(false)}>
                关掉
              </button>
            </>
          )}
        </div>
      </div>
      <button type="button" className="rounded p-1 text-muted-foreground hover:bg-muted" onClick={onDismiss} aria-label="不再显示">
        <X className="size-3.5" />
      </button>
    </div>
  );
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/features/dashboard/AestheticLensCallout.test.tsx`
Expected: PASS (4/4).

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/AestheticLensCallout.tsx apps/desktop-shell/src/features/dashboard/AestheticLensCallout.test.tsx
git commit -m "feat(home): aesthetic lens onboarding callout component"
```

### Task E12.3: Wire callout into DashboardPage

**Files:**
- Modify: `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx` (between hero header and Top-3 actions)
- Modify: `rust/crates/wiki_store/templates/curation-preferences.yml` (add `aesthetic_lenses` section seed)
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (add `setAestheticLensesEnabled`)
- Test: covered by E12.1 + E12.2 (no new test)

**Step 1: Add `aesthetic_lenses` section to default curation-preferences template**

```yaml
# In rust/crates/wiki_store/templates/curation-preferences.yml — append:

aesthetic_lenses:
  enabled: []   # candidates: color, mood, recurring
  # Buddy will only apply lenses that appear here. Default: empty (neutral).
```

**Step 2: Add API helper**

```typescript
// In repository.ts
export async function setAestheticLensesEnabled(lenses: string[]): Promise<void> {
  await fetchJson("/api/wiki/rules/file", {
    method: "PUT",
    body: JSON.stringify({
      path: "schema/curation-preferences.yml",
      patch: { aesthetic_lenses: { enabled: lenses } },
    }),
  });
}
```

**Step 3: Wire callout into DashboardPage**

```tsx
// In DashboardPage.tsx — render callout above Top-3
const onboarding = useAestheticLensOnboardingStore();
const phase = computeOnboardingPhase({
  wikiCount: pagesQuery.data?.pages?.length ?? 0,
  inspirationCount:
    pagesQuery.data?.pages?.filter((p) => p.category === "inspiration").length ?? 0,
  dismissedAt: onboarding.dismissedAt,
  startedAt: onboarding.startedAt,
  decisionAt: onboarding.decisionAt,
  now: Date.now(),
});

<AestheticLensCallout
  phase={phase}
  onStart={async () => {
    onboarding.start();
    await setAestheticLensesEnabled(["color", "mood", "recurring"]);
  }}
  onDismiss={onboarding.dismiss}
  onDecide={async (keep) => {
    onboarding.decide();
    if (!keep) await setAestheticLensesEnabled([]);
  }}
/>
```

**Step 4: Run full slice verification**

Run: `cd apps/desktop-shell && npm run build`
Expected: build clean.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/DashboardPage.tsx rust/crates/wiki_store/templates/curation-preferences.yml apps/desktop-shell/src/api/wiki/repository.ts
git commit -m "feat(home): wire aesthetic lens onboarding callout into Pulse"
```

---

## Slice E13 — Cross-Domain Inference Quality Telemetry + Auto-Degrade

**Tension:** Cross-domain inference (E5) is rule-driven. ROI depends on hit rate. If accept rate drops below 50%, users will turn it off entirely. The plan has no feedback loop or degrade gate.

**Resolution:** Append-only feedback log of every accept / correct / ignore decision. A rolling 30-day accept_rate is computed per-source-domain. When per-domain accept_rate < 0.5 over ≥ 20 events, that branch's inferred_use_domain is set to `unknown` instead of pre-populating, so users see the inference is being held back. Per-domain metric surfaces in Connections > Source Readiness panel.

**Done means:**
- Every Inbox accept / correct / ignore writes a row to `.clawwiki/cross-domain-feedback.jsonl`.
- `computeAcceptRate(events, domain)` returns rolling 30-day rate.
- Inference auto-degrades to `unknown` when rate < 0.5 over ≥ 20 events.
- Connections page shows per-source-domain hit rate + degrade status.

### Task E13.1: JSONL feedback log helpers (Rust)

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (append helpers)
- Test: `rust/crates/wiki_store/src/lib.rs` test module

**Step 1: Write the failing test**

```rust
#[test]
fn cross_domain_feedback_appends_jsonl_and_reads_back() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = init_wiki(tmp.path()).unwrap();
    append_cross_domain_feedback(&paths, &CrossDomainFeedback {
        timestamp_ms: 1700000000000,
        decision: "accept".into(),
        source_domain: "shopping".into(),
        inferred_use_domain: "design-reference".into(),
        correction: None,
    }).unwrap();
    append_cross_domain_feedback(&paths, &CrossDomainFeedback {
        timestamp_ms: 1700000001000,
        decision: "correct".into(),
        source_domain: "shopping".into(),
        inferred_use_domain: "design-reference".into(),
        correction: Some("personal-archive".into()),
    }).unwrap();
    let events = read_cross_domain_feedback(&paths).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].decision, "accept");
    assert_eq!(events[1].correction.as_deref(), Some("personal-archive"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p wiki_store cross_domain_feedback_appends_jsonl_and_reads_back`
Expected: FAIL — `CrossDomainFeedback` not found.

**Step 3: Write minimal implementation**

```rust
// In wiki_store/src/lib.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossDomainFeedback {
    pub timestamp_ms: i64,
    pub decision: String,            // "accept" | "correct" | "ignore"
    pub source_domain: String,
    pub inferred_use_domain: String,
    pub correction: Option<String>,
}

const CROSS_DOMAIN_FEEDBACK_FILENAME: &str = "cross-domain-feedback.jsonl";

pub fn append_cross_domain_feedback(paths: &WikiPaths, event: &CrossDomainFeedback) -> Result<()> {
    let path = paths.clawwiki_dir().join(CROSS_DOMAIN_FEEDBACK_FILENAME);
    let mut line = serde_json::to_string(event).map_err(WikiStoreError::serde)?;
    line.push('\n');
    let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)
        .map_err(|e| WikiStoreError::io(path.clone(), e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(())
}

pub fn read_cross_domain_feedback(paths: &WikiPaths) -> Result<Vec<CrossDomainFeedback>> {
    let path = paths.clawwiki_dir().join(CROSS_DOMAIN_FEEDBACK_FILENAME);
    if !path.exists() { return Ok(Vec::new()); }
    let content = fs::read_to_string(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let mut out = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() { continue; }
        out.push(serde_json::from_str(line).map_err(WikiStoreError::serde)?);
    }
    Ok(out)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p wiki_store cross_domain_feedback_appends_jsonl_and_reads_back`
Expected: PASS.

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs
git commit -m "feat(wiki_store): cross-domain feedback JSONL log helpers"
```

### Task E13.2: POST `/api/wiki/cross-domain/feedback` endpoint

**Files:**
- Create: `rust/crates/desktop-server/src/handlers/cross_domain_feedback.rs`
- Modify: `rust/crates/desktop-server/src/handlers/mod.rs` (register module)
- Modify: `rust/crates/desktop-server/src/lib.rs` (add route)
- Test: handler-level test inside `cross_domain_feedback.rs`

**Step 1: Write the failing test**

```rust
// In cross_domain_feedback.rs
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn post_feedback_appends_to_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(tmp.path().to_path_buf());
        let body = serde_json::json!({
            "decision": "correct",
            "source_domain": "shopping",
            "inferred_use_domain": "design-reference",
            "correction": "personal-archive"
        });
        let resp = app
            .oneshot(Request::builder()
                .method("POST")
                .uri("/api/wiki/cross-domain/feedback")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())).unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), 200);
        let events = wiki_store::read_cross_domain_feedback(&wiki_store::resolve_paths(tmp.path()).unwrap()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].correction.as_deref(), Some("personal-archive"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p desktop-server post_feedback_appends_to_jsonl`
Expected: FAIL — module / route missing.

**Step 3: Write minimal implementation**

```rust
// cross_domain_feedback.rs
use axum::{Json, http::StatusCode, response::IntoResponse, extract::State};
use serde::Deserialize;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct FeedbackBody {
    pub decision: String,
    pub source_domain: String,
    pub inferred_use_domain: String,
    pub correction: Option<String>,
}

pub async fn post_cross_domain_feedback(
    State(state): State<AppState>,
    Json(body): Json<FeedbackBody>,
) -> impl IntoResponse {
    let event = wiki_store::CrossDomainFeedback {
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        decision: body.decision,
        source_domain: body.source_domain,
        inferred_use_domain: body.inferred_use_domain,
        correction: body.correction,
    };
    match wiki_store::append_cross_domain_feedback(&state.paths, &event) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

```rust
// In lib.rs router builder, add:
.route("/api/wiki/cross-domain/feedback",
       axum::routing::post(handlers::cross_domain_feedback::post_cross_domain_feedback))
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p desktop-server post_feedback_appends_to_jsonl`
Expected: PASS.

**Step 5: Commit**

```bash
git add rust/crates/desktop-server/src/handlers/cross_domain_feedback.rs rust/crates/desktop-server/src/handlers/mod.rs rust/crates/desktop-server/src/lib.rs
git commit -m "feat(api): POST /api/wiki/cross-domain/feedback"
```

### Task E13.3: computeAcceptRate + auto-degrade gate

**Files:**
- Modify: `apps/desktop-shell/src/features/cross-domain/cross-domain.ts` (add `computeAcceptRate`, `shouldDegradeInference`)
- Test: `apps/desktop-shell/src/features/cross-domain/cross-domain.test.ts`

**Step 1: Write the failing test**

```typescript
import { computeAcceptRate, shouldDegradeInference } from "./cross-domain";

describe("cross-domain accept rate + degrade gate", () => {
  it("returns 1.0 when all events accept", () => {
    const events = [
      { decision: "accept", source_domain: "shopping", timestamp_ms: 1 },
      { decision: "accept", source_domain: "shopping", timestamp_ms: 2 },
    ];
    expect(computeAcceptRate(events, "shopping", 30, 1000)).toBe(1);
  });

  it("returns 0.5 with mixed events", () => {
    const events = [
      { decision: "accept", source_domain: "shopping", timestamp_ms: 1 },
      { decision: "correct", source_domain: "shopping", timestamp_ms: 2 },
    ];
    expect(computeAcceptRate(events, "shopping", 30, 1000)).toBe(0.5);
  });

  it("ignores events outside the 30-day window", () => {
    const now = 1700000000000;
    const events = [
      { decision: "accept", source_domain: "shopping", timestamp_ms: now - 31 * 86400000 },
      { decision: "correct", source_domain: "shopping", timestamp_ms: now - 1000 },
    ];
    expect(computeAcceptRate(events, "shopping", 30, now)).toBe(0);
  });

  it("does not degrade with insufficient sample size (< 20 events)", () => {
    const events = Array.from({ length: 19 }, (_, i) => ({
      decision: "ignore" as const,
      source_domain: "shopping",
      timestamp_ms: 1700000000000 - i * 1000,
    }));
    expect(shouldDegradeInference(events, "shopping", 1700000000000)).toBe(false);
  });

  it("degrades when ≥ 20 events and accept_rate < 0.5", () => {
    const events = Array.from({ length: 25 }, (_, i) => ({
      decision: i < 10 ? "accept" : "correct",
      source_domain: "shopping",
      timestamp_ms: 1700000000000 - i * 1000,
    }));
    expect(shouldDegradeInference(events, "shopping", 1700000000000)).toBe(true);
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/features/cross-domain/cross-domain.test.ts -t "accept rate"`
Expected: FAIL — functions missing.

**Step 3: Write minimal implementation**

```typescript
export interface FeedbackEvent {
  decision: "accept" | "correct" | "ignore";
  source_domain: string;
  timestamp_ms: number;
}

const DEGRADE_MIN_SAMPLE = 20;
const DEGRADE_RATE_THRESHOLD = 0.5;

export function computeAcceptRate(
  events: ReadonlyArray<FeedbackEvent>,
  sourceDomain: string,
  windowDays: number,
  now: number,
): number {
  const cutoff = now - windowDays * 24 * 60 * 60 * 1000;
  const filtered = events.filter(
    (e) => e.source_domain === sourceDomain && e.timestamp_ms >= cutoff,
  );
  if (filtered.length === 0) return 0;
  const accepts = filtered.filter((e) => e.decision === "accept").length;
  return accepts / filtered.length;
}

export function shouldDegradeInference(
  events: ReadonlyArray<FeedbackEvent>,
  sourceDomain: string,
  now: number,
): boolean {
  const sampleEvents = events.filter(
    (e) => e.source_domain === sourceDomain && e.timestamp_ms >= now - 30 * 86400000,
  );
  if (sampleEvents.length < DEGRADE_MIN_SAMPLE) return false;
  return computeAcceptRate(events, sourceDomain, 30, now) < DEGRADE_RATE_THRESHOLD;
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/features/cross-domain/cross-domain.test.ts -t "accept rate"`
Expected: PASS (5/5).

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/cross-domain/cross-domain.ts apps/desktop-shell/src/features/cross-domain/cross-domain.test.ts
git commit -m "feat(cross-domain): rolling accept_rate + degrade gate"
```

### Task E13.4: Wire telemetry into Inbox + render rate in Connections

**Files:**
- Modify: `apps/desktop-shell/src/features/inbox/InboxPage.tsx` (call `postCrossDomainFeedback` on accept/correct/ignore)
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (add `postCrossDomainFeedback`, `listCrossDomainFeedback`)
- Modify: `apps/desktop-shell/src/features/connections/ConnectionsPage.tsx` (add per-domain hit-rate panel under 来源接入门槛)
- Test: covered by E13.3 unit tests

**Step 1: Add API helpers**

```typescript
export async function postCrossDomainFeedback(event: {
  decision: "accept" | "correct" | "ignore";
  source_domain: string;
  inferred_use_domain: string;
  correction?: string;
}): Promise<void> {
  await fetchJson("/api/wiki/cross-domain/feedback", { method: "POST", body: JSON.stringify(event) });
}

export async function listCrossDomainFeedback(): Promise<FeedbackEvent[]> {
  return fetchJson("/api/wiki/cross-domain/feedback");
}
```

**Step 2: Wire fire-and-forget telemetry into Inbox row handlers**

```typescript
// In InboxPage.tsx — wrap acceptCrossDomain / correctCrossDomain / ignoreCrossDomain
function trackFeedback(decision: "accept" | "correct" | "ignore", entry: InboxEntry, correction?: string) {
  if (!entry.source_domain || !entry.inferred_use_domain) return;
  postCrossDomainFeedback({
    decision,
    source_domain: entry.source_domain,
    inferred_use_domain: entry.inferred_use_domain,
    correction,
  }).catch(() => { /* telemetry is best-effort */ });
}
```

**Step 3: Render per-domain rate in Connections**

```tsx
// In ConnectionsPage.tsx — under the 来源接入门槛 section
const feedbackQuery = useQuery({
  queryKey: ["wiki", "cross-domain", "feedback"],
  queryFn: () => listCrossDomainFeedback(),
  staleTime: 30_000,
});

const domains = ["shopping", "music", "article", "image"];
const now = Date.now();

<div className="rounded-md border border-border bg-card px-4 py-4">
  <h3 className="text-[13px] font-medium">跨界推断质量 (近 30 天)</h3>
  <p className="mt-1 text-[11px] text-muted-foreground">
    accept_rate &lt; 50% 且样本 ≥ 20 时，对应来源会自动暂停推断。
  </p>
  <table className="mt-2 w-full text-[12px]">
    {domains.map((d) => {
      const rate = computeAcceptRate(feedbackQuery.data ?? [], d, 30, now);
      const degraded = shouldDegradeInference(feedbackQuery.data ?? [], d, now);
      return (
        <tr key={d}>
          <td>{d}</td>
          <td>{(rate * 100).toFixed(0)}%</td>
          <td>{degraded ? "已暂停" : "活跃"}</td>
        </tr>
      );
    })}
  </table>
</div>
```

**Step 4: Run full slice verification**

Run: `cd apps/desktop-shell && npm run build && cargo test -p wiki_store -p desktop-server`
Expected: build clean, all tests pass.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/inbox/InboxPage.tsx apps/desktop-shell/src/api/wiki/repository.ts apps/desktop-shell/src/features/connections/ConnectionsPage.tsx
git commit -m "feat(connections): cross-domain accept-rate panel + telemetry wiring"
```

---

## Slice E14 — Patrol Cooling Threshold Calibration

**Tension:** Cooling window is a magic number in `curation-preferences.yml`. Too short → cooling group floods → users lose trust. Too long → cooling never fires → category becomes useless.

**Resolution:** Empirical calibration from the existing E9 patrol audit log: compute median time-since-last-revisit for pages user actually accepted as cooling. Suggest `cooling_window_days = max(median, 14)` as a one-click apply card in Rules Studio. YAML edit always wins.

**Done means:**
- `compute_cooling_calibration()` reads patrol audit + accepted Inbox decisions.
- Returns `{median_days, sample_size, recommended_window}` or `InsufficientData`.
- Rules Studio renders a calibration card + 一键应用 button.
- User-edited YAML is never overwritten silently.

### Task E14.1: compute_cooling_calibration in wiki_patrol

**Files:**
- Modify: `rust/crates/wiki_patrol/src/lib.rs`
- Test: `rust/crates/wiki_patrol/src/lib.rs` test module

**Step 1: Write the failing test**

```rust
#[test]
fn cooling_calibration_returns_insufficient_data_below_5_events() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = wiki_store::init_wiki(tmp.path()).unwrap();
    let calib = compute_cooling_calibration(&paths).unwrap();
    assert!(matches!(calib, CoolingCalibration::InsufficientData { sample_size: 0 }));
}

#[test]
fn cooling_calibration_returns_median_with_sufficient_events() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = wiki_store::init_wiki(tmp.path()).unwrap();
    for days_ago in [10, 20, 30, 40, 50] {
        record_archive_event_with_age(&paths, days_ago);
    }
    let calib = compute_cooling_calibration(&paths).unwrap();
    if let CoolingCalibration::Recommended { median_days, recommended_window, sample_size } = calib {
        assert_eq!(median_days, 30);
        assert_eq!(recommended_window, 30);
        assert_eq!(sample_size, 5);
    } else { panic!("expected Recommended"); }
}

#[test]
fn cooling_calibration_clamps_recommended_to_floor_of_14() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = wiki_store::init_wiki(tmp.path()).unwrap();
    for days_ago in [3, 5, 7, 9, 10] {
        record_archive_event_with_age(&paths, days_ago);
    }
    if let CoolingCalibration::Recommended { recommended_window, .. } =
        compute_cooling_calibration(&paths).unwrap() {
        assert_eq!(recommended_window, 14, "floor at 14 even when median is lower");
    } else { panic!("expected Recommended"); }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p wiki_patrol cooling_calibration`
Expected: FAIL — function/types missing.

**Step 3: Write minimal implementation**

```rust
#[derive(Debug, Serialize, Deserialize)]
pub enum CoolingCalibration {
    InsufficientData { sample_size: usize },
    Recommended { median_days: u32, recommended_window: u32, sample_size: usize },
}

const COOLING_FLOOR_DAYS: u32 = 14;
const COOLING_MIN_SAMPLE: usize = 5;

pub fn compute_cooling_calibration(paths: &wiki_store::WikiPaths) -> Result<CoolingCalibration> {
    let events = wiki_store::read_archive_events(paths)?;
    if events.len() < COOLING_MIN_SAMPLE {
        return Ok(CoolingCalibration::InsufficientData { sample_size: events.len() });
    }
    let mut ages_days: Vec<u32> = events.iter().map(|e| e.age_days).collect();
    ages_days.sort_unstable();
    let median = ages_days[ages_days.len() / 2];
    let recommended = median.max(COOLING_FLOOR_DAYS);
    Ok(CoolingCalibration::Recommended {
        median_days: median,
        recommended_window: recommended,
        sample_size: ages_days.len(),
    })
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p wiki_patrol cooling_calibration`
Expected: PASS (3/3).

**Step 5: Commit**

```bash
git add rust/crates/wiki_patrol/src/lib.rs rust/crates/wiki_store/src/lib.rs
git commit -m "feat(patrol): cooling calibration from archive events"
```

### Task E14.2: GET `/api/wiki/patrol/cooling-calibration` endpoint

**Files:**
- Create: `rust/crates/desktop-server/src/handlers/cooling_calibration.rs`
- Modify: `rust/crates/desktop-server/src/lib.rs` (add route)
- Test: handler test inside same file

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn cooling_calibration_endpoint_returns_recommended_window() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = wiki_store::init_wiki(tmp.path()).unwrap();
    for days_ago in [10, 20, 30, 40, 50] {
        wiki_store::record_archive_event_with_age(&paths, days_ago);
    }
    let app = build_router(tmp.path().to_path_buf());
    let resp = app.oneshot(Request::builder()
        .uri("/api/wiki/patrol/cooling-calibration")
        .body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&resp.collect_body().await).unwrap();
    assert_eq!(body["recommended_window"], 30);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p desktop-server cooling_calibration_endpoint_returns_recommended_window`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
pub async fn get_cooling_calibration(State(state): State<AppState>) -> impl IntoResponse {
    match wiki_patrol::compute_cooling_calibration(&state.paths) {
        Ok(c) => Json(c).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

```rust
// router
.route("/api/wiki/patrol/cooling-calibration", get(handlers::cooling_calibration::get_cooling_calibration))
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p desktop-server cooling_calibration_endpoint_returns_recommended_window`
Expected: PASS.

**Step 5: Commit**

```bash
git add rust/crates/desktop-server/src/handlers/cooling_calibration.rs rust/crates/desktop-server/src/lib.rs
git commit -m "feat(api): GET /api/wiki/patrol/cooling-calibration"
```

### Task E14.3: Schema Editor calibration card

**Files:**
- Modify: `apps/desktop-shell/src/features/schema/SchemaEditorPage.tsx` (insert card above Validation snapshot)
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (add `getCoolingCalibration`)
- Test: end-to-end via build

**Step 1: Add API call**

```typescript
export async function getCoolingCalibration(): Promise<
  | { kind: "InsufficientData"; sample_size: number }
  | { kind: "Recommended"; median_days: number; recommended_window: number; sample_size: number }
> {
  return fetchJson("/api/wiki/patrol/cooling-calibration");
}
```

**Step 2: Render calibration card**

```tsx
const calib = useQuery({
  queryKey: ["wiki", "patrol", "cooling-calibration"],
  queryFn: getCoolingCalibration,
  staleTime: 60_000,
});

{calib.data?.kind === "Recommended" && (
  <div className="rounded-md border border-border bg-card px-4 py-3">
    <h3 className="text-[13px] font-medium">冷却阈值校准</h3>
    <p className="mt-1 text-[11px] text-muted-foreground">
      你已归档 {calib.data.sample_size} 页，中位停留 {calib.data.median_days} 天。
      建议 cooling_window_days = <span className="font-medium">{calib.data.recommended_window}</span>。
    </p>
    <button
      type="button"
      className="mt-2 rounded-md bg-primary px-3 py-1 text-[12px] text-primary-foreground"
      onClick={() => applyCoolingWindow(calib.data.recommended_window)}
    >
      一键应用
    </button>
  </div>
)}
```

**Step 3: Wire 一键应用 to PUT `/api/wiki/rules/file`**

```typescript
async function applyCoolingWindow(days: number) {
  await fetchJson("/api/wiki/rules/file", {
    method: "PUT",
    body: JSON.stringify({
      path: "schema/curation-preferences.yml",
      patch: { cooling_window_days: days },
    }),
  });
  queryClient.invalidateQueries({ queryKey: ["wiki", "patrol"] });
}
```

**Step 4: Run full slice verification**

Run: `cd apps/desktop-shell && npm run build && cargo test -p wiki_patrol -p desktop-server`
Expected: build clean, all tests pass.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/schema/SchemaEditorPage.tsx apps/desktop-shell/src/api/wiki/repository.ts
git commit -m "feat(rules): cooling threshold calibration card"
```

---

## Slice E15 — Re-Emergence (Auto-Resurface)

**Tension:** Once a wiki page hits `vitality: cooling | archived`, there is no path back to active without manual user edit. Knowledge cycles in real life — an archived "API design notes" page becomes relevant again when a new API project starts. Buddy needs a re-emergence signal.

**Resolution:** Two triggers:
1. **Absorb match** — when maintainer's `update_existing` flow matches a cooling/archived page as merge target, do not auto-merge; instead create an Inbox `Resurface` review task.
2. **Ask citation** — when a cooling/archived page is cited in 3+ Ask answers within 7 days, propose Resurface.

User accept flips `vitality` back to `growing`, updates `last_revisited_at`. User reject keeps page cooling but logs the reason ("explicitly kept cooling on 2026-05-08 — too narrow").

**Done means:**
- New `InboxKind::Resurface` variant routes through existing accept/reject pipeline.
- Maintainer absorb pipeline detects cooling target and routes to Resurface instead of silent update.
- Frontend Ask citation tracker debounces over 7-day window.
- Rejected Resurface creates `priority_reason` annotation on the page.

### Task E15.1: InboxKind::Resurface variant + routing

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (extend `InboxKind` enum)
- Modify: `apps/desktop-shell/src/api/wiki/types.ts` (extend `InboxKind` union)
- Test: `rust/crates/wiki_store/src/lib.rs` test module

**Step 1: Write the failing test**

```rust
#[test]
fn inbox_kind_resurface_round_trips_through_serde() {
    let kind = InboxKind::Resurface { target_slug: "api-design".to_string(), reason: "matched by raw 12345".to_string() };
    let json = serde_json::to_string(&kind).unwrap();
    let parsed: InboxKind = serde_json::from_str(&json).unwrap();
    assert!(matches!(parsed, InboxKind::Resurface { .. }));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p wiki_store inbox_kind_resurface_round_trips_through_serde`
Expected: FAIL — variant missing.

**Step 3: Write minimal implementation**

```rust
// In wiki_store/src/lib.rs InboxKind enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InboxKind {
    // ... existing variants ...
    Resurface { target_slug: String, reason: String },
}
```

```typescript
// In types.ts
export type InboxKind =
  | { kind: "Stale"; target_slug?: string }
  | { kind: "Deprecate"; target_slug?: string }
  | { kind: "NewRaw"; raw_id: number }
  | { kind: "Resurface"; target_slug: string; reason: string };
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p wiki_store inbox_kind_resurface_round_trips_through_serde`
Expected: PASS.

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs apps/desktop-shell/src/api/wiki/types.ts
git commit -m "feat(wiki_store): InboxKind::Resurface variant"
```

### Task E15.2: Detect cooling-match in absorb pipeline

**Files:**
- Modify: `rust/crates/wiki_maintainer/src/lib.rs` (in absorb_batch step 3c, before update_existing)
- Test: `rust/crates/wiki_maintainer/src/lib.rs` test module

**Step 1: Write the failing test**

```rust
#[test]
fn absorb_routes_cooling_match_to_resurface_instead_of_update() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = wiki_store::init_wiki(tmp.path()).unwrap();
    overwrite_wiki_page_content(&paths, "api-design", "concept",
        "---\ntype: concept\nvitality: cooling\n---\n\nold body").unwrap();
    let raw_id = write_raw_entry(&paths, &fixture_raw_about("api design")).unwrap();

    // Simulate maintainer matching api-design as merge target
    let proposal = WikiPageProposal { slug: "api-design".to_string(), action: "update_existing".to_string(), .. };
    let outcome = route_absorb_proposal(&paths, &proposal, raw_id).unwrap();

    assert!(matches!(outcome, AbsorbOutcome::ResurfaceQueued { .. }));
    let inbox = list_inbox(&paths).unwrap();
    assert!(inbox.iter().any(|t| matches!(t.kind, InboxKind::Resurface { .. })));
    let (summary, body) = read_wiki_page(&paths, "api-design").unwrap();
    assert_eq!(body, "old body", "page must NOT be updated until user accepts");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p wiki_maintainer absorb_routes_cooling_match_to_resurface_instead_of_update`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
pub enum AbsorbOutcome { Updated(String), ResurfaceQueued { slug: String, raw_id: u32 }, Created(String) }

pub fn route_absorb_proposal(paths: &WikiPaths, proposal: &WikiPageProposal, raw_id: u32) -> Result<AbsorbOutcome> {
    if proposal.action == "update_existing" {
        let (existing, _) = read_wiki_page(paths, &proposal.slug)?;
        if matches!(existing.vitality.as_deref(), Some("cooling") | Some("archived")) {
            create_inbox_task(paths, InboxKind::Resurface {
                target_slug: proposal.slug.clone(),
                reason: format!("matched by raw:{raw_id:05}"),
            })?;
            return Ok(AbsorbOutcome::ResurfaceQueued {
                slug: proposal.slug.clone(),
                raw_id,
            });
        }
    }
    apply_absorb_proposal(paths, proposal, raw_id)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p wiki_maintainer absorb_routes_cooling_match_to_resurface_instead_of_update`
Expected: PASS.

**Step 5: Commit**

```bash
git add rust/crates/wiki_maintainer/src/lib.rs
git commit -m "feat(maintainer): route cooling matches to Resurface inbox task"
```

### Task E15.3: Resurface accept handler flips vitality

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (in `apply_inbox_proposal`)
- Test: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` test module

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn accept_resurface_flips_vitality_to_growing_and_logs_revisit() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = wiki_store::init_wiki(tmp.path()).unwrap();
    overwrite_wiki_page_content(&paths, "api-design", "concept",
        "---\ntype: concept\nvitality: cooling\n---\n\nbody").unwrap();
    let task_id = create_inbox_task(&paths, InboxKind::Resurface {
        target_slug: "api-design".to_string(),
        reason: "matched by raw:00001".to_string(),
    }).unwrap();

    let app = build_router(tmp.path().to_path_buf());
    let resp = app.oneshot(Request::builder()
        .method("POST")
        .uri(format!("/api/wiki/inbox/{task_id}/proposal/apply"))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"accept": true}"#)).unwrap()).await.unwrap();
    assert_eq!(resp.status(), 200);

    let (summary, _) = read_wiki_page(&paths, "api-design").unwrap();
    assert_eq!(summary.vitality.as_deref(), Some("growing"));
    assert!(summary.last_revisited_at.is_some());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p desktop-server accept_resurface_flips_vitality_to_growing_and_logs_revisit`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
// In wiki_crud.rs apply_inbox_proposal — add new branch:
InboxKind::Resurface { target_slug, .. } if accept => {
    let now = chrono::Utc::now().to_rfc3339();
    update_page_frontmatter(paths, &target_slug, |fm| {
        fm.insert("vitality".into(), "growing".into());
        fm.insert("last_revisited_at".into(), now.clone());
    })?;
}
InboxKind::Resurface { target_slug, .. } if !accept => {
    let now_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    update_page_frontmatter(paths, &target_slug, |fm| {
        let prev = fm.get("priority_reason").cloned().unwrap_or_default();
        let suffix = format!("\n[{now_date}] explicitly kept cooling");
        fm.insert("priority_reason".into(), format!("{prev}{suffix}"));
    })?;
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p desktop-server accept_resurface_flips_vitality_to_growing_and_logs_revisit`
Expected: PASS.

**Step 5: Commit**

```bash
git add rust/crates/desktop-server/src/handlers/wiki_crud.rs
git commit -m "feat(api): Resurface accept flips vitality, reject logs reason"
```

### Task E15.4: Ask citation 7-day tracker

**Files:**
- Modify: `apps/desktop-shell/src/features/ask/UsedSourcesBar.tsx` (record citation)
- Create: `apps/desktop-shell/src/features/ask/citation-tracker.ts`
- Test: `apps/desktop-shell/src/features/ask/citation-tracker.test.ts`

**Step 1: Write the failing test**

```typescript
import { CitationTracker } from "./citation-tracker";

describe("CitationTracker", () => {
  it("emits resurface signal when 3+ citations of cooling page within 7 days", () => {
    const tracker = new CitationTracker(() => ({ "api-design": { vitality: "cooling" } }));
    const signals: string[] = [];
    tracker.onResurfaceCandidate = (slug) => signals.push(slug);
    const now = 1700000000000;
    tracker.recordCitation("api-design", now);
    tracker.recordCitation("api-design", now + 1000);
    tracker.recordCitation("api-design", now + 2000);
    expect(signals).toEqual(["api-design"]);
  });

  it("does not emit for active page", () => {
    const tracker = new CitationTracker(() => ({ "api-design": { vitality: "growing" } }));
    const signals: string[] = [];
    tracker.onResurfaceCandidate = (slug) => signals.push(slug);
    tracker.recordCitation("api-design", 1700000000000);
    tracker.recordCitation("api-design", 1700000001000);
    tracker.recordCitation("api-design", 1700000002000);
    expect(signals).toEqual([]);
  });

  it("decays citations older than 7 days", () => {
    const tracker = new CitationTracker(() => ({ "api-design": { vitality: "cooling" } }));
    const signals: string[] = [];
    tracker.onResurfaceCandidate = (slug) => signals.push(slug);
    const now = 1700000000000;
    tracker.recordCitation("api-design", now - 8 * 86400000);
    tracker.recordCitation("api-design", now);
    tracker.recordCitation("api-design", now + 1000);
    expect(signals).toEqual([]);
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/features/ask/citation-tracker.test.ts`
Expected: FAIL — module missing.

**Step 3: Write minimal implementation**

```typescript
const WINDOW_MS = 7 * 24 * 60 * 60 * 1000;
const THRESHOLD = 3;

export class CitationTracker {
  private events = new Map<string, number[]>();
  public onResurfaceCandidate?: (slug: string) => void;
  constructor(private getPageMeta: () => Record<string, { vitality?: string }>) {}

  recordCitation(slug: string, now: number) {
    const list = this.events.get(slug) ?? [];
    list.push(now);
    const fresh = list.filter((t) => now - t <= WINDOW_MS);
    this.events.set(slug, fresh);
    if (fresh.length >= THRESHOLD) {
      const meta = this.getPageMeta()[slug];
      if (meta?.vitality === "cooling" || meta?.vitality === "archived") {
        this.onResurfaceCandidate?.(slug);
      }
    }
  }
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/features/ask/citation-tracker.test.ts`
Expected: PASS (3/3).

**Step 5: Wire into UsedSourcesBar**

```tsx
// In UsedSourcesBar.tsx — when rendering each citation chip:
useEffect(() => {
  citations.forEach((c) => citationTracker.recordCitation(c.slug, Date.now()));
}, [citations]);
```

**Step 6: Run full slice verification**

Run: `cd apps/desktop-shell && npm run build && cargo test -p wiki_maintainer -p desktop-server -p wiki_store`
Expected: build clean, all tests pass.

**Step 7: Commit**

```bash
git add apps/desktop-shell/src/features/ask/UsedSourcesBar.tsx apps/desktop-shell/src/features/ask/citation-tracker.ts apps/desktop-shell/src/features/ask/citation-tracker.test.ts
git commit -m "feat(ask): citation tracker emits Resurface signal"
```

---

## Slice E16 — Synthesis-To-Page (`/synthesize-brief-as-wiki`)

**Tension:** `/theme-brief` (E7) produces a useful synthesis answer in chat, but produces no persistent artifact. User has to manually copy → create wiki page. The cycle "raw → reflect → page" is broken at the last step.

**Resolution:** Add `/synthesize-brief-as-wiki` slash command. Composer behavior: after the brief answer renders, append a `保存为 wiki 页` button. Click → POST creates a `type: inspiration` page populated from the answer with `source_refs` matching cited entries → auto-navigate to `/wiki/<slug>`. Permission stays explicit (user click), no auto-write.

**Done means:**
- New slash command appears in palette + reflection prompt list.
- Brief answer renders with `保存为 wiki 页` button.
- Save creates `type: inspiration` page with `source_refs` from citations.
- Navigation to new slug works.

### Task E16.1: Add synthesize-brief reflection prompt

**Files:**
- Modify: `apps/desktop-shell/src/features/ask/ask-reflection-prompts.ts` (append entry)
- Test: `apps/desktop-shell/src/features/ask/ask-reflection-prompts.test.ts`

**Step 1: Write the failing test**

```typescript
import { ASK_REFLECTION_PROMPTS } from "./ask-reflection-prompts";

it("includes /synthesize-brief-as-wiki", () => {
  const prompt = ASK_REFLECTION_PROMPTS.find((p) => p.slashName === "/synthesize-brief-as-wiki");
  expect(prompt).toBeDefined();
  expect(prompt?.savesArtifact).toBe(true);
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/features/ask/ask-reflection-prompts.test.ts -t "synthesize-brief"`
Expected: FAIL.

**Step 3: Write minimal implementation**

```typescript
// In ask-reflection-prompts.ts append:
{
  id: "synthesize_brief_wiki",
  slashName: "/synthesize-brief-as-wiki",
  description: "把当前对话或选中的素材合成一个简报，并保存为灵感 wiki 页",
  template: "把这组素材合成一个 brief：核心 insight、证据、反复出现的元素、用例、归档候选、下一步。最后只回 JSON: { title, insight, evidence, recurring, use_cases, archive_candidates, next_actions }。",
  savesArtifact: true,
},
```

```typescript
// Extend AskReflectionPrompt interface:
export interface AskReflectionPrompt {
  id: AskReflectionPromptId;
  slashName: string;
  description: string;
  template: string;
  savesArtifact?: boolean;
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/features/ask/ask-reflection-prompts.test.ts -t "synthesize-brief"`
Expected: PASS.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/ask/ask-reflection-prompts.ts apps/desktop-shell/src/features/ask/ask-reflection-prompts.test.ts
git commit -m "feat(ask): add /synthesize-brief-as-wiki reflection prompt"
```

### Task E16.2: SaveAsWikiButton component

**Files:**
- Create: `apps/desktop-shell/src/features/ask/SaveAsWikiButton.tsx`
- Test: `apps/desktop-shell/src/features/ask/SaveAsWikiButton.test.tsx`

**Step 1: Write the failing test**

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { SaveAsWikiButton } from "./SaveAsWikiButton";

it("renders when answer is JSON-shaped synthesis brief", () => {
  const answer = JSON.stringify({ title: "Moodboard 1", insight: "...", evidence: [], recurring: [], use_cases: [], archive_candidates: [], next_actions: [] });
  render(<SaveAsWikiButton rawAnswer={answer} citationSlugs={["raw:00001"]} onSaved={() => {}} />);
  expect(screen.getByRole("button", { name: /保存为 wiki 页/ })).toBeInTheDocument();
});

it("does not render for non-synthesis answers", () => {
  const { container } = render(<SaveAsWikiButton rawAnswer="hello" citationSlugs={[]} onSaved={() => {}} />);
  expect(container).toBeEmptyDOMElement();
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx vitest run src/features/ask/SaveAsWikiButton.test.tsx`
Expected: FAIL.

**Step 3: Write minimal implementation**

```tsx
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { saveSynthesisAsInspirationPage } from "@/api/wiki/repository";

export interface SaveAsWikiButtonProps {
  rawAnswer: string;
  citationSlugs: string[];
  onSaved: (slug: string) => void;
}

export function SaveAsWikiButton({ rawAnswer, citationSlugs, onSaved }: SaveAsWikiButtonProps) {
  const navigate = useNavigate();
  const [saving, setSaving] = useState(false);
  let parsed: any;
  try { parsed = JSON.parse(rawAnswer); } catch { return null; }
  if (!parsed?.title || !parsed?.insight) return null;

  return (
    <button
      type="button"
      disabled={saving}
      onClick={async () => {
        setSaving(true);
        try {
          const { slug } = await saveSynthesisAsInspirationPage({ brief: parsed, source_refs: citationSlugs });
          onSaved(slug);
          navigate(`/wiki/${slug}`);
        } finally { setSaving(false); }
      }}
      className="inline-flex h-8 items-center gap-2 rounded-md bg-primary px-3 text-[12px] text-primary-foreground"
    >
      保存为 wiki 页
    </button>
  );
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx vitest run src/features/ask/SaveAsWikiButton.test.tsx`
Expected: PASS (2/2).

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/ask/SaveAsWikiButton.tsx apps/desktop-shell/src/features/ask/SaveAsWikiButton.test.tsx
git commit -m "feat(ask): SaveAsWikiButton renders only for synthesis briefs"
```

### Task E16.3: Backend save endpoint + wire button into Ask answer flow

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (add `POST /api/wiki/inspiration/from-synthesis`)
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (add `saveSynthesisAsInspirationPage`)
- Modify: `apps/desktop-shell/src/features/ask/AskMessage.tsx` (render `SaveAsWikiButton` after answer)
- Test: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` test module

**Step 1: Write the failing Rust test**

```rust
#[tokio::test]
async fn synthesis_to_inspiration_creates_page_with_source_refs() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(tmp.path().to_path_buf());
    let body = serde_json::json!({
        "brief": {
            "title": "Quiet design language",
            "insight": "Form follows restraint",
            "evidence": ["A", "B"],
            "recurring": ["white space"],
            "use_cases": ["pricing pages"],
            "archive_candidates": [],
            "next_actions": ["draft v1"]
        },
        "source_refs": ["raw:00001", "raw:00002"]
    });
    let resp = app.oneshot(Request::builder()
        .method("POST")
        .uri("/api/wiki/inspiration/from-synthesis")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap()).await.unwrap();
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = serde_json::from_slice(&resp.collect_body().await).unwrap();
    let slug = json["slug"].as_str().unwrap();
    let (summary, body) = read_wiki_page(&wiki_store::resolve_paths(tmp.path()).unwrap(), slug).unwrap();
    assert_eq!(summary.category, "inspiration");
    assert!(body.contains("Form follows restraint"));
    assert_eq!(summary.source_refs.unwrap(), vec!["raw:00001".to_string(), "raw:00002".to_string()]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p desktop-server synthesis_to_inspiration_creates_page_with_source_refs`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
#[derive(Deserialize)]
pub struct SynthesisBrief {
    pub title: String,
    pub insight: String,
    pub evidence: Vec<String>,
    pub recurring: Vec<String>,
    pub use_cases: Vec<String>,
    pub archive_candidates: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Deserialize)]
pub struct SynthesisRequest {
    pub brief: SynthesisBrief,
    pub source_refs: Vec<String>,
}

pub async fn post_synthesis_to_inspiration(
    State(state): State<AppState>,
    Json(req): Json<SynthesisRequest>,
) -> impl IntoResponse {
    let slug = format!("inspiration-{}-{}", slugify(&req.brief.title), now_compact_id());
    let body = render_inspiration_body(&req.brief, &req.source_refs);
    match wiki_store::overwrite_wiki_page_content(&state.paths, &slug, "inspiration", &body) {
        Ok(()) => Json(serde_json::json!({"slug": slug})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn render_inspiration_body(brief: &SynthesisBrief, source_refs: &[String]) -> String {
    format!(
        "---\ntype: inspiration\nstatus: active\ntitle: {}\nsummary: {}\npriority: medium\nvitality: seed\nsource_refs:\n{}\ncreated_at: {}\n---\n\n## Insight\n\n{}\n\n## Evidence\n\n{}\n\n## Recurring elements\n\n{}\n\n## Use cases\n\n{}\n\n## Archive candidates\n\n{}\n\n## Next actions\n\n{}\n",
        brief.title, brief.insight,
        source_refs.iter().map(|r| format!("  - {r}")).collect::<Vec<_>>().join("\n"),
        chrono::Utc::now().to_rfc3339(),
        brief.insight,
        brief.evidence.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n"),
        brief.recurring.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n"),
        brief.use_cases.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n"),
        brief.archive_candidates.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n"),
        brief.next_actions.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n"),
    )
}
```

```rust
// In router:
.route("/api/wiki/inspiration/from-synthesis", post(handlers::wiki_crud::post_synthesis_to_inspiration))
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p desktop-server synthesis_to_inspiration_creates_page_with_source_refs`
Expected: PASS.

**Step 5: Wire frontend**

```typescript
// repository.ts
export async function saveSynthesisAsInspirationPage(payload: {
  brief: { title: string; insight: string; evidence: string[]; recurring: string[]; use_cases: string[]; archive_candidates: string[]; next_actions: string[] };
  source_refs: string[];
}): Promise<{ slug: string }> {
  return fetchJson("/api/wiki/inspiration/from-synthesis", { method: "POST", body: JSON.stringify(payload) });
}
```

```tsx
// In AskMessage.tsx — when message.role === "assistant":
{message.role === "assistant" && (
  <SaveAsWikiButton
    rawAnswer={message.content}
    citationSlugs={message.citations?.map((c) => c.slug) ?? []}
    onSaved={() => { /* toast */ }}
  />
)}
```

**Step 6: Run full slice verification**

Run: `cd apps/desktop-shell && npm run build && cargo test -p desktop-server`
Expected: build clean, all tests pass.

**Step 7: Commit**

```bash
git add rust/crates/desktop-server/src/handlers/wiki_crud.rs rust/crates/desktop-server/src/lib.rs apps/desktop-shell/src/api/wiki/repository.ts apps/desktop-shell/src/features/ask/AskMessage.tsx
git commit -m "feat(ask): synthesize-brief saves as inspiration wiki page"
```

---

## Slice Matrix

| Slice | Primary surface | Risk | Minimum gate |
|---|---|---|---|
| E11 | Inbox + Wiki | Falsely clusters unrelated entries | Unit tests on `detectInspirationClusters`, smoke 3-entry fixture |
| E12 | Home | Annoying users with persistent prompt | Phase machine unit tests, dismissal persistence |
| E13 | Inbox + Connections | Privacy concern of feedback log | JSONL is local-only, no PII in events |
| E14 | Rules Studio | Suggesting overly aggressive cooling | Floor at 14 days, never overwrite YAML silently |
| E15 | Maintainer + Inbox + Ask | Over-resurfacing creates noise | 7-day window + 3-citation threshold |
| E16 | Ask + Wiki | User accidentally creates near-duplicate inspiration pages | Title pattern dedupes via slug timestamp |

---

## Quality Matrix

| Surface | Required verification |
|---|---|
| Frontend pure logic | `vitest run` for the file + `npm run build` |
| Frontend React components | `vitest run` with @testing-library + visual smoke via preview |
| Rust unit logic | `cargo test -p <crate>` |
| API endpoint | Handler-level test with `tower::ServiceExt::oneshot` |
| Cross-slice integration | `npm run test:buddy:smoke` (existing playwright suite) before slice's last commit |

---

## Risk Matrix

| Risk | Mitigation |
|---|---|
| Inspiration cluster false positives | Min cluster size of 3, ignore `unknown` domain, dismissable banner |
| Aesthetic onboarding annoys users | One-shot dismissal persists permanently; 7-day decision then never re-asks |
| Telemetry log grows unbounded | JSONL append-only, future slice can add rotation; current sample size always windowed at 30 days |
| Auto-degrade hides real bugs | Show degrade status visibly in Connections panel + reset button (manual override) |
| Cooling calibration recommends too short | Floor at 14 days, user must click apply |
| Re-emergence floods Inbox | 7-day citation window + threshold of 3, plus reject path that logs reason |
| Synthesis-to-page creates orphan pages | source_refs always populated from citations; dedupe via slug timestamp |

---

## Completion Definition

The plan is complete when Buddy can show a user:

- ✅ When 3+ entries cluster around a use, suggest aggregating into an inspiration page (E11).
- ✅ Explicitly invite users to try aesthetic lenses for one week (E12).
- ✅ Visibly track and degrade cross-domain inference quality (E13).
- ✅ Calibrate cooling threshold from user's actual behavior (E14).
- ✅ Surface archived pages back to attention when newly relevant (E15).
- ✅ Save Ask synthesis directly as wiki inspiration pages (E16).

All actions remain reversible through Vault Git. No automatic deletions. AI judgments cite reasons. Source identity never overwritten.

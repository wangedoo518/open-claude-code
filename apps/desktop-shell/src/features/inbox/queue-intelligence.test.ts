/**
 * Q1 Inbox Queue Intelligence — unit tests for the seven-rule ladder.
 *
 * Runs under vitest. The harness isn't wired up in `apps/desktop-shell`
 * yet (no `vitest` devDependency / no `vitest.config.ts`), so these
 * tests are authored against the locked contract and will execute
 * verbatim once a later sprint adds vitest. Meanwhile the file still
 * type-checks under the project's main `tsc --noEmit` pass.
 *
 * We avoid `import { describe, it, expect } from "vitest"` so that the
 * missing dependency doesn't break `tsc`. Instead, the globals below
 * are declared locally — vitest's own globals plugin (or the
 * `vitest/globals` types reference) will merge cleanly once installed.
 * Behavioural semantics match vitest's chai-like API.
 */

import {
  ENTROPY_GROUP_ORDER,
  computeQueueIntelligence,
  defaultLifecycleForEntropyGroup,
  groupByEntropyJudgment,
  groupAndSortByAction,
  GROUP_ORDER,
  type QueueIntelligence,
  type RecommendedAction,
} from "./queue-intelligence";
import type { InboxEntry } from "@/api/wiki/types";
import type { IngestDecision } from "@/lib/tauri";

// ── Local vitest ambient globals (drop once vitest is installed) ───
//
// These match the subset of the vitest API that we use; they are
// assignable from vitest's real types via structural compatibility.
// When `vitest` is added to devDependencies, this block becomes a
// no-op duplicate declaration — remove it then for cleanliness.

type TestFn = () => void | Promise<void>;
interface SuiteFn {
  (name: string, fn: () => void): void;
  skip: (name: string, fn: () => void) => void;
}
interface ItFn {
  (name: string, fn: TestFn): void;
  skip: (name: string, fn: TestFn) => void;
}
interface Expect<T> {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
  toBeGreaterThan(expected: number): void;
  toBeLessThan(expected: number): void;
  toBeGreaterThanOrEqual(expected: number): void;
  toBeDefined(): void;
  toBeUndefined(): void;
  toContain(expected: unknown): void;
  toHaveLength(expected: number): void;
  not: Expect<T>;
}
declare const describe: SuiteFn;
declare const it: ItFn;
declare const expect: <T>(actual: T) => Expect<T>;

// ── Fixture helpers ────────────────────────────────────────────────

/**
 * Minimal valid `InboxEntry` factory. All fields defaulted to a
 * "blank new-raw" shape; callers override only the signals relevant
 * to the rule under test.
 */
function makeEntry(partial: Partial<InboxEntry> = {}): InboxEntry {
  return {
    id: 1,
    kind: "new-raw",
    status: "pending",
    title: "test entry",
    description: "",
    source_raw_id: null,
    created_at: new Date().toISOString(),
    resolved_at: null,
    ...partial,
  };
}

/** Entry decorated with `intelligence` for the grouping tests. */
type DecoratedEntry = InboxEntry & { intelligence: QueueIntelligence };

function decorate(
  entry: InboxEntry,
  decision?: IngestDecision | null,
): DecoratedEntry {
  return { ...entry, intelligence: computeQueueIntelligence(entry, decision) };
}

// ── r1-r7: the locked seven-rule ladder ────────────────────────────

describe("computeQueueIntelligence — 7 规则", () => {
  it("r1: proposal_status=pending → open_diff_preview + pending_proposal", () => {
    const entry = makeEntry({
      proposal_status: "pending",
      target_page_slug: "example-page",
    });
    const result = computeQueueIntelligence(entry);
    expect(result.recommended_action).toBe("open_diff_preview");
    expect(result.reason_code).toBe("pending_proposal");
    expect(result.target_candidate).toEqual({
      slug: "example-page",
      source: "target",
    });
    // r1 is top priority — base 90, expect high score.
    expect(result.score).toBeGreaterThanOrEqual(80);
  });

  it("r2: target_page_slug set (no pending proposal) → update_existing", () => {
    const entry = makeEntry({ target_page_slug: "existing-page" });
    const result = computeQueueIntelligence(entry);
    expect(result.recommended_action).toBe("update_existing");
    expect(result.reason_code).toBe("has_target_slug");
    expect(result.target_candidate).toEqual({
      slug: "existing-page",
      source: "target",
    });
  });

  it("r3: IngestDecision.content_duplicate → suggest_reject", () => {
    const entry = makeEntry();
    const decision: IngestDecision = {
      kind: "content_duplicate",
      matching_raw_id: 42,
      matching_url: "https://example.com/dup",
    };
    const result = computeQueueIntelligence(entry, decision);
    expect(result.recommended_action).toBe("suggest_reject");
    expect(result.reason_code).toBe("duplicate_content");
  });

  it("r4: IngestDecision.reused_approved → defer", () => {
    const entry = makeEntry();
    const decision: IngestDecision = {
      kind: "reused_approved",
      reason: "already in wiki",
    };
    const result = computeQueueIntelligence(entry, decision);
    expect(result.recommended_action).toBe("defer");
    expect(result.reason_code).toBe("already_approved");
  });

  it("r5: IngestDecision.reused_after_reject → ask_first + rejected_history", () => {
    const entry = makeEntry();
    const decision: IngestDecision = {
      kind: "reused_after_reject",
      reason: "previously rejected: low signal",
    };
    const result = computeQueueIntelligence(entry, decision);
    expect(result.recommended_action).toBe("ask_first");
    expect(result.reason_code).toBe("rejected_history");
    // r5 nudges +10 over the plain r7 ask_first base (20), so expect > 20.
    expect(result.score).toBeGreaterThan(20);
  });

  it("r6: new-raw + source_raw_id + no target slug → create_new", () => {
    const entry = makeEntry({
      kind: "new-raw",
      source_raw_id: 123,
      target_page_slug: null,
      proposed_wiki_slug: "proposed-slug",
    });
    const result = computeQueueIntelligence(entry);
    expect(result.recommended_action).toBe("create_new");
    expect(result.reason_code).toBe("fresh_content");
    expect(result.target_candidate).toEqual({
      slug: "proposed-slug",
      source: "proposed",
    });
  });

  it("r7: missing signals → ask_first (default catch-all)", () => {
    const entry = makeEntry({
      kind: "conflict", // not new-raw → bypasses r6
      source_raw_id: null,
      target_page_slug: null,
    });
    const result = computeQueueIntelligence(entry);
    expect(result.recommended_action).toBe("ask_first");
    expect(result.reason_code).toBe("missing_signals");
  });

  // Precedence safety check — r1 must beat r2 even when both match.
  it("precedence: pending proposal wins over target_page_slug", () => {
    const entry = makeEntry({
      proposal_status: "pending",
      target_page_slug: "some-page",
    });
    expect(computeQueueIntelligence(entry).recommended_action).toBe(
      "open_diff_preview",
    );
  });

  // r2 must beat r3/r4/r5/r6 — target slug short-circuits IngestDecision.
  it("precedence: target_page_slug wins over content_duplicate", () => {
    const entry = makeEntry({ target_page_slug: "page" });
    const decision: IngestDecision = {
      kind: "content_duplicate",
      matching_raw_id: 1,
      matching_url: "https://x",
    };
    expect(computeQueueIntelligence(entry, decision).recommended_action).toBe(
      "update_existing",
    );
  });
});

// ── groupAndSortByAction ───────────────────────────────────────────

describe("groupAndSortByAction", () => {
  it("组内按 score desc 排序", () => {
    // Two entries in update_existing group with different ages →
    // freshness modifier nudges fresh one higher.
    const freshEntry = decorate(
      makeEntry({
        id: 1,
        target_page_slug: "page-a",
        created_at: new Date().toISOString(), // fresh → +10
      }),
    );
    const staleEntry = decorate(
      makeEntry({
        id: 2,
        target_page_slug: "page-b",
        created_at: new Date(Date.now() - 10 * 86_400_000).toISOString(), // -10
      }),
    );
    // Shuffle input order so the sort has work to do.
    const grouped = groupAndSortByAction([staleEntry, freshEntry]);
    expect(grouped).toHaveLength(1);
    expect(grouped[0].action).toBe("update_existing");
    expect(grouped[0].entries[0].id).toBe(1); // fresh wins
    expect(grouped[0].entries[1].id).toBe(2);
  });

  it("tie-break: 同 score 按 id desc", () => {
    const now = new Date().toISOString();
    const a = decorate(makeEntry({ id: 1, target_page_slug: "p", created_at: now }));
    const b = decorate(makeEntry({ id: 5, target_page_slug: "p", created_at: now }));
    const grouped = groupAndSortByAction([a, b]);
    // Same base + freshness → same score → higher id first.
    expect(grouped[0].entries[0].id).toBe(5);
    expect(grouped[0].entries[1].id).toBe(1);
  });

  it("空组被隐藏", () => {
    // Only supply ask_first entries → only one group returned.
    const only = decorate(
      makeEntry({ id: 1, kind: "conflict", source_raw_id: null }),
    );
    const grouped = groupAndSortByAction([only]);
    expect(grouped).toHaveLength(1);
    expect(grouped[0].action).toBe("ask_first");
  });

  it("GROUP_ORDER 顺序正确：open_diff_preview → defer", () => {
    // One entry per group, ensure returned order matches GROUP_ORDER.
    const entries: DecoratedEntry[] = [
      // defer (last)
      decorate(makeEntry({ id: 10 }), {
        kind: "reused_approved",
        reason: "x",
      }),
      // open_diff_preview (first)
      decorate(makeEntry({ id: 20, proposal_status: "pending" })),
      // create_new (3rd)
      decorate(makeEntry({ id: 30, source_raw_id: 9 })),
      // ask_first (4th)
      decorate(makeEntry({ id: 40, kind: "conflict" })),
      // update_existing (2nd)
      decorate(makeEntry({ id: 50, target_page_slug: "p" })),
      // suggest_reject (5th)
      decorate(makeEntry({ id: 60 }), {
        kind: "content_duplicate",
        matching_raw_id: 1,
        matching_url: "https://x",
      }),
    ];
    const grouped = groupAndSortByAction(entries);
    const actualOrder: RecommendedAction[] = grouped.map((g) => g.action);
    expect(actualOrder).toEqual([
      "open_diff_preview",
      "update_existing",
      "create_new",
      "ask_first",
      "suggest_reject",
      "defer",
    ]);
    // Sanity: the declared GROUP_ORDER matches.
    expect(GROUP_ORDER).toEqual([
      "open_diff_preview",
      "update_existing",
      "create_new",
      "ask_first",
      "suggest_reject",
      "defer",
    ]);
  });

  it("空输入返回空数组", () => {
    expect(groupAndSortByAction([])).toEqual([]);
  });
});
describe("groupByEntropyJudgment", () => {
  it("puts pending proposals into the small review-today lane", () => {
    const entries: DecoratedEntry[] = [
      decorate(makeEntry({ id: 1, proposal_status: "pending" })),
      decorate(makeEntry({ id: 2, proposal_status: "pending" })),
      decorate(makeEntry({ id: 3, proposal_status: "pending" })),
      decorate(makeEntry({ id: 4, proposal_status: "pending" })),
    ];

    const grouped = groupByEntropyJudgment(entries, { todayLimit: 2 });
    const review = grouped.find((group) => group.key === "review_today");

    expect(review).toBeDefined();
    expect(review?.entries.map((entry) => entry.id)).toEqual([4, 3]);
    expect(review?.entries).toHaveLength(2);
  });

  it("keeps duplicate and already-approved material in cooling, never bulk-review", () => {
    const duplicate = decorate(makeEntry({ id: 1 }), {
      kind: "content_duplicate",
      matching_raw_id: 42,
      matching_url: "https://example.com/dup",
    });
    const reused = decorate(makeEntry({ id: 2 }), {
      kind: "reused_approved",
      reason: "already accepted",
    });

    const grouped = groupByEntropyJudgment([duplicate, reused]);
    const cooling = grouped.find((group) => group.key === "cooling");

    expect(cooling).toBeDefined();
    expect(cooling?.meta.bulkAccept).toBe(false);
    expect(cooling?.entries.map((entry) => entry.id)).toEqual([1, 2]);
  });

  it("splits mergeable, crystallize, and needs-decision judgments", () => {
    const merge = decorate(makeEntry({ id: 1, target_page_slug: "existing" }));
    const create = decorate(makeEntry({ id: 2, source_raw_id: 10 }));
    const ask = decorate(makeEntry({ id: 3, kind: "conflict" }));

    const grouped = groupByEntropyJudgment([ask, create, merge]);
    const order = grouped.map((group) => group.key);

    expect(order).toEqual(["crystallize", "mergeable", "needs_decision"]);
  });

  it("recognizes repeated target direction as growing before ordinary merge", () => {
    const first = decorate(makeEntry({ id: 1, target_page_slug: "taste-map" }));
    const second = decorate(makeEntry({ id: 2, target_page_slug: "taste-map" }));

    const grouped = groupByEntropyJudgment([first, second]);
    const growing = grouped.find((group) => group.key === "growing");

    expect(growing).toBeDefined();
    expect(growing?.entries.map((entry) => entry.id)).toEqual([2, 1]);
  });

  it("declares entropy group order from review through cooling", () => {
    expect(ENTROPY_GROUP_ORDER).toEqual([
      "review_today",
      "growing",
      "crystallize",
      "mergeable",
      "needs_decision",
      "cooling",
    ]);
  });

  it("maps entropy groups to lifecycle defaults for maintain payloads", () => {
    expect(defaultLifecycleForEntropyGroup("growing").priority).toBe("high");
    expect(defaultLifecycleForEntropyGroup("growing").vitality).toBe("growing");
    expect(defaultLifecycleForEntropyGroup("crystallize").priority).toBe("medium");
    expect(defaultLifecycleForEntropyGroup("crystallize").vitality).toBe("seed");
    expect(defaultLifecycleForEntropyGroup("cooling").priority).toBe("low");
    expect(defaultLifecycleForEntropyGroup("cooling").vitality).toBe("cooling");
  });
});

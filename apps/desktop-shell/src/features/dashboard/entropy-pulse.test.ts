/**
 * Slice E2 - Home entropy pulse derivation contract.
 *
 * Authored in the same ambient-vitest style as the existing desktop-shell
 * tests: it type-checks today and can run unchanged once a test runner is
 * wired into the package.
 */

import type { WikiPageSummary } from "@/api/wiki/types";
import {
  buildEntropyPulseSummary,
  buildPatrolLifecycleSuggestions,
  countPatrolLifecycleSuggestions,
} from "./entropy-pulse";

type TestFn = () => void | Promise<void>;
interface ItFn {
  (name: string, fn: TestFn): void;
}
interface Expect<T> {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
  toContain(expected: unknown): void;
  not: Expect<T>;
}
declare const describe: (name: string, fn: () => void) => void;
declare const it: ItFn;
declare const expect: <T>(actual: T) => Expect<T>;

function page(partial: Partial<WikiPageSummary>): WikiPageSummary {
  return {
    slug: "page",
    title: "Page",
    summary: "Summary",
    created_at: "2026-05-01T00:00:00Z",
    byte_size: 100,
    category: "concept",
    ...partial,
  };
}

describe("buildEntropyPulseSummary", () => {
  it("separates growth, review-today, cooling, and expressed signals", () => {
    const result = buildEntropyPulseSummary(
      [
        page({
          slug: "visual-patterns",
          title: "Visual Patterns",
          priority: "high",
          vitality: "growing",
          priority_reason: "Repeated across design captures",
        }),
        page({
          slug: "already-used",
          title: "Already Used",
          priority: "high",
          vitality: "growing",
          expressed_in: ["ask:session-1"],
        }),
        page({
          slug: "old-link",
          title: "Old Link",
          priority: "low",
          vitality: "cooling",
        }),
        page({
          slug: "due-review",
          title: "Due Review",
          priority: "medium",
          vitality: "stable",
          next_review_at: "2026-05-01T00:00:00Z",
        }),
      ],
      { now: Date.parse("2026-05-07T00:00:00Z") },
    );

    expect(result.hasLifecycleSignals).toBe(true);
    expect(result.growing.map((item) => item.slug)).toContain("visual-patterns");
    expect(result.growing.map((item) => item.slug)).toContain("already-used");
    expect(result.reviewToday.map((item) => item.slug)).toEqual([
      "visual-patterns",
      "due-review",
    ]);
    expect(result.reviewToday.map((item) => item.slug)).not.toContain("already-used");
    expect(result.cooling.map((item) => item.slug)).toEqual(["old-link"]);
    expect(result.highPriorityCount).toBe(2);
    expect(result.coolingCount).toBe(1);
  });

  it("treats missing lifecycle metadata as unknown, not as archive advice", () => {
    const result = buildEntropyPulseSummary([
      page({ slug: "legacy", title: "Legacy Page" }),
    ]);

    expect(result.hasLifecycleSignals).toBe(false);
    expect(result.growing).toEqual([]);
    expect(result.reviewToday).toEqual([]);
    expect(result.cooling).toEqual([]);
  });

  it("summarizes patrol lifecycle suggestions for the home pulse", () => {
    const report = {
      checked_at: "2026-05-07T00:00:00Z",
      summary: {
        orphans: 0,
        stale: 0,
        schema_violations: 0,
        oversized: 0,
        stubs: 0,
        confidence_decay: 0,
        uncrystallized: 0,
        stale_sparks: 1,
        cooling_pages: 1,
        unexpressed_high_priority: 1,
        noise_candidates: 1,
      },
      issues: [
        {
          kind: "noise-candidate" as const,
          page_slug: "noise",
          description: "Weak signal.",
          suggested_action: "Merge or archive.",
        },
        {
          kind: "unexpressed-high-priority" as const,
          page_slug: "high-unused",
          description: "High priority but unused.",
          suggested_action: "Express it.",
        },
      ],
    };

    const suggestions = buildPatrolLifecycleSuggestions(report, [
      page({ slug: "high-unused", title: "High Unused" }),
      page({ slug: "noise", title: "Noise" }),
    ]);

    expect(countPatrolLifecycleSuggestions(report)).toBe(4);
    expect(suggestions.map((item) => item.slug)).toEqual(["high-unused", "noise"]);
    expect(suggestions[0].title).toBe("High Unused");
    expect(suggestions[0].badge).toBe("express");
  });
});

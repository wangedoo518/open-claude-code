/**
 * Slice E18.1 — ROI pulse pure-function tests.
 *
 * Ambient-vitest contract: type-checks via `tsc --noEmit`, runs
 * verbatim once vitest is wired up. Mirrors the patterns in
 * `entropy-pulse.test.ts`.
 */

import {
  computeRoiFunnel,
  computeRoiByPurpose,
  type RoiPageSummary,
} from "./roi-pulse";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
};

const NOW = 1_700_000_000_000;
const DAY = 86_400_000;

function page(over: Partial<RoiPageSummary>): RoiPageSummary {
  return {
    slug: over.slug ?? "x",
    purpose: over.purpose ?? ["learning"],
    expressed_in: over.expressed_in ?? [],
    verdict: over.verdict ?? null,
    vitality: over.vitality ?? null,
    created_at_ms: over.created_at_ms ?? NOW - 5 * DAY,
  };
}

describe("computeRoiFunnel", () => {
  it("counts raw → wiki → expressed within window", () => {
    const f = computeRoiFunnel({
      pages: [
        page({ slug: "a", expressed_in: ["doc:1"] }),
        page({ slug: "b" }),
        page({ slug: "c", created_at_ms: NOW - 100 * DAY }), // out of window
      ],
      rawCount: 8,
      now: NOW,
      windowDays: 30,
    });
    expect(f).toEqual({
      windowDays: 30,
      rawCount: 8,
      wikiCount: 2,
      expressedCount: 1,
    });
  });

  it("returns zeroes when nothing has been captured", () => {
    const f = computeRoiFunnel({
      pages: [],
      rawCount: 0,
      now: NOW,
      windowDays: 30,
    });
    expect(f).toEqual({
      windowDays: 30,
      rawCount: 0,
      wikiCount: 0,
      expressedCount: 0,
    });
  });
});

describe("computeRoiByPurpose", () => {
  it("groups by purpose and only emits rows with sample ≥ 3", () => {
    const rows = computeRoiByPurpose({
      pages: [
        page({ purpose: ["learning"], verdict: "should_continue" }),
        page({ purpose: ["learning"], verdict: "should_let_go" }),
        page({ purpose: ["learning"], expressed_in: ["d"] }),
        page({ purpose: ["building"], verdict: "should_continue" }), // sample = 1
      ],
      now: NOW,
      windowDays: 30,
    });
    expect(rows.length).toBe(1);
    expect(rows[0].purpose).toBe("learning");
    expect(rows[0].sample).toBe(3);
    expect(rows[0].continueRate).toBe(1 / 3);
    expect(rows[0].letGoRate).toBe(1 / 3);
    expect(rows[0].expressedRate).toBe(1 / 3);
  });

  it("treats vitality=growing as a soft continue signal when verdict missing", () => {
    const rows = computeRoiByPurpose({
      pages: [
        page({ purpose: ["learning"], vitality: "growing" }),
        page({ purpose: ["learning"], vitality: "growing" }),
        page({ purpose: ["learning"], vitality: "cooling" }),
      ],
      now: NOW,
      windowDays: 30,
    });
    expect(rows[0].continueSoftCount).toBe(2);
  });
});

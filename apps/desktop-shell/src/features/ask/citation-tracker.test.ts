/**
 * Slice E15.4 — citation tracker unit tests.
 *
 * Ambient-vitest contract: type-checks via `tsc --noEmit`, will run
 * verbatim once vitest is wired up. Shape mirrors the existing
 * `cross-domain.test.ts` and `queue-intelligence.test.ts` so the
 * behavior contract is locked even before the runner exists.
 */

import { CitationTracker } from "./citation-tracker";

declare const describe: (name: string, fn: () => void) => void;
declare const it: (name: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
  toContain(expected: unknown): void;
};

describe("CitationTracker", () => {
  const NOW = 1_700_000_000_000;
  const DAY = 24 * 60 * 60 * 1000;

  it("emits resurface signal at exactly 3 citations of a cooling page within 7 days", () => {
    const tracker = new CitationTracker(() => ({ vitality: "cooling" }));
    const fired: string[] = [];
    tracker.onResurfaceCandidate = (slug) => fired.push(slug);

    tracker.recordCitation("api-design", NOW);
    expect(fired).toEqual([]);
    tracker.recordCitation("api-design", NOW + 1000);
    expect(fired).toEqual([]);
    tracker.recordCitation("api-design", NOW + 2000);
    expect(fired).toEqual(["api-design"]);
  });

  it("does not emit when the page is active / growing / stable", () => {
    const tracker = new CitationTracker(() => ({ vitality: "growing" }));
    const fired: string[] = [];
    tracker.onResurfaceCandidate = (slug) => fired.push(slug);

    tracker.recordCitation("api-design", NOW);
    tracker.recordCitation("api-design", NOW + 1000);
    tracker.recordCitation("api-design", NOW + 2000);
    expect(fired).toEqual([]);
  });

  it("emits for archived pages too, not just cooling", () => {
    const tracker = new CitationTracker(() => ({ vitality: "archived" }));
    const fired: string[] = [];
    tracker.onResurfaceCandidate = (slug) => fired.push(slug);

    tracker.recordCitation("retired-rfc", NOW);
    tracker.recordCitation("retired-rfc", NOW + 100);
    tracker.recordCitation("retired-rfc", NOW + 200);
    expect(fired).toEqual(["retired-rfc"]);
  });

  it("decays citations older than 7 days", () => {
    const tracker = new CitationTracker(() => ({ vitality: "cooling" }));
    const fired: string[] = [];
    tracker.onResurfaceCandidate = (slug) => fired.push(slug);

    // First citation 8 days ago — must drop out of the window
    tracker.recordCitation("old-page", NOW - 8 * DAY);
    // Two fresh citations: only 2 in window → no fire
    tracker.recordCitation("old-page", NOW);
    tracker.recordCitation("old-page", NOW + 100);
    expect(fired).toEqual([]);
  });

  it("does not re-fire after the first fire for the same slug", () => {
    const tracker = new CitationTracker(() => ({ vitality: "cooling" }));
    const fired: string[] = [];
    tracker.onResurfaceCandidate = (slug) => fired.push(slug);

    tracker.recordCitation("api-design", NOW);
    tracker.recordCitation("api-design", NOW + 1);
    tracker.recordCitation("api-design", NOW + 2);
    tracker.recordCitation("api-design", NOW + 3);
    tracker.recordCitation("api-design", NOW + 4);
    expect(fired).toEqual(["api-design"]);
  });

  it("isolates slugs: 3 citations of A do not flag B", () => {
    const tracker = new CitationTracker((slug) =>
      slug === "a" ? { vitality: "cooling" } : { vitality: "growing" },
    );
    const fired: string[] = [];
    tracker.onResurfaceCandidate = (slug) => fired.push(slug);

    tracker.recordCitation("a", NOW);
    tracker.recordCitation("a", NOW + 1);
    tracker.recordCitation("a", NOW + 2);
    tracker.recordCitation("b", NOW);
    tracker.recordCitation("b", NOW + 1);
    tracker.recordCitation("b", NOW + 2);
    expect(fired).toEqual(["a"]);
  });

  it("ignores empty slug calls", () => {
    const tracker = new CitationTracker(() => ({ vitality: "cooling" }));
    const fired: string[] = [];
    tracker.onResurfaceCandidate = (slug) => fired.push(slug);

    tracker.recordCitation("", NOW);
    tracker.recordCitation("", NOW + 1);
    tracker.recordCitation("", NOW + 2);
    expect(fired).toEqual([]);
  });
});

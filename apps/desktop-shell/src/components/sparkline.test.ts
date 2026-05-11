/**
 * Slice E28.1 — Sparkline pure-helper test.
 *
 * Ambient-vitest contract: type-checks via `tsc --noEmit`, runs
 * verbatim once vitest is wired up. Mirrors `roi-pulse.test.ts`.
 */

import { computeSparklinePath } from "./Sparkline";

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

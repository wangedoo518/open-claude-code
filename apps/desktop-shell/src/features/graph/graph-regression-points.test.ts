/**
 * Graph regression contract.
 *
 * The graph view is interaction-heavy and the current force layout,
 * dragging, zooming, panning, edge draw, node enter and hover logic are
 * already product-good. This file locks the regression points before
 * visual-only redesign work so future edits do not accidentally treat
 * those behaviours as disposable styling.
 *
 * The desktop shell does not wire Vitest yet. This follows the existing
 * local-ambient pattern in nearby tests: it type-checks today and becomes
 * executable unchanged once the test runner is added.
 */

import {
  GRAPH_INTERACTION_REGRESSION_POINTS,
  GRAPH_VISUAL_LIMITS,
} from "./graph-regression-points";

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
  toBeLessThanOrEqual(expected: number): void;
  toContain(expected: unknown): void;
}
declare const describe: SuiteFn;
declare const it: ItFn;
declare const expect: <T>(actual: T) => Expect<T>;

describe("graph regression points", () => {
  it("keeps the existing interaction contract explicit", () => {
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("force-layout");
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("node-drag");
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("wheel-zoom");
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("canvas-pan");
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("edge-draw-animation");
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("node-enter-animation");
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("hover-highlight");
    expect(GRAPH_INTERACTION_REGRESSION_POINTS).toContain("search-filter");
  });

  it("keeps the visual redesign inside the agreed limits", () => {
    expect(GRAPH_VISUAL_LIMITS.maxNodeDiameter).toBeLessThanOrEqual(16);
    expect(GRAPH_VISUAL_LIMITS.maxEdgeStrokeWidth).toBeLessThanOrEqual(1);
    expect(GRAPH_VISUAL_LIMITS.maxClusterOpacity).toBe(0);
    expect(GRAPH_VISUAL_LIMITS.dotGridSize).toBe(0);
    expect(GRAPH_VISUAL_LIMITS.dotDiameter).toBe(0);
  });
});

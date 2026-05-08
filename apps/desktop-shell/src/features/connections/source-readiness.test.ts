/**
 * Slice E10 - high-volume source readiness gate.
 *
 * Ambient-vitest contract: type-checks during build, and can run unchanged
 * once the package wires a browser/unit runner.
 */

import {
  SOURCE_READINESS_REQUIREMENTS,
  evaluateHighVolumeSourcePlans,
  evaluateSourceReadiness,
  type SourceReadinessPlan,
} from "./source-readiness";

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

function completePlan(partial: Partial<SourceReadinessPlan> = {}): SourceReadinessPlan {
  return {
    id: "source",
    label: "Source",
    sourceKind: "test",
    notes: "All gates covered.",
    captureOnly: false,
    requirements: Object.fromEntries(
      SOURCE_READINESS_REQUIREMENTS.map((requirement) => [requirement.id, true]),
    ),
    ...partial,
  };
}

describe("source readiness gate", () => {
  it("passes only when every checklist item is present and the source is not capture-only", () => {
    const result = evaluateSourceReadiness(completePlan());

    expect(result.ready).toBe(true);
    expect(result.completed).toBe(SOURCE_READINESS_REQUIREMENTS.length);
    expect(result.missing).toEqual([]);
    expect(result.blockers).toEqual([]);
  });

  it("blocks capture-only connectors even if they carry source evidence", () => {
    const result = evaluateSourceReadiness(
      completePlan({
        captureOnly: true,
      }),
    );

    expect(result.ready).toBe(false);
    expect(result.blockers).toContain("capture-only import is blocked");
  });

  it("keeps high-volume future sources blocked until dedupe, priority, cross-domain, archive, reason, and privacy are covered", () => {
    const evaluations = evaluateHighVolumeSourcePlans();
    const futureSources = evaluations.filter((source) => source.id !== "wechat");

    expect(futureSources.every((source) => !source.ready)).toBe(true);
    expect(
      futureSources.every((source) =>
        source.blockers.includes("capture-only import is blocked"),
      ),
    ).toBe(true);
    expect(evaluations.find((source) => source.id === "wechat")?.ready).toBe(true);
  });
});

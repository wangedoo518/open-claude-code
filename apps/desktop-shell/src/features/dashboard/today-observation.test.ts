/**
 * Slice E28.3 — pickObservation rule tests.
 *
 * Ambient-vitest contract.
 */

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
      inputs({
        gitDirty: true,
        gitChangedCount: 27,
        lastGitCommitMs: NOW - 2 * DAY,
      }),
    );
    expect(obs.id).toBe("vault-stale");
    expect(obs.message.includes("27")).toBe(true);
  });

  it("flags inbox backlog when pending ≥ 10", () => {
    const obs = pickObservation(inputs({ pendingInbox: 13 }));
    expect(obs.id).toBe("inbox-backlog");
    expect(obs.message.includes("13")).toBe(true);
  });

  it("celebrates expression activity when ≥3 in 7 days", () => {
    const obs = pickObservation(inputs({ recentExpressionCount7d: 4 }));
    expect(obs.id).toBe("express-streak");
  });

  it("nudges toward expression when many pages but 0 recent expressions", () => {
    const obs = pickObservation(
      inputs({ totalPages: 30, recentExpressionCount7d: 0 }),
    );
    expect(obs.id).toBe("knowledge-no-export");
  });

  it("falls back to default observation when nothing notable", () => {
    const obs = pickObservation(inputs());
    expect(obs.id).toBe("default");
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
    expect(obs.id).toBe("vault-stale");
  });

  // Reviewer fix — explicit regression test for the gitDirty:false
  // short-circuit. Previously implied by the default test (which
  // also has gitDirty:false), but this pins the contract: even with
  // 30 changes + 5 days stale, a clean working tree (gitDirty:false)
  // skips vault-stale.
  it("vault-stale requires gitDirty=true even with high change count", () => {
    const obs = pickObservation(
      inputs({
        gitDirty: false,
        gitChangedCount: 30,
        lastGitCommitMs: NOW - 5 * DAY,
      }),
    );
    expect(obs.id).toBe("default");
  });
});

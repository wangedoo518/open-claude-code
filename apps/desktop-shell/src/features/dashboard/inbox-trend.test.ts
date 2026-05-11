/**
 * Slice E28.1 — inbox-trend pure-helper tests.
 *
 * Ambient-vitest contract: type-checks via `tsc --noEmit`.
 */

import { bucketInboxByDay, type InboxTimestamped } from "./inbox-trend";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
};

const NOW = Date.parse("2026-05-11T12:00:00Z");
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

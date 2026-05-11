/**
 * Slice E28.1 — pure helper to bucket inbox entries into daily
 * counts for the inbox-stat sparkline. Lives next to other
 * dashboard pure helpers (entropy-pulse / roi-pulse) so all "data
 * → tiny chart" derivations sit together.
 *
 * Why pure (no JSX): tests don't need React, and the same fn can
 * later feed e.g. a tooltip "3 today / 2 yesterday / 0 / 0 / ..."
 * without recomputation.
 */

const DAY_MS = 86_400_000;

/** Minimal subset of InboxEntry needed for bucketing. Loose interface
 *  so tests don't need to import the full DTO. */
export interface InboxTimestamped {
  created_at: string;
}

/** Bucket entries into daily counts ending at `now`. Returns an
 *  array of length `days`, oldest bucket first. Entries with
 *  unparseable timestamps or outside the window are ignored.
 *
 *  Buckets align to LOCAL midnight (the user's calendar), NOT UTC
 *  midnight. Reviewer caught: UTC bucketing breaks for users
 *  outside UTC — e.g. a UTC+8 user creating an entry at 06:00
 *  local (= 22:00 UTC the previous day) would see the entry land
 *  in "yesterday's" bucket instead of "today's". Pinning to local
 *  midnight via Date.setHours(0,0,0,0) fixes this for any TZ
 *  because the JS Date object respects the runtime's local zone. */
export function bucketInboxByDay(
  entries: ReadonlyArray<InboxTimestamped>,
  opts: { now: number; days: number },
): number[] {
  const buckets = new Array<number>(opts.days).fill(0);
  const todayLocalMidnight = new Date(opts.now);
  todayLocalMidnight.setHours(0, 0, 0, 0);
  const todayMidnightMs = todayLocalMidnight.getTime();
  const day0Start = todayMidnightMs - (opts.days - 1) * DAY_MS;
  for (const entry of entries) {
    const t = Date.parse(entry.created_at);
    if (!Number.isFinite(t)) continue;
    if (t < day0Start || t >= todayMidnightMs + DAY_MS) continue;
    const idx = Math.floor((t - day0Start) / DAY_MS);
    if (idx >= 0 && idx < opts.days) {
      buckets[idx] += 1;
    }
  }
  return buckets;
}

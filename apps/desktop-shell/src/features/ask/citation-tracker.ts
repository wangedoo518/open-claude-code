/**
 * Slice E15.4 — Ask citation tracker.
 *
 * Counts wiki citations rendered in `UsedSourcesBar` over a rolling
 * 7-day window. When a cooling / archived page accumulates ≥ 3
 * citations in the window, fires `onResurfaceCandidate(slug)` so the
 * UI (or, in a future slice, a backend POST) can queue a Resurface
 * inbox task.
 *
 * Why a class instead of a hook: the tracker outlives any single
 * `UsedSourcesBar` mount. The Ask shell instantiates one tracker per
 * session and passes it down; rerender churn must NOT reset the
 * count window. Plain class avoids React lifecycle entanglement and
 * makes unit testing trivial — no fake timers, no act().
 *
 * Why no persistence (yet): MVP keeps citations in memory. A
 * follow-up slice can persist to `.clawwiki/ask-citations.jsonl` so
 * resurface signals survive desktop restarts. Until then, restarting
 * Buddy loses pending citation history — acceptable trade-off because
 * users typically discover stale pages within a single session.
 */

const WINDOW_MS = 7 * 24 * 60 * 60 * 1000;
const RESURFACE_THRESHOLD = 3;

export interface PageVitalityLookup {
  /** Returns the page metadata or undefined if Buddy has no record
   * of this slug yet. The tracker only fires for pages whose
   * vitality is `cooling` or `archived` — anything else (active,
   * growing, unknown) is ignored. */
  (slug: string): { vitality?: string | null } | undefined;
}

export class CitationTracker {
  private events = new Map<string, number[]>();
  /** Slugs we have already fired for in this session. Repeated
   * citations of an already-flagged page do not re-fire — the user
   * has already been told once. */
  private fired = new Set<string>();
  public onResurfaceCandidate?: (slug: string) => void;

  constructor(private readonly getPageMeta: PageVitalityLookup) {}

  /** Record a citation timestamp for `slug`. Side effects:
   * 1. Prune events older than the rolling window.
   * 2. If the count reaches the threshold AND the page is in a
   *    cooling/archived state AND we have not already fired for
   *    this slug, fire `onResurfaceCandidate`.
   */
  recordCitation(slug: string, now: number): void {
    if (!slug) return;
    const list = this.events.get(slug) ?? [];
    list.push(now);
    const fresh = list.filter((t) => now - t <= WINDOW_MS);
    this.events.set(slug, fresh);

    if (fresh.length < RESURFACE_THRESHOLD) return;
    if (this.fired.has(slug)) return;
    const meta = this.getPageMeta(slug);
    const vitality = meta?.vitality;
    if (vitality !== "cooling" && vitality !== "archived") return;

    this.fired.add(slug);
    this.onResurfaceCandidate?.(slug);
  }

  /** For tests: resets all state so callers can re-use one instance
   * across cases without leakage. Not used in production code. */
  reset(): void {
    this.events.clear();
    this.fired.clear();
  }
}

// ── Module singleton ────────────────────────────────────────────────
//
// One tracker per process. UsedSourcesBar mounts/unmounts many times
// during an Ask session; the tracker must persist across those mounts
// so the 7-day window is real, not "since this render".
//
// `pageVitalityRegistry` is a tiny in-memory map populated by the
// existing wiki page list query. UsedSourcesBar parents subscribe to
// that data and call `registerPageVitality(slug, meta)` on every
// refresh. Lookup is sync so the tracker's hot path does not block.
//
// The follow-up backend wiring (POST /api/wiki/resurface/from-citation
// → wiki_store::append_inbox_resurface_pending) is scoped to a future
// slice; for now the singleton fires onResurfaceCandidate to whatever
// listener the parent attaches, defaulting to a debug log.

const pageVitalityRegistry = new Map<string, { vitality?: string | null }>();

export function registerPageVitality(
  slug: string,
  meta: { vitality?: string | null },
): void {
  pageVitalityRegistry.set(slug, meta);
}

export function clearPageVitalityRegistry(): void {
  pageVitalityRegistry.clear();
}

export const askCitationTracker = new CitationTracker((slug) =>
  pageVitalityRegistry.get(slug),
);

// Default listener: log to dev console so the signal is observable
// before the backend wiring lands. Replaceable by callers that want
// to fire a real backend POST.
askCitationTracker.onResurfaceCandidate = (slug) => {
  // eslint-disable-next-line no-console
  console.info(
    `[ask-citation] cooling page \`${slug}\` cited 3+ times in 7d — Resurface candidate (backend wiring pending)`,
  );
};

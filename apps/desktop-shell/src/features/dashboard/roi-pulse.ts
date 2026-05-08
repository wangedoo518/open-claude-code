/**
 * Slice E18 — ROI Pulse Panel.
 *
 * Pure derivations of "投入 → 表达" funnels from existing wiki page +
 * raw count data. No fetches; the parent provides the data. Mirrors
 * the E13 / entropy-pulse pattern of read-only summaries.
 *
 * Buddy positioning: users decide what to keep investing in vs what
 * to let go of. That decision needs an honest reflection of how their
 * past 30/90 days actually paid off — how many raw inflows became
 * wiki pages, how many of those got `expressed_in` references.
 */

const DAY_MS = 24 * 60 * 60 * 1000;
const PURPOSE_MIN_SAMPLE = 3;

export interface RoiPageSummary {
  slug: string;
  purpose: ReadonlyArray<string>;
  expressed_in: ReadonlyArray<string>;
  verdict?: string | null;
  vitality?: string | null;
  /** Page creation time as epoch ms; pre-parse to avoid re-parsing per
   * window calculation. */
  created_at_ms: number;
}

export interface RoiFunnelInput {
  pages: ReadonlyArray<RoiPageSummary>;
  rawCount: number;
  now: number;
  windowDays: number;
}

export interface RoiFunnel {
  windowDays: number;
  rawCount: number;
  wikiCount: number;
  expressedCount: number;
}

export function computeRoiFunnel(input: RoiFunnelInput): RoiFunnel {
  const cutoff = input.now - input.windowDays * DAY_MS;
  const inWindow = input.pages.filter((p) => p.created_at_ms >= cutoff);
  const wikiCount = inWindow.length;
  const expressedCount = inWindow.filter((p) => p.expressed_in.length > 0).length;
  return {
    windowDays: input.windowDays,
    rawCount: input.rawCount,
    wikiCount,
    expressedCount,
  };
}

export interface RoiByPurposeInput {
  pages: ReadonlyArray<RoiPageSummary>;
  now: number;
  windowDays: number;
}

export interface RoiPurposeRow {
  purpose: string;
  sample: number;
  /** Share of pages where verdict === should_continue. */
  continueRate: number;
  /** Share of pages where verdict === should_let_go. */
  letGoRate: number;
  /** Share of pages with at least one expressed_in reference. */
  expressedRate: number;
  /** Pages where verdict is missing AND vitality === "growing". A
   * soft "this might still pay off" signal — surfaced separately so it
   * doesn't inflate the explicit verdict numbers. */
  continueSoftCount: number;
}

export function computeRoiByPurpose(
  input: RoiByPurposeInput,
): RoiPurposeRow[] {
  const cutoff = input.now - input.windowDays * DAY_MS;
  const inWindow = input.pages.filter((p) => p.created_at_ms >= cutoff);
  const byPurpose = new Map<string, RoiPageSummary[]>();
  for (const p of inWindow) {
    for (const purpose of p.purpose) {
      const list = byPurpose.get(purpose) ?? [];
      list.push(p);
      byPurpose.set(purpose, list);
    }
  }
  const out: RoiPurposeRow[] = [];
  for (const [purpose, pages] of byPurpose) {
    if (pages.length < PURPOSE_MIN_SAMPLE) continue;
    const sample = pages.length;
    const cont = pages.filter((p) => p.verdict === "should_continue").length;
    const lg = pages.filter((p) => p.verdict === "should_let_go").length;
    const expressed = pages.filter((p) => p.expressed_in.length > 0).length;
    const softCont = pages.filter(
      (p) => !p.verdict && p.vitality === "growing",
    ).length;
    out.push({
      purpose,
      sample,
      continueRate: cont / sample,
      letGoRate: lg / sample,
      expressedRate: expressed / sample,
      continueSoftCount: softCont,
    });
  }
  out.sort((a, b) => b.sample - a.sample);
  return out;
}

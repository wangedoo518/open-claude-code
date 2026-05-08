import type { PatrolIssue, PatrolReport, WikiPageSummary } from "@/api/wiki/types";

const DEFAULT_LIMIT = 3;
const GROWING_VITALITY = new Set(["growing", "seed"]);
const COOLING_VITALITY = new Set(["cooling", "archived", "noise"]);
const LIFECYCLE_PATROL_KINDS = [
  "unexpressed-high-priority",
  "stale-spark",
  "cooling-page",
  "noise-candidate",
] as const;

type LifecyclePatrolKind = (typeof LIFECYCLE_PATROL_KINDS)[number];

export interface EntropyPulseOptions {
  now?: number;
  limit?: number;
}

export interface EntropyPulseItem {
  slug: string;
  title: string;
  reason: string;
  badge: string;
  priority?: string | null;
  vitality?: string | null;
  next_review_at?: string | null;
}

export interface EntropyPulseSummary {
  reviewToday: EntropyPulseItem[];
  growing: EntropyPulseItem[];
  cooling: EntropyPulseItem[];
  highPriorityCount: number;
  coolingCount: number;
  dueReviewCount: number;
  hasLifecycleSignals: boolean;
}

export interface PatrolLifecycleSuggestion {
  kind: LifecyclePatrolKind;
  slug: string;
  title: string;
  description: string;
  suggestedAction: string;
  badge: string;
}

export function buildEntropyPulseSummary(
  pages: WikiPageSummary[],
  options: EntropyPulseOptions = {},
): EntropyPulseSummary {
  const now = options.now ?? Date.now();
  const limit = options.limit ?? DEFAULT_LIMIT;
  const growingCandidates: Array<EntropyPulseItem & { score: number }> = [];
  const reviewCandidates: Array<EntropyPulseItem & { score: number }> = [];
  const coolingCandidates: Array<EntropyPulseItem & { score: number }> = [];
  let highPriorityCount = 0;
  let coolingCount = 0;
  let dueReviewCount = 0;
  let hasLifecycleSignals = false;

  for (const page of pages) {
    const priority = normalizeLifecycleValue(page.priority);
    const vitality = normalizeLifecycleValue(page.vitality);
    const hasPriority = Boolean(priority);
    const hasVitality = Boolean(vitality);
    const dueForReview = isDue(page.next_review_at, now);
    const expressed = Boolean(page.expressed_in?.length);
    const cooling = isCoolingSignal(priority, vitality, expressed);
    const growing = isGrowingSignal(priority, vitality);

    if (
      hasPriority ||
      hasVitality ||
      Boolean(page.priority_reason) ||
      Boolean(page.last_revisited_at) ||
      Boolean(page.next_review_at)
    ) {
      hasLifecycleSignals = true;
    }
    if (priority === "high") highPriorityCount += 1;
    if (dueForReview) dueReviewCount += 1;
    if (cooling) coolingCount += 1;

    if (growing) {
      growingCandidates.push({
        ...toPulseItem(page, "growing"),
        score: scoreGrowing(priority, vitality, expressed),
      });
    }
    if (!expressed && !cooling && (priority === "high" || vitality === "growing" || dueForReview)) {
      reviewCandidates.push({
        ...toPulseItem(page, dueForReview ? "review" : "priority"),
        score: scoreReview(priority, vitality, dueForReview),
      });
    }
    if (cooling) {
      coolingCandidates.push({
        ...toPulseItem(page, "cooling"),
        score: scoreCooling(priority, vitality),
      });
    }
  }

  return {
    reviewToday: rankAndLimit(reviewCandidates, limit),
    growing: rankAndLimit(growingCandidates, limit),
    cooling: rankAndLimit(coolingCandidates, limit),
    highPriorityCount,
    coolingCount,
    dueReviewCount,
    hasLifecycleSignals,
  };
}

export function countPatrolLifecycleSuggestions(report?: PatrolReport | null): number {
  const summary = report?.summary;
  if (!summary) return 0;
  return (
    (summary.stale_sparks ?? 0) +
    (summary.cooling_pages ?? 0) +
    (summary.unexpressed_high_priority ?? 0) +
    (summary.noise_candidates ?? 0)
  );
}

export function buildPatrolLifecycleSuggestions(
  report: PatrolReport | null | undefined,
  pages: WikiPageSummary[] = [],
  limit = DEFAULT_LIMIT,
): PatrolLifecycleSuggestion[] {
  if (!report?.issues.length) return [];
  const titleBySlug = new Map(
    pages.map((page) => [page.slug, page.title || page.slug] as const),
  );
  return report.issues
    .filter((issue): issue is PatrolIssue & { kind: LifecyclePatrolKind } =>
      isLifecyclePatrolKind(issue.kind),
    )
    .map((issue) => ({
      kind: issue.kind,
      slug: issue.page_slug,
      title: titleBySlug.get(issue.page_slug) ?? issue.page_slug,
      description: issue.description,
      suggestedAction: issue.suggested_action,
      badge: lifecycleBadge(issue.kind),
      score: lifecycleScore(issue.kind),
    }))
    .sort((a, b) => b.score - a.score || a.title.localeCompare(b.title))
    .slice(0, limit)
    .map(({ score: _score, ...suggestion }) => suggestion);
}

function isLifecyclePatrolKind(kind: PatrolIssue["kind"]): kind is LifecyclePatrolKind {
  return (LIFECYCLE_PATROL_KINDS as readonly string[]).includes(kind);
}

function lifecycleBadge(kind: LifecyclePatrolKind): string {
  if (kind === "unexpressed-high-priority") return "express";
  if (kind === "stale-spark") return "review";
  if (kind === "cooling-page") return "cooling";
  return "noise";
}

function lifecycleScore(kind: LifecyclePatrolKind): number {
  if (kind === "unexpressed-high-priority") return 90;
  if (kind === "stale-spark") return 75;
  if (kind === "cooling-page") return 60;
  return 50;
}

function normalizeLifecycleValue(value?: string | null): string | null {
  const normalized = value?.trim().toLowerCase();
  return normalized || null;
}

function isDue(value: string | null | undefined, now: number): boolean {
  if (!value) return false;
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) && timestamp <= now;
}

function isGrowingSignal(priority: string | null, vitality: string | null): boolean {
  if (priority === "high") return true;
  return vitality ? GROWING_VITALITY.has(vitality) : false;
}

function isCoolingSignal(
  priority: string | null,
  vitality: string | null,
  expressed: boolean,
): boolean {
  if (vitality && COOLING_VITALITY.has(vitality)) return true;
  return priority === "low" && !expressed;
}

function toPulseItem(
  page: WikiPageSummary,
  kind: "growing" | "priority" | "review" | "cooling",
): EntropyPulseItem {
  const priority = normalizeLifecycleValue(page.priority);
  const vitality = normalizeLifecycleValue(page.vitality);
  return {
    slug: page.slug,
    title: page.title || page.slug,
    reason: page.priority_reason?.trim() || fallbackReason(kind, priority, vitality),
    badge: badgeFor(kind, priority, vitality),
    priority,
    vitality,
    next_review_at: page.next_review_at,
  };
}

function fallbackReason(
  kind: "growing" | "priority" | "review" | "cooling",
  priority: string | null,
  vitality: string | null,
): string {
  if (kind === "review") return "到达 next_review_at，适合今天做一次轻量复盘。";
  if (kind === "cooling") {
    if (vitality === "archived") return "已经处于归档注意力状态，保留证据，不自动删除。";
    if (vitality === "noise") return "信号密度偏低，适合降噪或并入其他主题。";
    return "热度正在冷却，适合判断是否保留、合并或归档。";
  }
  if (priority === "high" && vitality === "growing") {
    return "高优先级且正在增长，适合继续观察或结晶。";
  }
  if (priority === "high") return "高优先级信号，适合优先复盘。";
  if (vitality === "seed") return "仍是种子状态，适合观察是否继续反复出现。";
  return "正在增长，适合继续观察共性和可表达方向。";
}

function badgeFor(
  kind: "growing" | "priority" | "review" | "cooling",
  priority: string | null,
  vitality: string | null,
): string {
  if (kind === "review") return "due";
  if (kind === "cooling") return vitality ?? priority ?? "cooling";
  if (priority === "high") return "high";
  return vitality ?? kind;
}

function scoreGrowing(
  priority: string | null,
  vitality: string | null,
  expressed: boolean,
): number {
  let score = 0;
  if (priority === "high") score += 80;
  if (vitality === "growing") score += 40;
  if (vitality === "seed") score += 20;
  if (expressed) score += 10;
  return score;
}

function scoreReview(
  priority: string | null,
  vitality: string | null,
  dueForReview: boolean,
): number {
  let score = 0;
  if (priority === "high") score += 90;
  if (vitality === "growing") score += 25;
  if (dueForReview) score += 45;
  return score;
}

function scoreCooling(priority: string | null, vitality: string | null): number {
  let score = 0;
  if (vitality === "noise") score += 90;
  if (vitality === "archived") score += 80;
  if (vitality === "cooling") score += 70;
  if (priority === "low") score += 30;
  return score;
}

function rankAndLimit<T extends EntropyPulseItem & { score: number }>(
  items: T[],
  limit: number,
): EntropyPulseItem[] {
  return items
    .sort((a, b) => b.score - a.score || a.title.localeCompare(b.title))
    .slice(0, limit)
    .map(({ score: _score, ...item }) => item);
}

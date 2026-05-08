/**
 * Inbox Queue Intelligence — pure decision layer that turns a raw
 * `InboxEntry` (plus its optional `IngestDecision`) into a
 * `QueueIntelligence` envelope describing *what the user should do
 * next*.
 *
 * The seven-rule ladder encodes the Q1 contract locked by Main:
 *
 *   r1: proposal_status === "pending"                → open_diff_preview
 *   r2: target_page_slug is set                      → update_existing
 *   r3: IngestDecision.kind === "content_duplicate"  → suggest_reject
 *   r4: IngestDecision.kind === "reused_approved"    → defer
 *   r5: IngestDecision.kind === "reused_after_reject"→ ask_first
 *   r6: new-raw + source_raw_id + no target slug     → create_new
 *   r7: ELSE                                         → ask_first
 *
 * Rules evaluate top-down; the first match wins. Each rule also
 * emits a `reason_code`, a one-line Chinese `why` (for the inline
 * pill hover), and an optional `why_long` body (for the
 * `InfoTooltip` on each row).
 *
 * Scoring is a cheap sortable heuristic — higher = sooner the user
 * should act. Freshness and resolved-status modifiers ride on top
 * of the base per-action score so already-resolved items sink to
 * the bottom of every group without reshuffling the group order.
 *
 * Pure functions only — no network, no React, no I/O. Safe to call
 * inside `useMemo`. Tests (when added) exercise each rule branch
 * plus score ordering in isolation.
 */

import type { InboxEntry } from "@/api/wiki/types";
import type { IngestDecision } from "@/lib/tauri";

/**
 * The six recommended actions surfaced in the Inbox queue UI.
 *
 * `defer` is a group key (for reused-approved items that don't need
 * a decision) and is NOT a user-selectable action in the single-row
 * workbench — it maps to a "leave it" UX.
 *
 * `open_diff_preview` is the Phase-2 continuation of `update_existing`
 * once a proposal has been generated; kept as its own action so the
 * list can prioritise "has diff waiting" above "still needs a target".
 */
export type RecommendedAction =
  | "open_diff_preview"
  | "update_existing"
  | "create_new"
  | "ask_first"
  | "suggest_reject"
  | "defer";

/**
 * Machine-readable tag for the *why* behind the recommendation.
 * Paired 1:1 with the rule that fired — useful for telemetry + for
 * rendering a stable icon/accent in the tooltip.
 */
export type ReasonCode =
  | "pending_proposal"
  | "has_target_slug"
  | "duplicate_content"
  | "already_approved"
  | "rejected_history"
  | "fresh_content"
  | "missing_signals";

export interface QueueIntelligence {
  /** 0-100, higher = surface sooner. Comparable across actions. */
  score: number;
  /** Which of the 5 visible groups this row lands in. */
  group_key: RecommendedAction;
  /** Primary recommended action (same as `group_key` for now). */
  recommended_action: RecommendedAction;
  /** Stable tag pairing the rule that fired to UI surfaces. */
  reason_code: ReasonCode;
  /** One-line Chinese explanation shown on the row's hover pill. */
  why: string;
  /** Optional longer-form Chinese body for the InfoTooltip. */
  why_long?: string;
  /**
   * When we can suggest a concrete target slug (from existing
   * `target_page_slug` or a proposed slug the server stamped on the
   * entry), surface it so the parent can pre-seed MaintainActionRadio.
   */
  target_candidate?: { slug: string; source: "target" | "proposed" };
  /**
   * Optional cohort marker — entries sharing the same
   * `source_raw_id` are in the same cohort. Post-processed by
   * `markCohorts`.
   */
  cohort_raw_id?: number;
}

/** Fixed group order — must match Main's locked spec. */
export const GROUP_ORDER: RecommendedAction[] = [
  "open_diff_preview",
  "update_existing",
  "create_new",
  "ask_first",
  "suggest_reject",
  "defer",
];

/**
 * Display metadata per group — icon name is a stable string; the
 * consumer component (`QueueGroupHeader` / `RecommendedActionBadge`)
 * maps it to the actual Lucide component. We avoid importing
 * `lucide-react` here so the intelligence module stays pure and
 * tree-shakable.
 */
export const GROUP_META: Record<
  RecommendedAction,
  {
    label: string;
    icon: "FileEdit" | "Edit" | "Plus" | "HelpCircle" | "XCircle" | "Archive";
    tone: "primary" | "info" | "success" | "warning" | "error" | "muted";
  }
> = {
  open_diff_preview: { label: "需要审阅 Diff", icon: "FileEdit", tone: "primary" },
  update_existing: { label: "建议更新到已有页", icon: "Edit", tone: "info" },
  create_new: { label: "建议新建页", icon: "Plus", tone: "success" },
  ask_first: { label: "先 Ask 确认", icon: "HelpCircle", tone: "warning" },
  suggest_reject: { label: "可能重复或低价值", icon: "XCircle", tone: "error" },
  defer: { label: "已复用，延后", icon: "Archive", tone: "muted" },
};

/* ── Base scores per action (pre-modifiers) ─────────────────────── */

const BASE_SCORE: Record<RecommendedAction, number> = {
  open_diff_preview: 90,
  update_existing: 70,
  create_new: 50,
  ask_first: 20,
  suggest_reject: 10,
  defer: 5,
};

/* ── Freshness + status modifiers ───────────────────────────────── */

const MS_PER_DAY = 86_400_000;

function ageInDays(createdAt: string): number | null {
  const then = Date.parse(createdAt);
  if (Number.isNaN(then)) return null;
  return Math.max(0, (Date.now() - then) / MS_PER_DAY);
}

/**
 * Freshness + resolved-status modifiers applied on top of the base
 * per-action score. Fresh (<24h) nudges +10; stale (>7d) drops -10;
 * anything the user already resolved lands at the bottom via -50 so
 * it doesn't pollute the pending queue.
 */
function applyModifiers(base: number, entry: InboxEntry): number {
  let score = base;
  const age = ageInDays(entry.created_at);
  if (age !== null) {
    if (age < 1) score += 10;
    if (age > 7) score -= 10;
  }
  if (entry.status === "approved" || entry.status === "rejected") {
    score -= 50;
  }
  return Math.max(0, Math.min(100, Math.round(score)));
}

/* ── The seven-rule ladder ──────────────────────────────────────── */

/**
 * Compute the queue-intelligence envelope for a single inbox entry.
 *
 * `decision` is the optional `IngestDecision` for the entry's raw
 * (populated via the raw-detail fetch in the parent — this function
 * never calls the network). When absent, the duplicate / reused
 * branches silently fall through to later rules.
 */
export function computeQueueIntelligence(
  entry: InboxEntry,
  decision?: IngestDecision | null,
): QueueIntelligence {
  const ageDays = ageInDays(entry.created_at);
  const ageLabel = formatAgeLabel(ageDays);

  // r1: W2 proposal waiting for review — always top of queue.
  if (entry.proposal_status === "pending") {
    const slug = entry.target_page_slug ?? "";
    return {
      recommended_action: "open_diff_preview",
      group_key: "open_diff_preview",
      reason_code: "pending_proposal",
      score: applyModifiers(BASE_SCORE.open_diff_preview, entry),
      why: "W2 提案待审",
      why_long: [
        "上一步已生成 LLM 合并草稿，等待你确认 diff。",
        slug ? `目标页: ${slug}` : undefined,
        ageLabel ? `生成于 ${ageLabel}` : undefined,
      ]
        .filter(Boolean)
        .join(" · "),
      target_candidate: slug ? { slug, source: "target" } : undefined,
    };
  }

  // r2: user (or server) already chose a target page — recommend
  // Phase-1 update_existing so the picker lands pre-seeded.
  if (entry.target_page_slug) {
    return {
      recommended_action: "update_existing",
      group_key: "update_existing",
      reason_code: "has_target_slug",
      score: applyModifiers(BASE_SCORE.update_existing, entry),
      why: "已锁定目标页",
      why_long: `建议合并到已有页 · 目标: ${entry.target_page_slug}`,
      target_candidate: {
        slug: entry.target_page_slug,
        source: "target",
      },
    };
  }

  // r3-r5: branch on IngestDecision (only meaningful when present).
  if (decision) {
    if (decision.kind === "content_duplicate") {
      return {
        recommended_action: "suggest_reject",
        group_key: "suggest_reject",
        reason_code: "duplicate_content",
        score: applyModifiers(BASE_SCORE.suggest_reject, entry),
        why: "疑似重复内容",
        why_long: `相同内容已存在 raw #${String(decision.matching_raw_id).padStart(5, "0")}（URL: ${decision.matching_url}）建议拒绝以避免重复`,
      };
    }
    if (decision.kind === "reused_approved") {
      return {
        recommended_action: "defer",
        group_key: "defer",
        reason_code: "already_approved",
        score: applyModifiers(BASE_SCORE.defer, entry),
        why: "已被批准复用",
        why_long: `相同 URL 的素材此前已批准进入 wiki · ${decision.reason}`,
      };
    }
    if (decision.kind === "reused_after_reject") {
      return {
        recommended_action: "ask_first",
        group_key: "ask_first",
        reason_code: "rejected_history",
        score: applyModifiers(BASE_SCORE.ask_first + 10, entry),
        why: "历史曾被拒绝",
        why_long: `相同 URL 此前被拒绝 · ${decision.reason} · 建议先 Ask 复核再决策`,
      };
    }
  }

  // r6: fresh new-raw with a source raw — default to create_new.
  if (
    entry.kind === "new-raw" &&
    entry.source_raw_id != null &&
    !entry.target_page_slug
  ) {
    const slug = entry.proposed_wiki_slug ?? null;
    return {
      recommended_action: "create_new",
      group_key: "create_new",
      reason_code: "fresh_content",
      score: applyModifiers(BASE_SCORE.create_new, entry),
      why: "新素材，可新建页",
      why_long: [
        `基于 raw #${String(entry.source_raw_id).padStart(5, "0")} 可新建 wiki 页`,
        slug ? `建议 slug: ${slug}` : undefined,
        ageLabel ? `入队于 ${ageLabel}` : undefined,
      ]
        .filter(Boolean)
        .join(" · "),
      target_candidate: slug
        ? { slug, source: "proposed" }
        : undefined,
    };
  }

  // r7: catch-all — tell the user to Ask first.
  return {
    recommended_action: "ask_first",
    group_key: "ask_first",
    reason_code: "missing_signals",
    score: applyModifiers(BASE_SCORE.ask_first, entry),
    why: "信号不足，先 Ask",
    why_long: [
      "没有明显的目标页或重复信号，建议先去 Ask 复核再决策。",
      ageLabel ? `入队于 ${ageLabel}` : undefined,
    ]
      .filter(Boolean)
      .join(" · "),
  };
}

function formatAgeLabel(ageDays: number | null): string | null {
  if (ageDays === null) return null;
  if (ageDays < 1 / 24) return "刚刚";
  if (ageDays < 1) return `${Math.floor(ageDays * 24)} 小时前`;
  if (ageDays < 30) return `${Math.floor(ageDays)} 天前`;
  return `${Math.floor(ageDays / 30)} 个月前`;
}

/* ── Grouping + sorting ─────────────────────────────────────────── */

/**
 * Bucket entries by `group_key`, sort each bucket by `score desc`
 * (ties broken by id desc so the freshly-ingested task wins), then
 * emit the groups in `GROUP_ORDER`, dropping any that are empty.
 *
 * The result is ready to render as a flat list with sticky group
 * headers — callers map over `groups` and emit
 * `<QueueGroupHeader />` + entry rows for each.
 */
export function groupAndSortByAction<
  T extends InboxEntry & { intelligence: QueueIntelligence },
>(
  entries: T[],
): Array<{
  action: RecommendedAction;
  meta: (typeof GROUP_META)[RecommendedAction];
  entries: T[];
}> {
  const buckets = new Map<RecommendedAction, T[]>();
  for (const entry of entries) {
    const key = entry.intelligence.group_key;
    const bucket = buckets.get(key) ?? [];
    bucket.push(entry);
    buckets.set(key, bucket);
  }

  const result: Array<{
    action: RecommendedAction;
    meta: (typeof GROUP_META)[RecommendedAction];
    entries: T[];
  }> = [];
  for (const action of GROUP_ORDER) {
    const bucket = buckets.get(action);
    if (!bucket || bucket.length === 0) continue;
    bucket.sort((a, b) => {
      const delta = b.intelligence.score - a.intelligence.score;
      if (delta !== 0) return delta;
      return b.id - a.id;
    });
    result.push({ action, meta: GROUP_META[action], entries: bucket });
  }
  return result;
}

export type EntropyInboxGroupKey =
  | "review_today"
  | "growing"
  | "crystallize"
  | "mergeable"
  | "needs_decision"
  | "cooling";

export interface EntropyInboxGroupMeta {
  label: string;
  reason: string;
  color: string;
  priority?: string;
  bulkAccept: boolean;
}

export type EntropyPriority = "low" | "medium" | "high";
export type EntropyVitality =
  | "seed"
  | "growing"
  | "stable"
  | "cooling"
  | "archived";

export interface EntropyLifecycleDefault {
  priority: EntropyPriority;
  vitality: EntropyVitality;
  reviewAfterDays: number;
}

export const ENTROPY_GROUP_ORDER: EntropyInboxGroupKey[] = [
  "review_today",
  "growing",
  "crystallize",
  "mergeable",
  "needs_decision",
  "cooling",
];

export const ENTROPY_GROUP_META: Record<
  EntropyInboxGroupKey,
  EntropyInboxGroupMeta
> = {
  review_today: {
    label: "今天只审这些",
    reason: "已有合并草稿或高确定性事项，先处理小批量，避免被完整队列拖住。",
    color: "#C65D22",
    priority: "高确定性",
    bulkAccept: true,
  },
  growing: {
    label: "长期反复出现的方向",
    reason: "多个素材指向同一主题，说明它可能不只是一次性收藏。",
    color: "#5B6FC7",
    priority: "识别共性",
    bulkAccept: true,
  },
  crystallize: {
    label: "值得结晶成主题",
    reason: "素材信号足够独立，可以沉淀成新的 wiki 页面。",
    color: "#1D9E75",
    priority: "结晶",
    bulkAccept: true,
  },
  mergeable: {
    label: "可合并到已有主题",
    reason: "已有明确目标页，优先把碎片并入稳定知识结构。",
    color: "#2A6BB8",
    priority: "去重合并",
    bulkAccept: true,
  },
  needs_decision: {
    label: "需要先判断",
    reason: "信号不足或历史判断有冲突，先进入 Ask 澄清再决定。",
    color: "#9A6A18",
    priority: "先问清楚",
    bulkAccept: false,
  },
  cooling: {
    label: "冷却 / 可归档",
    reason: "疑似重复、已复用或低价值，不自动删除，只建议降噪暂缓。",
    color: "#767A82",
    priority: "降噪",
    bulkAccept: false,
  },
};

const ENTROPY_LIFECYCLE_DEFAULTS: Record<
  EntropyInboxGroupKey,
  EntropyLifecycleDefault
> = {
  review_today: { priority: "high", vitality: "growing", reviewAfterDays: 7 },
  growing: { priority: "high", vitality: "growing", reviewAfterDays: 7 },
  crystallize: { priority: "medium", vitality: "seed", reviewAfterDays: 30 },
  mergeable: { priority: "medium", vitality: "stable", reviewAfterDays: 30 },
  needs_decision: { priority: "medium", vitality: "seed", reviewAfterDays: 14 },
  cooling: { priority: "low", vitality: "cooling", reviewAfterDays: 45 },
};

export function defaultLifecycleForEntropyGroup(
  key: EntropyInboxGroupKey,
): EntropyLifecycleDefault {
  return ENTROPY_LIFECYCLE_DEFAULTS[key];
}

const DEFAULT_TODAY_LIMIT = 3;
const REVIEW_TODAY_MIN_SCORE = 85;

export interface EntropyInboxGroup<
  T extends InboxEntry & { intelligence: QueueIntelligence },
> {
  key: EntropyInboxGroupKey;
  meta: EntropyInboxGroupMeta;
  entries: T[];
}

function compareQueueEntries<
  T extends InboxEntry & { intelligence: QueueIntelligence },
>(a: T, b: T): number {
  const scoreDelta = b.intelligence.score - a.intelligence.score;
  if (scoreDelta !== 0) return scoreDelta;
  return b.id - a.id;
}

function directionKey(
  entry: InboxEntry & { intelligence: QueueIntelligence },
): string | null {
  const slug =
    entry.intelligence.target_candidate?.slug ??
    entry.target_page_slug ??
    entry.proposed_wiki_slug ??
    null;
  const normalized = slug?.trim();
  return normalized ? normalized : null;
}

function isCoolingAction(action: RecommendedAction): boolean {
  return action === "suggest_reject" || action === "defer";
}

function isReviewTodayCandidate(
  entry: InboxEntry & { intelligence: QueueIntelligence },
): boolean {
  return (
    entry.intelligence.recommended_action === "open_diff_preview" ||
    entry.intelligence.score >= REVIEW_TODAY_MIN_SCORE
  );
}

function entropyKeyForEntry(
  entry: InboxEntry & { intelligence: QueueIntelligence },
  reviewIds: Set<number>,
  repeatedDirectionKeys: Set<string>,
): EntropyInboxGroupKey {
  const action = entry.intelligence.recommended_action;
  if (reviewIds.has(entry.id)) return "review_today";
  if (isCoolingAction(action)) return "cooling";

  const key = directionKey(entry);
  if (key && repeatedDirectionKeys.has(key)) return "growing";

  if (action === "create_new") return "crystallize";
  if (action === "update_existing" || action === "open_diff_preview") {
    return "mergeable";
  }
  return "needs_decision";
}

export function groupByEntropyJudgment<
  T extends InboxEntry & { intelligence: QueueIntelligence },
>(
  entries: T[],
  options: { todayLimit?: number } = {},
): EntropyInboxGroup<T>[] {
  const pending = entries
    .filter((entry) => entry.status === "pending")
    .slice()
    .sort(compareQueueEntries);

  const todayLimit = Math.max(0, options.todayLimit ?? DEFAULT_TODAY_LIMIT);
  const reviewIds = new Set(
    pending
      .filter(
        (entry) =>
          !isCoolingAction(entry.intelligence.recommended_action) &&
          isReviewTodayCandidate(entry),
      )
      .slice(0, todayLimit)
      .map((entry) => entry.id),
  );

  const directionCounts = new Map<string, number>();
  for (const entry of pending) {
    if (isCoolingAction(entry.intelligence.recommended_action)) continue;
    const key = directionKey(entry);
    if (!key) continue;
    directionCounts.set(key, (directionCounts.get(key) ?? 0) + 1);
  }
  const repeatedDirectionKeys = new Set(
    [...directionCounts.entries()]
      .filter(([, count]) => count > 1)
      .map(([key]) => key),
  );

  const buckets = new Map<EntropyInboxGroupKey, T[]>();
  for (const entry of pending) {
    const key = entropyKeyForEntry(entry, reviewIds, repeatedDirectionKeys);
    const bucket = buckets.get(key) ?? [];
    bucket.push(entry);
    buckets.set(key, bucket);
  }

  const result: EntropyInboxGroup<T>[] = [];
  for (const key of ENTROPY_GROUP_ORDER) {
    const bucket = buckets.get(key);
    if (!bucket || bucket.length === 0) continue;
    bucket.sort(compareQueueEntries);
    result.push({ key, meta: ENTROPY_GROUP_META[key], entries: bucket });
  }
  return result;
}

/**
 * Mark cohorts — entries sharing the same `source_raw_id` have their
 * `intelligence.cohort_raw_id` set on the *same* object reference. The
 * UI shows a "同源 × N" chip on any entry where a cohort of size > 1
 * exists. Mutates in place (hence the void return) because the
 * consumer already holds a stable reference post-memoisation and the
 * field is additive.
 */
export function markCohorts<
  T extends InboxEntry & { intelligence: QueueIntelligence },
>(entries: T[]): void {
  const counts = new Map<number, number>();
  for (const entry of entries) {
    if (entry.source_raw_id != null) {
      counts.set(
        entry.source_raw_id,
        (counts.get(entry.source_raw_id) ?? 0) + 1,
      );
    }
  }
  for (const entry of entries) {
    if (entry.source_raw_id != null) {
      const size = counts.get(entry.source_raw_id) ?? 0;
      if (size > 1) {
        entry.intelligence.cohort_raw_id = entry.source_raw_id;
      }
    }
  }
}

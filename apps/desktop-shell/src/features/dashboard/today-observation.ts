/**
 * Slice E28.3 — pure observation generator for the Home hero card.
 *
 * Scans current Pulse state and picks ONE observation worth
 * surfacing. Rules are table-driven + ordered by priority: first
 * match wins. Adding a new observation = appending to OBSERVATIONS.
 *
 * Why pure: tests pin the contract without React; same fn could
 * later feed e.g. a status-bar tooltip or the command palette
 * empty state.
 */

const DAY_MS = 86_400_000;

export interface ObservationInputs {
  now: number;
  pendingInbox: number;
  gitDirty: boolean;
  gitChangedCount: number;
  /** ISO ms of latest git audit entry; null if never. */
  lastGitCommitMs: number | null;
  lifecyclePatrolCount: number;
  /** Count of pages with `expressed_in.length > 0 && created_at < 7 days`. */
  recentExpressionCount7d: number;
  /** Total wiki pages. Used for the "knowledge but no export" nudge. */
  totalPages: number;
}

export type ObservationId =
  | "vault-stale"
  | "inbox-backlog"
  | "patrol-issues"
  | "express-streak"
  | "knowledge-no-export"
  | "default";

export interface Observation {
  id: ObservationId;
  /** Display message, plain text. May contain interpolated counts. */
  message: string;
  /** Optional CTA route. */
  href?: string;
  /** Tone for the hero card tint. */
  tone: "warning" | "primary" | "success" | "neutral";
}

const OBSERVATIONS: ReadonlyArray<{
  id: ObservationId;
  match: (i: ObservationInputs) => boolean;
  build: (i: ObservationInputs) => Observation;
}> = [
  {
    id: "vault-stale",
    match: (i) => {
      if (!i.gitDirty || i.gitChangedCount < 20) return false;
      if (i.lastGitCommitMs == null) return true;
      return i.now - i.lastGitCommitMs >= DAY_MS;
    },
    build: (i) => {
      const hours = i.lastGitCommitMs
        ? Math.floor((i.now - i.lastGitCommitMs) / (60 * 60 * 1000))
        : null;
      const stale =
        hours == null
          ? ""
          : hours < 48
            ? `堆了 ${hours} 小时`
            : `堆了 ${Math.floor(hours / 24)} 天`;
      return {
        id: "vault-stale",
        message: `${i.gitChangedCount} 个 Vault 改动${stale ? " " + stale : ""}，要 checkpoint 吗？`,
        href: "/connections#git",
        tone: "warning",
      };
    },
  },
  {
    id: "inbox-backlog",
    match: (i) => i.pendingInbox >= 10,
    build: (i) => ({
      id: "inbox-backlog",
      message: `Inbox 有 ${i.pendingInbox} 条待审了，先收一下`,
      href: "/inbox",
      tone: "warning",
    }),
  },
  {
    id: "patrol-issues",
    match: (i) => i.lifecyclePatrolCount >= 5,
    build: (i) => ({
      id: "patrol-issues",
      message: `${i.lifecyclePatrolCount} 个页面建议进 patrol 看一下生命周期`,
      href: "/wiki",
      tone: "warning",
    }),
  },
  {
    id: "express-streak",
    match: (i) => i.recentExpressionCount7d >= 3,
    build: (i) => ({
      id: "express-streak",
      message: `这周你已经表达了 ${i.recentExpressionCount7d} 次！可以试试结晶一篇`,
      href: "/drafts",
      tone: "success",
    }),
  },
  {
    id: "knowledge-no-export",
    match: (i) => i.totalPages >= 10 && i.recentExpressionCount7d === 0,
    build: (i) => ({
      id: "knowledge-no-export",
      message: `积了 ${i.totalPages} 页知识，但本周还没表达过，试试导出一篇 draft`,
      href: "/drafts",
      tone: "primary",
    }),
  },
];

const DEFAULT_OBSERVATION: Observation = {
  id: "default",
  message: "今天没有特别需要看的事，继续摄入或表达 ✨",
  tone: "neutral",
};

export function pickObservation(input: ObservationInputs): Observation {
  for (const rule of OBSERVATIONS) {
    if (rule.match(input)) return rule.build(input);
  }
  return DEFAULT_OBSERVATION;
}

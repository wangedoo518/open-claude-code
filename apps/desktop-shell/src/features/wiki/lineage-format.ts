/**
 * P1 sprint — pure helpers for the lineage-explorer UI surfaces.
 *
 * These are shared by `WikiLineagePanel`, `InboxLineageSummary`, and
 * `RawLineageBadge` so the icon / tone / display-title / upstream-
 * downstream formatting logic stays in one place. Worker A owns the
 * wire types (`LineageEvent` / `LineageRef` / `LineageEventType`) in
 * `@/lib/tauri`; we consume them by type only — no runtime imports
 * happen here so this file stays purely functional and testable.
 *
 * If Worker A's exports haven't landed yet, the consuming UI files
 * inline the type shapes and import this module for the helpers —
 * the helpers themselves take `any`-compatible structural parameters
 * by relying on discriminated unions (kind / event_type).
 */
import {
  CheckCircle2,
  FileText,
  GitMerge,
  Inbox as InboxIcon,
  Link2,
  MessageSquare,
  Sparkles,
  XCircle,
  type LucideIcon,
} from "lucide-react";

// ── Contract types (re-declared locally so this file does not depend
// on `@/lib/tauri` being ready yet — Worker A's exports will eventually
// match these). When Worker A lands, these definitions remain compatible
// because the wire shapes are fixed by the P1 contract.
export type LineageEventType =
  | "raw_written"
  | "inbox_appended"
  | "proposal_generated"
  | "wiki_page_applied"
  | "combined_wiki_page_applied"
  | "inbox_rejected"
  | "wechat_message_received"
  | "url_ingested";

export type LineageRef =
  | { kind: "raw"; id: number }
  | { kind: "inbox"; id: number }
  | { kind: "wiki_page"; slug: string; title?: string | null }
  | { kind: "wechat_message"; event_key: string }
  | { kind: "url_source"; canonical: string };

export interface LineageEventLike {
  event_id: string;
  event_type: LineageEventType;
  timestamp_ms: number;
  upstream: LineageRef[];
  downstream: LineageRef[];
  display_title: string;
  metadata: Record<string, unknown>;
}

// ── Tone palette ────────────────────────────────────────────────────
// Maps each event type to a semantic tone used for the small badge
// wrapper. The spec calls for:
//   - info-blue      source events (raw_written / wechat / url)
//   - neutral-gray   intermediate events (inbox_appended / proposal)
//   - success-green  applied events (wiki / combined_wiki)
//   - warning-amber  inbox_rejected
export type LineageTone = "source" | "neutral" | "applied" | "warning";

export function toneFor(eventType: LineageEventType): LineageTone {
  switch (eventType) {
    case "raw_written":
    case "wechat_message_received":
    case "url_ingested":
      return "source";
    case "inbox_appended":
    case "proposal_generated":
      return "neutral";
    case "wiki_page_applied":
    case "combined_wiki_page_applied":
      return "applied";
    case "inbox_rejected":
      return "warning";
    default:
      return "neutral";
  }
}

/**
 * Tailwind class pack per tone — kept in one place so the three
 * surfaces read identically. The neutral tone intentionally has no
 * background tint (just muted text) so mid-chain events recede.
 */
export const TONE_CLASSES: Record<LineageTone, { pill: string; text: string }> =
  {
    source: {
      pill: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
      text: "text-blue-600 dark:text-blue-400",
    },
    neutral: {
      pill: "bg-muted/50 text-muted-foreground",
      text: "text-muted-foreground",
    },
    applied: {
      pill: "bg-green-500/10 text-green-600 dark:text-green-400",
      text: "text-green-600 dark:text-green-400",
    },
    warning: {
      pill: "bg-yellow-500/10 text-yellow-600 dark:text-yellow-400",
      text: "text-yellow-600 dark:text-yellow-400",
    },
  };

// ── Icons ────────────────────────────────────────────────────────────
// Each event type maps to a Lucide icon; returned as a component
// reference so callers can size / colour it via className.
export function iconFor(eventType: LineageEventType): LucideIcon {
  switch (eventType) {
    case "raw_written":
      return FileText;
    case "wechat_message_received":
      return MessageSquare;
    case "url_ingested":
      return Link2;
    case "inbox_appended":
      return InboxIcon;
    case "proposal_generated":
      return Sparkles;
    case "wiki_page_applied":
      return CheckCircle2;
    case "combined_wiki_page_applied":
      return GitMerge;
    case "inbox_rejected":
      return XCircle;
    default:
      return FileText;
  }
}

/**
 * Legacy name from the brief ("iconNameFor"). Kept as an alias so
 * downstream callers can match either spelling — returns the same
 * `LucideIcon` reference as `iconFor`.
 */
export const iconNameFor = iconFor;

// ── Display title ────────────────────────────────────────────────────
// The backend already populates `event.display_title`, but when it is
// missing or empty we derive a sensible fallback from the event type
// and metadata so the UI never renders an empty row.
export function displayTitleFor(event: LineageEventLike): string {
  if (event.display_title && event.display_title.trim().length > 0) {
    return event.display_title;
  }
  // Fallbacks keyed off event type — intentionally short.
  switch (event.event_type) {
    case "raw_written":
      return "Raw 入库";
    case "wechat_message_received":
      return "WeChat 捕获";
    case "url_ingested":
      return "URL 入库";
    case "inbox_appended":
      return "Inbox 任务生成";
    case "proposal_generated":
      return "AI 预生成草稿";
    case "wiki_page_applied":
      return "Wiki 页应用";
    case "combined_wiki_page_applied":
      return "合并应用到 Wiki";
    case "inbox_rejected":
      return "任务已拒绝";
    default:
      return "事件";
  }
}

// ── Upstream / downstream formatting ────────────────────────────────
/**
 * Render a single `LineageRef` as short human-readable string. Keeps
 * copy compact (badges are tight) and skips long slugs.
 */
export function refLabel(ref: LineageRef): string {
  switch (ref.kind) {
    case "raw":
      return `raw #${String(ref.id).padStart(5, "0")}`;
    case "inbox":
      return `inbox #${ref.id}`;
    case "wiki_page":
      return ref.title && ref.title.trim().length > 0
        ? ref.title
        : ref.slug;
    case "wechat_message":
      return `WeChat (${ref.event_key.slice(0, 8)}…)`;
    case "url_source": {
      // Trim scheme + host-only shortening for readability.
      try {
        const url = new URL(ref.canonical);
        return url.hostname + (url.pathname === "/" ? "" : url.pathname);
      } catch {
        return ref.canonical;
      }
    }
    default:
      return String(ref);
  }
}

/**
 * Format an upstream / downstream list into a concatenated string.
 * When the list has > 3 items, the tail is collapsed into "+N 更多".
 */
export function formatRefs(refs: LineageRef[], max = 3): string {
  if (!refs || refs.length === 0) return "—";
  if (refs.length <= max) return refs.map(refLabel).join(" · ");
  const shown = refs.slice(0, max).map(refLabel).join(" · ");
  const more = refs.length - max;
  return `${shown} · +${more} 更多`;
}

/** Alias reflecting the task brief ("formatUpstream"). */
export function formatUpstream(refs: LineageRef[]): string {
  return formatRefs(refs);
}

/** Alias reflecting the task brief ("formatDownstream"). */
export function formatDownstream(refs: LineageRef[]): string {
  return formatRefs(refs);
}

// ── Relative time ────────────────────────────────────────────────────
/**
 * Render a unix-ms timestamp as a short 中文 relative string. Lives
 * here (rather than importing WeChat's copy) so the module stays
 * self-contained per the P1 brief's "pure helpers" rule.
 */
export function formatRelativeTime(
  timestampMs: number,
  nowMs: number = Date.now(),
): string {
  const delta = nowMs - timestampMs;
  if (delta < 0) return "刚刚";
  if (delta < 60_000) return "刚刚";
  if (delta < 60 * 60_000) return `${Math.floor(delta / 60_000)} 分钟前`;
  if (delta < 24 * 60 * 60_000) {
    return `${Math.floor(delta / 60 / 60_000)} 小时前`;
  }
  return `${Math.floor(delta / 24 / 60 / 60_000)} 天前`;
}

// ── Combined-apply detection ────────────────────────────────────────
/**
 * True when the event is a combined apply — the UI renders these as
 * a single collapsible row instead of fanning out their upstream list.
 */
export function isCombinedApply(event: LineageEventLike): boolean {
  return event.event_type === "combined_wiki_page_applied";
}

/**
 * Helper: the count of inbox entries merged in a combined-apply event.
 * Counts `inbox` upstream refs; falls back to the overall upstream
 * length when none are marked as inbox.
 */
export function combinedApplyInboxCount(event: LineageEventLike): number {
  const inbox = event.upstream.filter((r) => r.kind === "inbox").length;
  return inbox > 0 ? inbox : event.upstream.length;
}

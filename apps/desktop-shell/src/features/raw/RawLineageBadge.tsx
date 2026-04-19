/**
 * RawLineageBadge — P1 sprint, Worker B surface #3.
 *
 * Small downstream-status pill shown inline in the meta row of each
 * `RawLibraryPage` list item. Collapses the raw's downstream lineage
 * into a single glanceable state:
 *
 *   - 已入队 inbox #5   (inbox_appended event present, no apply yet)
 *   - 已应用到 {slug}   (wiki_page_applied event present)
 *   - 无下游            (no downstream events — muted)
 *   - N 条下游          (multiple events — InfoTooltip expands full list)
 *
 * The badge intentionally stays compact (a single line of small-caps
 * muted text) so it doesn't compete with the existing source badge,
 * date, and size pills in the raw row meta. Worker A contract:
 * `fetchRawLineage(id)` returns `RawLineageResponse` with a flat
 * `events[]` array.
 */

import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Inbox as InboxIcon } from "lucide-react";

import { InfoTooltip } from "@/components/ui/info-tooltip";
import { cn } from "@/lib/utils";
import {
  displayTitleFor,
  formatRelativeTime,
  refLabel,
  toneFor,
  TONE_CLASSES,
} from "@/features/wiki/lineage-format";

// Worker A's canonical wrapper + wire type.
import {
  fetchRawLineage,
  type LineageEvent,
  type LineageRef,
} from "@/lib/tauri";

// ── Component ───────────────────────────────────────────────────────

export interface RawLineageBadgeProps {
  rawId: number;
}

export function RawLineageBadge({ rawId }: RawLineageBadgeProps) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["raw", "lineage", rawId] as const,
    queryFn: () => fetchRawLineage(rawId),
    staleTime: 60_000,
  });

  if (isLoading) {
    return (
      <span
        className="inline-flex h-4 w-16 shrink-0 animate-pulse rounded-full bg-muted/30"
        aria-hidden
      />
    );
  }
  if (isError) {
    // Silent failure — the raw row is already dense; a noisy error
    // pill here would distract. The tooltip exposes the fact that
    // the fetch failed for devs who hover.
    return (
      <span
        className="shrink-0 rounded-full px-1.5 py-0.5 text-muted-foreground/40"
        style={{ fontSize: 10 }}
        title="lineage 加载失败"
      >
        —
      </span>
    );
  }

  const events = data?.events ?? [];

  // ── No downstream: muted "无下游" pill. ────────────────────────
  if (events.length === 0) {
    return (
      <span
        className="shrink-0 rounded-full bg-muted/30 px-1.5 py-0.5 text-muted-foreground/60"
        style={{ fontSize: 10 }}
        title="此素材尚未产生任何下游事件"
      >
        无下游
      </span>
    );
  }

  // Look for the "terminal" state first — an applied event beats an
  // inbox_appended one for the single-line summary.
  const appliedEvent = events.find(
    (e) =>
      e.event_type === "wiki_page_applied" ||
      e.event_type === "combined_wiki_page_applied",
  );
  const inboxEvent = events.find((e) => e.event_type === "inbox_appended");

  // ── Single event summary ────────────────────────────────────────
  if (events.length === 1) {
    const ev = events[0];
    return renderSingle(ev, appliedEvent, inboxEvent);
  }

  // ── Multi-event: a single summary pill + InfoTooltip w/ full list.
  const summary =
    appliedEvent != null
      ? summarizeApplied(appliedEvent)
      : inboxEvent != null
        ? summarizeInbox(inboxEvent)
        : `${events.length} 条下游`;

  return (
    <span
      className="inline-flex shrink-0 items-center gap-0.5"
      style={{ fontSize: 10 }}
    >
      <span
        className={cn(
          "rounded-full px-1.5 py-0.5",
          TONE_CLASSES[toneFor(appliedEvent?.event_type ?? "inbox_appended")]
            .pill,
        )}
      >
        {summary}
      </span>
      {events.length > 1 && (
        <InfoTooltip side="top">
          <div className="space-y-1 text-[11px]">
            <div className="font-medium text-foreground">
              {events.length} 条下游事件
            </div>
            <ul className="space-y-0.5 text-muted-foreground">
              {events.slice(0, 6).map((e) => (
                <li key={e.event_id} className="flex items-baseline gap-1">
                  <span>·</span>
                  <span>{displayTitleFor(e)}</span>
                  <span className="text-muted-foreground/60">
                    {formatRelativeTime(e.timestamp_ms)}
                  </span>
                </li>
              ))}
              {events.length > 6 && (
                <li className="text-muted-foreground/60">
                  …还有 {events.length - 6} 条
                </li>
              )}
            </ul>
          </div>
        </InfoTooltip>
      )}
    </span>
  );
}

/* ── Single event rendering helper ─────────────────────────────── */

function renderSingle(
  ev: LineageEvent,
  appliedEvent: LineageEvent | undefined,
  inboxEvent: LineageEvent | undefined,
) {
  const tone = toneFor(ev.event_type);
  const toneCls = TONE_CLASSES[tone];

  if (appliedEvent && appliedEvent === ev) {
    return (
      <span
        className={cn(
          "inline-flex shrink-0 items-center gap-0.5 rounded-full px-1.5 py-0.5",
          toneCls.pill,
        )}
        style={{ fontSize: 10 }}
        title={displayTitleFor(ev)}
      >
        <ArrowRight className="size-2.5" aria-hidden />
        {summarizeApplied(ev)}
      </span>
    );
  }

  if (inboxEvent && inboxEvent === ev) {
    return (
      <span
        className={cn(
          "inline-flex shrink-0 items-center gap-0.5 rounded-full px-1.5 py-0.5",
          toneCls.pill,
        )}
        style={{ fontSize: 10 }}
        title={displayTitleFor(ev)}
      >
        <InboxIcon className="size-2.5" aria-hidden />
        {summarizeInbox(ev)}
      </span>
    );
  }

  // Fallback: just the display title.
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center rounded-full px-1.5 py-0.5",
        toneCls.pill,
      )}
      style={{ fontSize: 10 }}
    >
      {displayTitleFor(ev)}
    </span>
  );
}

/* ── Summaries (short text for the pill) ──────────────────────── */

function summarizeApplied(ev: LineageEvent): string {
  // Prefer the downstream wiki_page ref's title/slug; fall back to
  // any wiki_page found anywhere on the event, then to the generic
  // display title.
  const wikiRef =
    findRef(ev.downstream, "wiki_page") ?? findRef(ev.upstream, "wiki_page");
  if (wikiRef && wikiRef.kind === "wiki_page") {
    return `已应用到 ${refLabel(wikiRef)}`;
  }
  return "已应用";
}

function summarizeInbox(ev: LineageEvent): string {
  const inboxRef =
    findRef(ev.downstream, "inbox") ?? findRef(ev.upstream, "inbox");
  if (inboxRef && inboxRef.kind === "inbox") {
    return `已入队 inbox #${inboxRef.id}`;
  }
  return "已入队";
}

function findRef(
  refs: LineageRef[],
  kind: LineageRef["kind"],
): LineageRef | undefined {
  return refs.find((r) => r.kind === kind);
}

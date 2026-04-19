/**
 * InboxLineageSummary — P1 sprint, Worker B surface #2.
 *
 * Inserted at the bottom of the §1 Evidence section in the inbox
 * Workbench's `EntryDetail`. Shows a compact two-block summary of
 * the lineage events flowing in and out of this inbox task:
 *
 *   ┌─ 上游 ──────────────────────────┐
 *   │ ↑ 来自 raw #234 · 微信链接        │
 *   │ ↑ 来自 WeChat 捕获 (2 小时前)    │
 *   ├─ 下游 ──────────────────────────┤
 *   │ ↓ 提案 pending (1 小时前)        │
 *   │ ↓ 尚未应用                       │
 *   └────────────────────────────────┘
 *
 * Height is capped (max-h) so Evidence doesn't jump when the backend
 * returns a long lineage chain. Empty arms render one muted line
 * ("尚未有上游 / 尚未下游") so the block always occupies a stable
 * footprint.
 *
 * Worker A contract: `fetchInboxLineage(id)` returns an
 * `InboxLineageResponse` with separate `upstream_events` /
 * `downstream_events` arrays (not a single flat list — the shape is
 * already split to match this UI's two arms).
 */

import { useQuery } from "@tanstack/react-query";
import { ArrowDownToLine, ArrowUpFromLine } from "lucide-react";

import { cn } from "@/lib/utils";
import {
  displayTitleFor,
  formatRefs,
  formatRelativeTime,
  iconFor,
  toneFor,
  TONE_CLASSES,
} from "@/features/wiki/lineage-format";

// Worker A's canonical wrapper + wire type.
import {
  fetchInboxLineage,
  type LineageEvent,
} from "@/lib/tauri";

// ── Component ───────────────────────────────────────────────────────

export interface InboxLineageSummaryProps {
  entryId: number;
}

export function InboxLineageSummary({ entryId }: InboxLineageSummaryProps) {
  const { data, isLoading } = useQuery({
    queryKey: ["inbox", "lineage", entryId] as const,
    queryFn: () => fetchInboxLineage(entryId),
    staleTime: 30_000,
  });

  const upstream = data?.upstream_events ?? [];
  const downstream = data?.downstream_events ?? [];

  return (
    <div
      className="rounded-md border border-border/40 bg-muted/5 text-[11px]"
      style={{ maxHeight: 200 }}
    >
      <LineageArm
        direction="up"
        label="上游 · Upstream"
        events={upstream}
        isLoading={isLoading}
        emptyHint="尚未有上游"
      />
      <div className="border-t border-border/30" />
      <LineageArm
        direction="down"
        label="下游 · Downstream"
        events={downstream}
        isLoading={isLoading}
        emptyHint="尚未下游"
      />
    </div>
  );
}

/* ── Arm (upstream / downstream) ──────────────────────────────── */

interface LineageArmProps {
  direction: "up" | "down";
  label: string;
  events: LineageEvent[];
  isLoading: boolean;
  emptyHint: string;
}

function LineageArm({
  direction,
  label,
  events,
  isLoading,
  emptyHint,
}: LineageArmProps) {
  const ArrowIcon = direction === "up" ? ArrowUpFromLine : ArrowDownToLine;
  return (
    <div className="px-3 py-2">
      <div className="mb-1 flex items-center gap-1 font-mono uppercase tracking-widest text-muted-foreground/50">
        <ArrowIcon className="size-2.5" aria-hidden />
        <span style={{ fontSize: 10 }}>{label}</span>
      </div>
      {isLoading ? (
        <div className="h-4 w-2/3 animate-pulse rounded bg-muted/30" />
      ) : events.length === 0 ? (
        <div className="text-muted-foreground/60" style={{ fontSize: 11 }}>
          {emptyHint}
        </div>
      ) : (
        <ul className="space-y-1">
          {events.map((event) => (
            <LineageLine
              key={event.event_id}
              event={event}
              direction={direction}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

/* ── Single line ──────────────────────────────────────────────── */

function LineageLine({
  event,
  direction,
}: {
  event: LineageEvent;
  direction: "up" | "down";
}) {
  const tone = toneFor(event.event_type);
  const IconComp = iconFor(event.event_type);
  const toneCls = TONE_CLASSES[tone];
  // The "supplementary refs" to show after the title depend on the
  // direction: upstream rows lean on their own upstream list to
  // identify the source; downstream rows highlight their downstream
  // targets. When the chosen side is empty, we fall back to the
  // opposite side so the row still carries a reference pointer.
  const primaryRefs =
    direction === "up"
      ? event.upstream.length > 0
        ? event.upstream
        : event.downstream
      : event.downstream.length > 0
        ? event.downstream
        : event.upstream;

  return (
    <li className="flex items-start gap-1.5 leading-snug">
      <IconComp
        className={cn("mt-0.5 size-3 shrink-0", toneCls.text)}
        aria-hidden
      />
      <div className="min-w-0 flex-1">
        <span className="text-foreground/85">
          {displayTitleFor(event)}
        </span>
        <span className="ml-1 text-muted-foreground/50">
          {formatRelativeTime(event.timestamp_ms)}
        </span>
        {primaryRefs.length > 0 && (
          <div className="mt-0.5 text-muted-foreground/60">
            {formatRefs(primaryRefs)}
          </div>
        )}
      </div>
    </li>
  );
}

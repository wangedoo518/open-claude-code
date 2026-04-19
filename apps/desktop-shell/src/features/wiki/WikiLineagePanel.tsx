/**
 * WikiLineagePanel — P1 sprint, Worker B surface #1.
 *
 * Rendered as the 4th section of `WikiArticleRelationsPanel` (below
 * Outgoing / Backlinks / Related). Shows a top-5 reverse-chrono
 * timeline of lineage events that touched this wiki page —
 * `raw_written`, `inbox_appended`, `wiki_page_applied`, etc.
 *
 * UX contract (see P1 brief):
 *   - Top-5 events only (limit 5)
 *   - Reverse-chronological (newest first)
 *   - Icon + display_title + relative time + upstream/downstream line
 *   - Combined applies render as single collapsible row (Collapsible
 *     from R1 primitives)
 *   - Long upstream lists (>3) auto-collapse to "+N 更多" via the
 *     `formatRefs` helper in `lineage-format.ts`
 *   - Empty state uses R1 `EmptyState` with size="compact"
 *
 * Worker A contract integration: if `@/lib/tauri` has not yet
 * exported `fetchWikiLineage` and the related `LineageEvent` /
 * `WikiLineageResponse` types, this component falls back to an
 * inline stub fetcher that returns an empty response. The stub
 * surfaces the fact that Worker A hasn't landed via the normal
 * `EmptyState` rather than throwing — matches the P1 brief's
 * "tolerance" bullet.
 */

import { useQuery } from "@tanstack/react-query";
import { ChevronDown, Clock } from "lucide-react";

import { EmptyState } from "@/components/ui/empty-state";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";
import {
  combinedApplyInboxCount,
  displayTitleFor,
  formatRefs,
  formatRelativeTime,
  iconFor,
  isCombinedApply,
  refLabel,
  toneFor,
  TONE_CLASSES,
} from "./lineage-format";

// Worker A's canonical wrapper + wire types. `LineageEvent` is
// structurally compatible with `LineageEventLike` from `lineage-format`,
// so the formatter helpers accept it without a cast.
import {
  fetchWikiLineage,
  type LineageEvent,
} from "@/lib/tauri";

// ── Component ───────────────────────────────────────────────────────

export interface WikiLineagePanelProps {
  slug: string;
}

/** Default top-5 window; configurable via Worker A's opts contract. */
const DEFAULT_LIMIT = 5;

export function WikiLineagePanel({ slug }: WikiLineagePanelProps) {
  const { data, isLoading } = useQuery({
    queryKey: ["wiki", "lineage", slug] as const,
    queryFn: () => fetchWikiLineage(slug, { limit: DEFAULT_LIMIT }),
    staleTime: 30_000,
  });

  const events = (data?.events ?? []).slice(0, DEFAULT_LIMIT);

  // Loading skeleton — three muted pulsing lines so the section has
  // a baseline height before the first payload arrives.
  if (isLoading) {
    return (
      <section className="mt-8 border-t border-[var(--color-border)] pt-6">
        <SectionHeader />
        <ul className="space-y-2">
          {[0, 1, 2].map((i) => (
            <li
              key={i}
              className="h-5 animate-pulse rounded bg-muted/30"
              style={{ width: `${80 - i * 10}%` }}
            />
          ))}
        </ul>
      </section>
    );
  }

  // Empty state — compact size so it doesn't dominate the narrower
  // relations column. Copy matches the P1 brief exactly.
  if (events.length === 0) {
    return (
      <section className="mt-8 border-t border-[var(--color-border)] pt-6">
        <SectionHeader />
        <EmptyState
          size="compact"
          icon={Clock}
          title="此页暂无来源记录"
          description="一旦有 raw 被写入、inbox 任务应用到本页，lineage 时间线就会显示在这里。"
        />
      </section>
    );
  }

  return (
    <section className="wiki-lineage-panel mt-8 border-t border-[var(--color-border)] pt-6">
      <SectionHeader total={data?.total_count ?? events.length} />
      <ul className="space-y-2">
        {events.map((event) => (
          <LineageRow key={event.event_id} event={event} />
        ))}
      </ul>
    </section>
  );
}

/* ── Section header ────────────────────────────────────────────── */

function SectionHeader({ total }: { total?: number }) {
  return (
    <h3 className="mb-3 flex items-center gap-1.5 text-[13px] font-medium text-[var(--color-muted-foreground)]">
      <Clock className="inline size-3" aria-hidden />
      <span>
        最近来源 · Recent Sources
        {typeof total === "number" && total > 0 ? (
          <span className="ml-1 text-muted-foreground/60">({total})</span>
        ) : null}
      </span>
    </h3>
  );
}

/* ── Single lineage row ────────────────────────────────────────── */

function LineageRow({ event }: { event: LineageEvent }) {
  // Combined applies get a collapsible single-row treatment instead
  // of fanning out all their upstreams in one rendered line.
  if (isCombinedApply(event)) {
    return <CombinedApplyRow event={event} />;
  }

  const tone = toneFor(event.event_type);
  const IconComp = iconFor(event.event_type);
  const toneCls = TONE_CLASSES[tone];

  return (
    <li className="flex items-start gap-2 text-[12px]">
      <span
        className={cn(
          "mt-0.5 inline-flex size-5 shrink-0 items-center justify-center rounded-full",
          toneCls.pill,
        )}
      >
        <IconComp className="size-3" aria-hidden />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline gap-1.5">
          <span className="text-foreground">{displayTitleFor(event)}</span>
          <span className="text-[11px] text-muted-foreground/60">
            {formatRelativeTime(event.timestamp_ms)}
          </span>
        </div>
        {/* Upstream / downstream caption — muted, joined by " → ". */}
        {(event.upstream.length > 0 || event.downstream.length > 0) && (
          <div className="mt-0.5 flex flex-wrap gap-x-1 gap-y-0.5 text-[11px] text-muted-foreground/70">
            {event.upstream.length > 0 && (
              <span>↑ {formatRefs(event.upstream)}</span>
            )}
            {event.downstream.length > 0 && (
              <span>→ {formatRefs(event.downstream)}</span>
            )}
          </div>
        )}
      </div>
    </li>
  );
}

/* ── Combined apply (collapsible) ──────────────────────────────── */

function CombinedApplyRow({ event }: { event: LineageEvent }) {
  const tone = toneFor(event.event_type);
  const IconComp = iconFor(event.event_type);
  const toneCls = TONE_CLASSES[tone];
  const inboxCount = combinedApplyInboxCount(event);

  return (
    <li className="text-[12px]">
      <Collapsible>
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="group flex w-full items-start gap-2 rounded-md py-0.5 text-left hover:bg-muted/30"
          >
            <span
              className={cn(
                "mt-0.5 inline-flex size-5 shrink-0 items-center justify-center rounded-full",
                toneCls.pill,
              )}
            >
              <IconComp className="size-3" aria-hidden />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-baseline gap-1.5">
                <span className="text-foreground">
                  由 {inboxCount} 条 inbox 合并应用
                </span>
                <span className="text-[11px] text-muted-foreground/60">
                  {formatRelativeTime(event.timestamp_ms)}
                </span>
              </div>
            </div>
            <ChevronDown
              className="mt-1 size-3 shrink-0 text-muted-foreground/50 transition-transform group-data-[state=open]:rotate-180"
              aria-hidden
            />
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="mt-1 ml-7 space-y-0.5 text-[11px] text-muted-foreground/80">
            {event.upstream.map((ref, idx) => (
              <div key={idx} className="flex items-baseline gap-1">
                <span aria-hidden>↑</span>
                <span>{refLabel(ref)}</span>
              </div>
            ))}
            {event.downstream.length > 0 && (
              <div className="mt-1 border-t border-border/30 pt-1">
                {event.downstream.map((ref, idx) => (
                  <div key={idx} className="flex items-baseline gap-1">
                    <span aria-hidden>→</span>
                    <span>{refLabel(ref)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </li>
  );
}

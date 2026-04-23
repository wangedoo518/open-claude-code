/**
 * InboxRow — feature-specific row for the Inbox (maintenance queue).
 *
 * DS 2.x-A product of Batch B audit §4.1 方案 B: an inline 148-line row
 * render in `features/inbox/InboxPage.tsx` (pre-migration L733-880)
 * promoted to a dedicated component so InboxPage's render tree stops
 * mixing queue-intelligence glue with row-level markup.
 *
 * Contrast with `components/ds/ListItem.tsx`:
 *  - ListItem is KB-specific (editorial 3-column with chevron, fixed
 *    category enum, `.ds-kb-*` class contract).
 *  - InboxRow owns the batch-mode leading-slot swap, the 3-badge meta
 *    line (decision + recommendedAction + sharedTarget), the cohort /
 *    why-long info chips, and the trailing reject action.
 *  - ListItem renders <li> with `.ds-kb-*` class contract; InboxRow
 *    renders <li> + inner <div role="button"> with the tailwind
 *    classes InboxPage shipped pre-migration, kept byte-stable to
 *    avoid any visual diff (audit spec: "Tailwind className 完全保真").
 *
 * Leaf deps (NOT modified by DS 2.x-A per worksheet hard constraint):
 *  - IngestDecisionBadge · RecommendedActionBadge · SharedTargetBadge
 *  - InfoTooltip (shared)
 *
 * Helpers inlined locally (row-exclusive pre-migration, safe to move):
 *  - translateKind (pre-migration InboxPage.tsx:121)
 *  - formatRelative (pre-migration InboxPage.tsx:2011)
 *
 * StatusIcon is imported from `@/components/ds/row-primitives` — the
 * DS 2.x-B hoist collapsed the detail-pane copy + this row copy into
 * a single primitive.
 *
 * DOM id `inbox-task-${entry.id}` preserved verbatim — `lib/deep-link.ts`
 * scrollIntoView on initial mount depends on it (see the I6/I7 sprint
 * regression notes in memory/observations.jsonl L3/L6/L8).
 */

import type { KeyboardEvent } from "react";
import { XCircle, Clock, Users } from "lucide-react";
import type { IntelligentEntry } from "@/features/inbox/InboxPage";
import { InfoTooltip } from "@/components/ui/info-tooltip";
import { IngestDecisionBadge } from "@/features/inbox/components/IngestDecisionBadge";
import { RecommendedActionBadge } from "@/features/inbox/components/RecommendedActionBadge";
import { SharedTargetBadge } from "@/features/inbox/components/SharedTargetBadge";
import { StatusIcon } from "@/components/ds/row-primitives";

export interface InboxRowProps {
  /** Full entry enriched with queue-intelligence + ingest decision. */
  entry: IntelligentEntry;
  /** Batch mode — leading slot renders a checkbox instead of StatusIcon. */
  batchMode: boolean;
  /** Batch-mode selection state (drives `input.checked`). */
  selected: boolean;
  /** Focus state (non-batch) — drives the 3px primary accent stripe. */
  active: boolean;
  /**
   * Queue-wide count of entries sharing this row's target slug. The
   * `<SharedTargetBadge>` self-gates on `slug != null && count >= 2`,
   * so the parent can pass 0 when the slug is absent. Caller owns the
   * O(1) map lookup so this component never sees the full count map
   * (audit §3 non-breaking extension: "sharedTargetCount: number 而非 Map").
   */
  sharedTargetCount: number;
  /** Toggle batch selection (fires on checkbox click OR batch-mode row click). */
  onToggleSelect: () => void;
  /** Set focused entry (fires on row click in non-batch mode). */
  onSelect: () => void;
  /**
   * Trailing delete action. Caller must pass `undefined` (not a no-op)
   * to hide the button — e.g. when `entry.status !== "pending"` or
   * in batchMode. The row ALSO gates on `!batchMode` internally as
   * defense in depth.
   */
  onReject?: () => void;
}

/** Translate inbox entry kind. Row-exclusive pre-migration helper. */
function translateKind(kind: string): string {
  const map: Record<string, string> = {
    "new-raw": "新素材",
    stale: "待更新",
    conflict: "冲突",
  };
  return map[kind] ?? kind;
}

/** Format ISO timestamp as 中文 relative. Row-exclusive pre-migration helper. */
function formatRelative(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;
  const deltaSecs = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (deltaSecs < 60) return `${deltaSecs}秒前`;
  if (deltaSecs < 3600) return `${Math.floor(deltaSecs / 60)}分钟前`;
  if (deltaSecs < 86_400) return `${Math.floor(deltaSecs / 3600)}小时前`;
  return `${Math.floor(deltaSecs / 86_400)}天前`;
}

export function InboxRow({
  entry,
  batchMode,
  selected,
  active,
  sharedTargetCount,
  onToggleSelect,
  onSelect,
  onReject,
}: InboxRowProps) {
  const handleClick = () => {
    if (batchMode) onToggleSelect();
    else onSelect();
  };
  const handleKeyDown = (ev: KeyboardEvent<HTMLDivElement>) => {
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      handleClick();
    }
  };

  const sharedSlug = entry.intelligence.target_candidate?.slug ?? null;

  return (
    <li
      id={`inbox-task-${entry.id}`}
      className="border-b border-border/20 last:border-b-0"
    >
      <div
        role="button"
        tabIndex={0}
        title={translateKind(entry.kind)}
        onClick={handleClick}
        onKeyDown={handleKeyDown}
        className={
          "flex w-full cursor-pointer items-start gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50 " +
          (active
            ? "bg-accent border-l-[3px] border-primary"
            : "border-l-[3px] border-l-transparent")
        }
      >
        {batchMode ? (
          <input
            type="checkbox"
            checked={selected}
            onClick={(ev) => ev.stopPropagation()}
            onChange={() => onToggleSelect()}
            className="mt-1 size-3.5 shrink-0 cursor-pointer accent-primary"
            aria-label={`选中任务 #${entry.id}`}
          />
        ) : (
          <StatusIcon status={entry.status} />
        )}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <IngestDecisionBadge decision={entry.decision} compact />
            <span
              className="flex-1 truncate text-foreground"
              style={{
                fontSize: 12,
                fontWeight: active ? 500 : 400,
              }}
            >
              {entry.title.replace(/^New raw entry/, "新素材")}
            </span>
          </div>
          <div className="mt-0.5 flex flex-wrap items-center gap-1">
            <RecommendedActionBadge
              action={entry.intelligence.recommended_action}
              compact
            />
            {/* Q2 — shared-target cohort indicator. SharedTargetBadge
                self-gates on `!slug || count < 2`, so the row just
                hands it the pre-computed pair and lets the leaf decide
                whether to render. */}
            <SharedTargetBadge slug={sharedSlug} count={sharedTargetCount} />
            {entry.intelligence.cohort_raw_id != null && (
              <span
                className="inline-flex items-center gap-0.5 rounded-full border border-border/40 px-1.5 py-0.5 text-muted-foreground/70"
                style={{ fontSize: 10 }}
                title={`同 raw #${String(entry.intelligence.cohort_raw_id).padStart(5, "0")} 还有其他任务`}
              >
                <Users className="size-2.5" aria-hidden />
                同源
              </span>
            )}
            {entry.intelligence.why_long ? (
              <InfoTooltip side="right">
                <div
                  className="space-y-1"
                  style={{ fontSize: 11, lineHeight: 1.5 }}
                >
                  <div className="font-medium text-foreground">
                    {entry.intelligence.why}
                  </div>
                  <div className="text-muted-foreground/80">
                    {entry.intelligence.why_long}
                  </div>
                </div>
              </InfoTooltip>
            ) : null}
            {/* DS1.5 — kind dropped from inline row; the
                RecommendedActionBadge + SharedTargetBadge already
                carry the semantic weight. Kind still surfaces via
                the row's native hover `title` attribute (on the outer
                row container) so power users can inspect. */}
          </div>
          <div
            className="mt-0.5 truncate text-muted-foreground/50"
            style={{ fontSize: 10 }}
          >
            {entry.description}
          </div>
          <div
            className="mt-0.5 flex items-center gap-1 text-muted-foreground/40"
            style={{ fontSize: 10 }}
          >
            <Clock className="size-2.5" />
            {formatRelative(entry.created_at)}
          </div>
        </div>
        {!batchMode && onReject && (
          <button
            type="button"
            className="shrink-0 rounded p-0.5 text-muted-foreground/30 transition-colors hover:text-destructive"
            onClick={(ev) => {
              ev.stopPropagation();
              onReject();
            }}
            title="删除"
            aria-label="删除任务"
          >
            <XCircle className="size-3" />
          </button>
        )}
      </div>
    </li>
  );
}

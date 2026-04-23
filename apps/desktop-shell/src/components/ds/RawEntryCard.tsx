/**
 * RawEntryCard — card-style row for the Raw material library.
 *
 * DS 2.x-A product of Batch B audit §4.2 方案 B: an inline 152-line
 * `EntryCard` function in `features/raw/RawLibraryPage.tsx`
 * (pre-migration L852-1003) promoted to a dedicated component so
 * RawLibraryPage keeps only page-level state (selection, expansion,
 * mutations) and the card markup lives in a single focused file.
 *
 * Semantic upgrade (audit §6 spec): outer container is `<article>`,
 * not `<div>`. The card is not inside a `<ul>` and acts as a
 * self-contained document card — `<article>` reads cleaner to screen
 * readers than a generic `<div>`. DOM id `raw-entry-${id}` is preserved
 * verbatim for `?entry=N` deep-link scrollIntoView.
 *
 * Contrast with `ListItem.tsx` / `InboxRow.tsx`:
 *  - ListItem is a flat KB-editorial row with chevron-right.
 *  - InboxRow is a compact queue row with a 3-badge meta and a
 *    batch-mode leading-slot swap.
 *  - RawEntryCard is a full card (rounded-xl + shadow-warm-ring) with
 *    hover-revealed trailing actions (Ask / Delete / ClearFocus) and
 *    an expand-in-place body viewer slot. The `group` utility is on
 *    the outer <article> so children can use `group-hover:*`.
 *
 * Leaf deps (NOT modified by DS 2.x-A per worksheet hard constraint):
 *  - RawLineageBadge
 *  - `expandedContent` slot accepts any ReactNode — callers typically
 *    pass `<ExpandedDetail />`; caller owns its query/lifecycle.
 *    `FullScreenReader` is a child of `ExpandedDetail`, transitively
 *    out of scope too.
 *
 * Source helpers (translateSource / sourceBadgeStyle / SourceIcon /
 * formatSize) are imported from `@/components/ds/row-primitives`. The
 * DS 2.x-B hoist collapsed what DS 2.x-A temporarily duplicated
 * (RawLibraryPage's ExpandedDetail / FullScreenReader now import the
 * same primitive).
 */

import type { ReactNode } from "react";
import {
  MessageCircleQuestion,
  Trash2,
  Loader2,
  X,
  ChevronDown,
} from "lucide-react";
import type { RawEntry } from "@/features/ingest/types";
import { RawLineageBadge } from "@/features/raw/RawLineageBadge";
import {
  SourceIcon,
  sourceBadgeStyle,
  translateSource,
  formatSize,
} from "@/components/ds/row-primitives";

export interface RawEntryCardProps {
  /** Full raw entry (slug, source, date, byte_size, etc.). */
  entry: RawEntry;
  /** Batch-delete selection state. */
  isSelected: boolean;
  /** Expand state — drives borderLeft accent + expandedContent render. */
  isExpanded: boolean;
  /** Delete mutation inflight — swaps Trash2 for Loader2 + disables the button. */
  isDeleting: boolean;
  onToggleSelect: () => void;
  onToggleExpand: () => void;
  /** Clear-focus X (only rendered while `isExpanded`). */
  onClearExpand: () => void;
  onDelete: () => void;
  /** Fires the "用这条素材提问" hover action (parent composes the /ask URL). */
  onAsk: () => void;
  /**
   * Caller-provided expand body slot. Typically an `<ExpandedDetail />`
   * instance. Only rendered when `isExpanded`; the component's own
   * guard + the parent's typical gating pattern both produce the same
   * effect — defense in depth against prop/state drift.
   */
  expandedContent?: ReactNode;
}

export function RawEntryCard({
  entry,
  isSelected,
  isExpanded,
  isDeleting,
  onToggleSelect,
  onToggleExpand,
  onClearExpand,
  onDelete,
  onAsk,
  expandedContent,
}: RawEntryCardProps) {
  const badge = sourceBadgeStyle(entry.source);

  return (
    <article
      id={`raw-entry-${entry.id}`}
      className="group rounded-xl border bg-card p-4 shadow-warm-ring transition-shadow hover:shadow-warm-ring-hover"
      style={{
        borderLeft: isExpanded ? "3px solid var(--color-primary)" : undefined,
      }}
    >
      {/* Card row */}
      <div className="flex items-center gap-3">
        {/* Checkbox for multi-select */}
        <input
          type="checkbox"
          checked={isSelected}
          onChange={(e) => {
            e.stopPropagation();
            onToggleSelect();
          }}
          className="size-3.5 shrink-0 cursor-pointer rounded border-border accent-primary"
        />

        {/* Clickable content area */}
        <button
          type="button"
          onClick={onToggleExpand}
          title={`${entry.slug} · ${translateSource(entry.source)}`}
          className="flex min-w-0 flex-1 items-start gap-3 text-left"
        >
          {/* Source icon tile */}
          <div
            className="flex size-8 shrink-0 items-center justify-center rounded-md"
            style={{ backgroundColor: badge.bg }}
          >
            <SourceIcon
              source={entry.source}
              className="size-4"
              style={{ color: badge.text }}
            />
          </div>

          {/* Title + metadata */}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              {/* Source badge pill */}
              <span
                className="shrink-0 rounded-full px-1.5 py-0.5 font-medium"
                style={{
                  fontSize: 10,
                  backgroundColor: badge.bg,
                  color: badge.text,
                }}
              >
                {translateSource(entry.source)}
              </span>
              {/* Date + size */}
              <span
                className="text-muted-foreground/40"
                style={{ fontSize: 11 }}
              >
                {entry.date}
              </span>
              <span
                className="text-muted-foreground/30"
                style={{ fontSize: 11 }}
              >
                {formatSize(entry.byte_size)}
              </span>
              {/* P1 sprint — downstream lineage status badge. */}
              <RawLineageBadge rawId={entry.id} />
            </div>
            {/* Title */}
            <div
              className="mt-0.5 truncate text-foreground"
              style={{ fontSize: 13, fontWeight: isExpanded ? 500 : 400 }}
            >
              {entry.slug}
            </div>
          </div>
        </button>

        {/* Ask with this — visible on hover */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onAsk();
          }}
          className="shrink-0 rounded-md p-1.5 text-muted-foreground/30 opacity-0 transition-all hover:bg-primary/10 hover:text-primary group-hover:opacity-100"
          title="用这条素材提问"
          aria-label="Ask with this"
        >
          <MessageCircleQuestion className="size-3.5" />
        </button>

        {/* Delete button — visible on hover */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          disabled={isDeleting}
          className="shrink-0 rounded-md p-1.5 text-muted-foreground/30 opacity-0 transition-all hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100 disabled:opacity-50"
          title="删除"
        >
          {isDeleting ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Trash2 className="size-3.5" />
          )}
        </button>

        {/* Clear-focus × — only while expanded. Mirrors the chip's ×
            so the user has two consistent exits. */}
        {isExpanded && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onClearExpand();
            }}
            title="清除聚焦"
            aria-label="清除聚焦"
            className="shrink-0 rounded-md p-1.5 text-muted-foreground/50 transition-colors hover:bg-accent hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        )}

        {/* Expand indicator */}
        <ChevronDown
          className={
            "size-3.5 shrink-0 text-muted-foreground/30 transition-transform " +
            (isExpanded ? "rotate-180" : "")
          }
        />
      </div>

      {/* Expanded detail — caller-provided slot. Gated by isExpanded so
          parent & row agree on visibility even if expandedContent is
          passed unconditionally. */}
      {isExpanded && expandedContent}
    </article>
  );
}

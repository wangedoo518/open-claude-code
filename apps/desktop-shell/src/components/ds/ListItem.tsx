/**
 * ListItem — editorial 3-column row for knowledge-base-style lists.
 *
 * v2 kit source: ui_kits/desktop-shell-v2/KnowledgeBase.jsx:30-44
 * (the `<div className="list-item">` pattern). kit.css selectors at
 * ui_kits/desktop-shell-v2/kit.css:290-299 define `.list-item` +
 * `.l-ico / .l-title / .l-sub / .l-meta`.
 *
 * DS class contract: `.ds-kb-item` + `.ds-kb-icon / .ds-kb-title /
 * .ds-kb-summary / .ds-kb-meta-row / .ds-kb-chevron` in
 * apps/desktop-shell/src/globals.css (DS1.2 block).
 *
 * Renders as `<li>` with either `href` (router Link wrapper) or
 * `onClick` (keyboard/mouse handler). `category` optionally drives a
 * colored badge at the top of the meta row (concept / person / topic
 * / compare — same taxonomy as KnowledgePagesList).
 *
 * Migrated from KnowledgePagesList.tsx:199-248 inline (DS1.7-B-γ).
 */

import type { ReactNode, KeyboardEvent } from "react";
import { ChevronRight, type LucideIcon } from "lucide-react";

export type ListItemCategory =
  | "concept"
  | "person"
  | "topic"
  | "compare"
  | "unknown";

const CATEGORY_LABELS: Record<ListItemCategory, string> = {
  concept: "概念",
  person: "人物",
  topic: "主题",
  compare: "对比",
  unknown: "未分类",
};

const CATEGORY_BADGE_CLASS: Record<ListItemCategory, string> = {
  concept: "ds-kb-badge ds-kb-badge-concept",
  person: "ds-kb-badge ds-kb-badge-person",
  topic: "ds-kb-badge ds-kb-badge-topic",
  compare: "ds-kb-badge",
  unknown: "ds-kb-badge",
};

export interface ListItemProps {
  /** Lucide icon rendered in the leading icon column. */
  icon: LucideIcon;
  /** Row title (truncates). */
  title: string;
  /** Optional 2-line summary below the title. */
  summary?: string;
  /**
   * Free-form meta content rendered on a row below the summary.
   * Typically a series of short `<span>` chips (source, updated time,
   * size). Consumers can use any JSX.
   */
  meta?: ReactNode;
  /** When set, the row renders inside a router-free click handler. */
  onClick?: () => void;
  /**
   * Category badge — rendered as the FIRST chip of the meta row when
   * provided. Callers pass the already-classified category; the
   * component handles label + class mapping.
   */
  category?: ListItemCategory;
  /** Optional href for semantic purposes (currently used by tooltip / title only). */
  href?: string;
}

export function ListItem({
  icon: Icon,
  title,
  summary,
  meta,
  onClick,
  category,
  href,
}: ListItemProps) {
  const handleKeyDown = (e: KeyboardEvent<HTMLLIElement>) => {
    if (!onClick) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onClick();
    }
  };

  const cat = category ?? "unknown";
  const showCategoryBadge = category != null && category !== "unknown";

  return (
    <li
      className="ds-kb-item"
      onClick={onClick}
      role={onClick ? "link" : undefined}
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={handleKeyDown}
      title={href}
    >
      <span className="ds-kb-icon">
        <Icon className="size-4" strokeWidth={1.5} />
      </span>
      <div className="min-w-0">
        <div className="ds-kb-title truncate">{title}</div>
        {summary && <p className="ds-kb-summary">{summary}</p>}
        {(showCategoryBadge || meta) && (
          <div className="ds-kb-meta-row">
            {showCategoryBadge && (
              <span className={CATEGORY_BADGE_CLASS[cat]}>
                {CATEGORY_LABELS[cat]}
              </span>
            )}
            {meta}
          </div>
        )}
      </div>
      <ChevronRight
        className="ds-kb-chevron size-4"
        strokeWidth={1.5}
        aria-hidden="true"
      />
    </li>
  );
}

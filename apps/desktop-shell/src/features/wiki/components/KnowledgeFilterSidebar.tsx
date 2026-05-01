/**
 * KnowledgeFilterSidebar — Slice 42 left-side Tolaria-style filter rail.
 *
 * Spec §7.3 calls Knowledge a three-column workbench: filters | list |
 * reader. The page already supports 类型 / 目的 / 来源 filtering through
 * inline toolbar selects; this sidebar exposes the same state via
 * vertical chip groups so the workbench layout matches the spec without
 * forcing a rewrite of the existing toolbar (the two surfaces stay in
 * sync because they share the same setter callbacks).
 *
 * Pure presentational — all state lives in the parent. No data fetches.
 */

import { Filter } from "lucide-react";
import {
  PURPOSE_LENSES,
  type PurposeLensId,
} from "@/features/purpose/purpose-lenses";

export type FilterMode = "all" | "concept" | "derived";
export type PurposeFilterMode = "all" | PurposeLensId;
export type SourceFilterMode = "all" | "sourced" | "missing";

export interface KnowledgeFilterSidebarProps {
  filterMode: FilterMode;
  onFilterMode: (m: FilterMode) => void;
  purposeMode: PurposeFilterMode;
  onPurposeMode: (m: PurposeFilterMode) => void;
  sourceMode: SourceFilterMode;
  onSourceMode: (m: SourceFilterMode) => void;
  visibleCount: number;
  total: number;
}

const FILTER_OPTIONS: ReadonlyArray<{ value: FilterMode; label: string }> = [
  { value: "all", label: "全部" },
  { value: "concept", label: "概念" },
  { value: "derived", label: "素材衍生" },
];

const SOURCE_OPTIONS: ReadonlyArray<{
  value: SourceFilterMode;
  label: string;
}> = [
  { value: "all", label: "全部" },
  { value: "sourced", label: "有来源" },
  { value: "missing", label: "缺来源" },
];

export function KnowledgeFilterSidebar({
  filterMode,
  onFilterMode,
  purposeMode,
  onPurposeMode,
  sourceMode,
  onSourceMode,
  visibleCount,
  total,
}: KnowledgeFilterSidebarProps) {
  return (
    <aside className="ds-kb-filter-sidebar" aria-label="Knowledge filters">
      <div className="ds-kb-filter-sidebar-head">
        <Filter className="size-3" strokeWidth={1.5} aria-hidden />
        <span>筛选</span>
        <span className="ds-kb-filter-sidebar-count">
          {visibleCount}/{total}
        </span>
      </div>

      <Section label="类型 (Type)">
        {FILTER_OPTIONS.map((opt) => (
          <ChipButton
            key={opt.value}
            active={filterMode === opt.value}
            onClick={() => onFilterMode(opt.value)}
          >
            {opt.label}
          </ChipButton>
        ))}
      </Section>

      <Section label="目的 (Purpose)">
        <ChipButton
          active={purposeMode === "all"}
          onClick={() => onPurposeMode("all")}
        >
          全部
        </ChipButton>
        {PURPOSE_LENSES.map((lens) => (
          <ChipButton
            key={lens.id}
            active={purposeMode === lens.id}
            onClick={() => onPurposeMode(lens.id)}
          >
            {lens.zhLabel}
          </ChipButton>
        ))}
      </Section>

      <Section label="来源 (Source)">
        {SOURCE_OPTIONS.map((opt) => (
          <ChipButton
            key={opt.value}
            active={sourceMode === opt.value}
            onClick={() => onSourceMode(opt.value)}
          >
            {opt.label}
          </ChipButton>
        ))}
      </Section>
    </aside>
  );
}

function Section({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="ds-kb-filter-section">
      <div className="ds-kb-filter-section-label">{label}</div>
      <div className="ds-kb-filter-section-chips">{children}</div>
    </div>
  );
}

function ChipButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className="ds-kb-filter-chip"
      data-active={active}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

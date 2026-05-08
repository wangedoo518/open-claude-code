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

import { Filter, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import {
  PURPOSE_LENSES,
  type PurposeLensId,
} from "@/features/purpose/purpose-lenses";
import { useKnowledgeFilterSidebarStore } from "@/state/knowledge-filter-sidebar-store";

export type FilterMode = "all" | "concept" | "derived" | "inspiration";
export type PurposeFilterMode = "all" | PurposeLensId;
export type SourceFilterMode = "all" | "sourced" | "missing";
export type PriorityFilterMode = "all" | "high" | "medium" | "low";
export type VitalityFilterMode =
  | "all"
  | "spark"
  | "seed"
  | "growing"
  | "stable"
  | "cooling"
  | "archived"
  | "noise";

export interface KnowledgeFilterSidebarProps {
  filterMode: FilterMode;
  onFilterMode: (m: FilterMode) => void;
  purposeMode: PurposeFilterMode;
  onPurposeMode: (m: PurposeFilterMode) => void;
  sourceMode: SourceFilterMode;
  onSourceMode: (m: SourceFilterMode) => void;
  priorityMode: PriorityFilterMode;
  onPriorityMode: (m: PriorityFilterMode) => void;
  vitalityMode: VitalityFilterMode;
  onVitalityMode: (m: VitalityFilterMode) => void;
  visibleCount: number;
  total: number;
}

const FILTER_OPTIONS: ReadonlyArray<{ value: FilterMode; label: string }> = [
  { value: "all", label: "全部" },
  { value: "concept", label: "概念" },
  { value: "derived", label: "素材衍生" },
  { value: "inspiration", label: "灵感" },
];

const SOURCE_OPTIONS: ReadonlyArray<{
  value: SourceFilterMode;
  label: string;
}> = [
  { value: "all", label: "全部" },
  { value: "sourced", label: "有来源" },
  { value: "missing", label: "缺来源" },
];

const PRIORITY_OPTIONS: ReadonlyArray<{
  value: PriorityFilterMode;
  label: string;
}> = [
  { value: "all", label: "全部" },
  { value: "high", label: "高" },
  { value: "medium", label: "中" },
  { value: "low", label: "低" },
];

const VITALITY_OPTIONS: ReadonlyArray<{
  value: VitalityFilterMode;
  label: string;
}> = [
  { value: "all", label: "全部" },
  { value: "spark", label: "火花" },
  { value: "seed", label: "种子" },
  { value: "growing", label: "增长" },
  { value: "stable", label: "稳定" },
  { value: "cooling", label: "冷却" },
  { value: "archived", label: "归档" },
  { value: "noise", label: "噪音" },
];

export function KnowledgeFilterSidebar({
  filterMode,
  onFilterMode,
  purposeMode,
  onPurposeMode,
  sourceMode,
  onSourceMode,
  priorityMode,
  onPriorityMode,
  vitalityMode,
  onVitalityMode,
  visibleCount,
  total,
}: KnowledgeFilterSidebarProps) {
  const open = useKnowledgeFilterSidebarStore((s) => s.open);
  const toggle = useKnowledgeFilterSidebarStore((s) => s.toggle);

  // Slice 49 — both content + restore are always mounted; the collapse
  // animation is driven by the --collapsed class on the outer aside,
  // which transitions width 250 → 40 over 260ms while content fades
  // out and restore fades in. Mirrors the Ask /ask page pattern
  // (.ds-rail-secondary--collapsed) for consistency across workbenches.
  return (
    <aside
      className={`ds-kb-filter-sidebar ${open ? "ds-kb-filter-sidebar--expanded" : "ds-kb-filter-sidebar--collapsed"}`}
      aria-label="Knowledge filters"
      data-collapsed={!open || undefined}
    >
      <div
        className="ds-kb-filter-sidebar-content"
        aria-hidden={!open}
        inert={!open || undefined}
      >
      <div className="ds-kb-filter-sidebar-head">
        <Filter className="size-3" strokeWidth={1.5} aria-hidden />
        <span>筛选</span>
        <span className="ds-kb-filter-sidebar-count">
          {visibleCount}/{total}
        </span>
        <button
          type="button"
          className="ask-history-icon-button"
          onClick={toggle}
          aria-label="收起筛选侧栏"
          title="收起筛选"
        >
          <PanelLeftClose className="size-3.5" />
        </button>
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

      <Section label="Priority">
        {PRIORITY_OPTIONS.map((opt) => (
          <ChipButton
            key={opt.value}
            active={priorityMode === opt.value}
            onClick={() => onPriorityMode(opt.value)}
          >
            {opt.label}
          </ChipButton>
        ))}
      </Section>

      <Section label="Vitality">
        {VITALITY_OPTIONS.map((opt) => (
          <ChipButton
            key={opt.value}
            active={vitalityMode === opt.value}
            onClick={() => onVitalityMode(opt.value)}
          >
            {opt.label}
          </ChipButton>
        ))}
      </Section>
      </div>
      <div
        className="ds-kb-filter-sidebar-restore"
        aria-hidden={open}
      >
        <button
          type="button"
          className="ds-rail-secondary-toggle"
          onClick={toggle}
          title="展开筛选"
          aria-label="展开筛选侧栏"
          tabIndex={open ? -1 : 0}
        >
          <PanelLeftOpen className="size-4" />
        </button>
      </div>
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

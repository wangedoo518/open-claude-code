/**
 * PillTabs — editorial pill-style tablist.
 *
 * v2 kit source: ui_kits/desktop-shell-v2/Shell.jsx:46-52 (the
 * `<div className="pill-tabs">` inside TopBarV2). kit.css selectors
 * at ui_kits/desktop-shell-v2/kit.css:90-104.
 *
 * DS class contract: `.ds-pill-tabs` + `.ds-pill-tab` with
 * `data-active="true"` on the active pill. Styles live in
 * apps/desktop-shell/src/globals.css (DS1.2 block).
 *
 * Wires ARIA `role="tablist" / "tab"` + `aria-selected` + `aria-controls`
 * so screen readers read this as a proper tablist. Callers are
 * responsible for rendering the tabpanel elsewhere with a matching
 * `id={\`${idPrefix}-panel-${tabId}\`}` and `aria-labelledby={\`${idPrefix}-tab-${activeTabId}\`}`.
 *
 * Migrated from KnowledgeHubPage.tsx:128-156 inline (DS1.7-B-γ).
 */

import type { LucideIcon } from "lucide-react";

export interface PillTab {
  id: string;
  label: string;
  icon?: LucideIcon;
  /** Optional short text shown on hover + read by aria-label beside the list. */
  hint?: string;
}

export interface PillTabsProps {
  tabs: readonly PillTab[];
  active: string;
  onChange: (id: string) => void;
  /** a11y label for the `<div role="tablist">` wrapper. */
  ariaLabel: string;
  /**
   * Unique prefix for generated tab ids. Each button receives
   * `id={\`${idPrefix}-tab-${tab.id}\`}` and
   * `aria-controls={\`${idPrefix}-panel-${tab.id}\`}`.
   * Consumers of the paired tabpanel must set `id={\`${idPrefix}-panel-<tabId>\`}`.
   * Defaults to `"pill"` when omitted (fine for single-use pages).
   */
  idPrefix?: string;
}

export function PillTabs({
  tabs,
  active,
  onChange,
  ariaLabel,
  idPrefix = "pill",
}: PillTabsProps) {
  return (
    <div className="ds-pill-tabs" role="tablist" aria-label={ariaLabel}>
      {tabs.map((t) => {
        const Icon = t.icon;
        const isActive = t.id === active;
        return (
          <button
            key={t.id}
            id={`${idPrefix}-tab-${t.id}`}
            type="button"
            role="tab"
            aria-selected={isActive}
            aria-controls={`${idPrefix}-panel-${t.id}`}
            onClick={() => onChange(t.id)}
            className="ds-pill-tab"
            data-active={isActive || undefined}
            title={t.hint}
          >
            {Icon && <Icon className="size-3.5" strokeWidth={1.5} />}
            <span>{t.label}</span>
          </button>
        );
      })}
    </div>
  );
}

/**
 * WikiTabBar — browser-style multi-tab bar for the Wiki Explorer.
 * Per ia-layout.md §3 and component-spec.md §1.
 */

import { X } from "lucide-react";
import { useWikiTabStore } from "@/state/wiki-tab-store";

export function WikiTabBar() {
  const tabs = useWikiTabStore((s) => s.tabs);
  const activeTabId = useWikiTabStore((s) => s.activeTabId);
  const setActiveTab = useWikiTabStore((s) => s.setActiveTab);
  const closeTab = useWikiTabStore((s) => s.closeTab);

  return (
    <div className="flex h-10 items-stretch border-b border-[var(--color-border)] bg-[var(--color-sidebar-background)] overflow-x-auto [scrollbar-width:none]">
      {tabs.map((tab, i) => {
        const isActive = tab.id === activeTabId;
        return (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`group/tab relative flex min-w-[140px] max-w-[240px] flex-[0_0_auto] items-center gap-1.5 px-3 text-[12px] transition-colors ${
              isActive
                ? "bg-[var(--color-background)] text-[var(--color-foreground)]"
                : "text-[var(--color-muted-foreground)] hover:bg-[var(--color-accent)]/50 hover:text-[var(--color-foreground)]"
            }`}
          >
            {/* Active indicator — 2px bottom border in primary color */}
            {isActive && (
              <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-[var(--color-primary)]" />
            )}

            {/* Title (truncated) */}
            <span className="flex-1 truncate text-left">{tab.title}</span>

            {/* Close button — per component-spec.md §1.5 */}
            {tab.closable && (
              <span
                role="button"
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.id);
                }}
                className="rounded-sm p-0.5 opacity-0 transition-all group-hover/tab:opacity-60 hover:!opacity-100 hover:bg-[var(--color-foreground)]/10"
              >
                <X className="size-3" />
              </span>
            )}

            {/* Separator line between tabs — per component-spec.md §1.4 */}
            {i < tabs.length - 1 && (
              <span className="absolute right-0 top-1/4 h-1/2 w-px bg-[var(--color-border)]/70" />
            )}
          </button>
        );
      })}
    </div>
  );
}

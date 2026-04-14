/**
 * Wiki Tab Store — multi-tab browser state for the Wiki Explorer.
 * Per 02-wiki-explorer.md §6.3 (WikiExplorerStore) and ia-layout.md §3.
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { namespacedStorage } from "./store-helpers";

export interface WikiTabItem {
  /** Unique identifier. For articles: slug; for special: kind string. */
  id: string;
  /** Tab kind discriminator. */
  kind: "index" | "article" | "graph" | "raw";
  /** Wiki page slug (only for kind="article"). */
  slug?: string;
  /** Display title in the tab bar. */
  title: string;
  /** Whether the tab can be closed. _index is permanent. */
  closable: boolean;
}

interface WikiTabStore {
  tabs: WikiTabItem[];
  activeTabId: string;

  /** Open a tab. If same id already exists, just activate it. */
  openTab: (item: WikiTabItem) => void;
  /** Close a tab. If it was active, activate the previous one. */
  closeTab: (id: string) => void;
  /** Switch to a specific tab. */
  setActiveTab: (id: string) => void;
}

const INDEX_TAB: WikiTabItem = {
  id: "_index",
  kind: "index",
  title: "Wiki",
  closable: false,
};

const GRAPH_TAB: WikiTabItem = {
  id: "_graph",
  kind: "graph",
  title: "Graph",
  closable: false,
};

export const useWikiTabStore = create<WikiTabStore>()(
  persist(
    (set, get) => ({
  tabs: [INDEX_TAB, GRAPH_TAB],
  activeTabId: INDEX_TAB.id,

  openTab: (item) => {
    const { tabs } = get();
    const existing = tabs.find((t) => t.id === item.id);
    if (existing) {
      set({ activeTabId: item.id });
    } else {
      set({
        tabs: [...tabs, item],
        activeTabId: item.id,
      });
    }
  },

  closeTab: (id) => {
    const { tabs, activeTabId } = get();
    // Don't close non-closable tabs.
    const target = tabs.find((t) => t.id === id);
    if (!target || !target.closable) return;

    const filtered = tabs.filter((t) => t.id !== id);
    let nextActive = activeTabId;
    if (activeTabId === id) {
      // Activate the last tab, or fall back to _index.
      nextActive = filtered.length > 0 ? filtered[filtered.length - 1].id : INDEX_TAB.id;
    }
    set({ tabs: filtered, activeTabId: nextActive });
  },

  setActiveTab: (id) => set({ activeTabId: id }),
}),
    {
      name: "wiki-tabs",
      storage: namespacedStorage("wiki-tabs"),
      partialize: (state) => ({
        tabs: state.tabs,
        activeTabId: state.activeTabId,
      }),
      merge: (persisted, current) => {
        const p = persisted as Partial<WikiTabStore> | null;
        if (!p?.tabs) return current;
        // Ensure fixed tabs always exist.
        const hasFix = (id: string) => p.tabs!.some((t) => t.id === id);
        const tabs = [...p.tabs!];
        if (!hasFix(INDEX_TAB.id)) tabs.unshift(INDEX_TAB);
        if (!hasFix(GRAPH_TAB.id)) tabs.splice(1, 0, GRAPH_TAB);
        return { ...current, tabs, activeTabId: p.activeTabId ?? current.activeTabId };
      },
    },
  ),
);

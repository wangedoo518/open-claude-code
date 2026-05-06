/**
 * Knowledge Filter Sidebar store — Slice 49.
 *
 * Single boolean (`open`) controlling the left-side filter sidebar
 * on /wiki. Default: true (visible) so first-time users see the full
 * Tolaria-style three-column workbench from spec §7.3 without
 * needing to discover the toggle. Persisted under
 * `open-claude-code:knowledge-filter-sidebar:state` so the choice
 * survives reloads.
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { namespacedStorage } from "./store-helpers";

interface KnowledgeFilterSidebarStore {
  open: boolean;
  setOpen: (open: boolean) => void;
  toggle: () => void;
}

export const useKnowledgeFilterSidebarStore =
  create<KnowledgeFilterSidebarStore>()(
    persist(
      (set) => ({
        open: true,
        setOpen: (open) => set({ open }),
        toggle: () => set((state) => ({ open: !state.open })),
      }),
      {
        name: "state",
        storage: namespacedStorage("knowledge-filter-sidebar"),
        partialize: (state) => ({ open: state.open }),
      },
    ),
  );

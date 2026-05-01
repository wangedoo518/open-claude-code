/**
 * Shell Inspector store — Slice 48.
 *
 * Single boolean (`open`) controlling the global ShellInspector aside.
 * Default: false (collapsed) so users opt in instead of being shown a
 * 320px right column on every fresh boot. Persisted under
 * `open-claude-code:shell-inspector:state` so the choice survives
 * reloads.
 *
 * Note: route-level visibility (graph / inbox / wiki article) still
 * lives in ClawWikiShell as a `showShellInspector` boolean; the store
 * only governs the user toggle. The aside renders only when both
 * conditions are true.
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { namespacedStorage } from "./store-helpers";

interface ShellInspectorStore {
  open: boolean;
  setOpen: (open: boolean) => void;
  toggle: () => void;
}

export const useShellInspectorStore = create<ShellInspectorStore>()(
  persist(
    (set) => ({
      open: false,
      setOpen: (open) => set({ open }),
      toggle: () => set((state) => ({ open: !state.open })),
    }),
    {
      name: "state",
      storage: namespacedStorage("shell-inspector"),
      partialize: (state) => ({ open: state.open }),
    },
  ),
);

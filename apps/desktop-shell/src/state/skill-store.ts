/**
 * Skill Store — tracks absorb / cleanup / patrol task state.
 * Per technical-design.md §5.3.2.
 */

import { create } from "zustand";

export interface AbsorbProgress {
  processed: number;
  total: number;
  current_entry_id: number;
  action: string;
  page_slug: string | null;
}

export interface AbsorbResultData {
  created: number;
  updated: number;
  skipped: number;
  failed: number;
  duration_ms: number;
}

interface SkillStore {
  // ── Absorb state ────────────────────────────────────────────
  absorbRunning: boolean;
  absorbTaskId: string | null;
  absorbProgress: AbsorbProgress | null;
  absorbResult: AbsorbResultData | null;
  absorbError: string | null;

  // ── Actions ─────────────────────────────────────────────────
  startAbsorb: (taskId: string) => void;
  updateAbsorbProgress: (progress: AbsorbProgress) => void;
  completeAbsorb: (result: AbsorbResultData) => void;
  failAbsorb: (error: string) => void;
  resetAbsorb: () => void;
}

export const useSkillStore = create<SkillStore>()((set) => ({
  absorbRunning: false,
  absorbTaskId: null,
  absorbProgress: null,
  absorbResult: null,
  absorbError: null,

  startAbsorb: (taskId) =>
    set({
      absorbRunning: true,
      absorbTaskId: taskId,
      absorbProgress: null,
      absorbResult: null,
      absorbError: null,
    }),

  updateAbsorbProgress: (progress) =>
    set({ absorbProgress: progress }),

  completeAbsorb: (result) =>
    set({
      absorbRunning: false,
      absorbProgress: null,
      absorbResult: result,
    }),

  failAbsorb: (error) =>
    set({
      absorbRunning: false,
      absorbProgress: null,
      absorbError: error,
    }),

  resetAbsorb: () =>
    set({
      absorbRunning: false,
      absorbTaskId: null,
      absorbProgress: null,
      absorbResult: null,
      absorbError: null,
    }),
}));

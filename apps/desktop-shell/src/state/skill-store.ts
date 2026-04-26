/**
 * Skill Store — tracks absorb / cleanup / patrol task state.
 * Per technical-design.md §5.3.2.
 */

import { create } from "zustand";

export interface AbsorbProgress {
  task_id: string;
  processed: number;
  total: number;
  current_entry_id: number;
  action: string;
  page_slug: string | null;
  page_title: string | null;
  error: string | null;
}

export interface AbsorbResultData {
  task_id?: string;
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
    set((state) => {
      if (state.absorbResult?.task_id === taskId) {
        return state;
      }
      if (state.absorbTaskId === taskId) {
        return {
          ...state,
          absorbRunning: true,
          absorbError: null,
        };
      }
      return {
        absorbRunning: true,
        absorbTaskId: taskId,
        absorbProgress: null,
        absorbResult: null,
        absorbError: null,
      };
    }),

  updateAbsorbProgress: (progress) =>
    set((state) => {
      if (state.absorbResult?.task_id === progress.task_id) {
        return state;
      }
      if (state.absorbTaskId && state.absorbTaskId !== progress.task_id) {
        return state;
      }
      return {
        absorbRunning: true,
        absorbTaskId: progress.task_id,
        absorbProgress: progress,
        absorbError: null,
      };
    }),

  completeAbsorb: (result) =>
    set((state) => {
      if (result.task_id && state.absorbTaskId && state.absorbTaskId !== result.task_id) {
        return state;
      }
      return {
        absorbRunning: false,
        absorbTaskId: null,
        absorbProgress: null,
        absorbResult: result,
        absorbError: null,
      };
    }),

  failAbsorb: (error) =>
    set({
      absorbRunning: false,
      absorbTaskId: null,
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

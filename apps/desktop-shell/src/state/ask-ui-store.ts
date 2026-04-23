/**
 * Ask UI store — cross-component state shared between AskWorkbench and
 * other surfaces that need to toggle the Ask view (e.g. the command
 * palette's "查看演示对话" entry).
 *
 * Scope (kept deliberately narrow):
 *   - `showDemo` — when true, AskWorkbench renders `MOCK_DEMO_MESSAGES`
 *     as the initial conversation + surfaces a dismissible banner.
 *
 * Why a store (Batch E §1):
 *   The "查看演示对话" entry used to live as a CTA inside AskWorkbench's
 *   empty-state hero, which is in the same render tree as the state it
 *   toggled. Batch E §0 trimmed that CTA out of the hero ("DS quiet
 *   intellectual" — the hero is now greeting + prompts only). Batch E
 *   §1 restores the entry as a command-palette item; the palette lives
 *   at the shell layer, far above AskWorkbench's provider, so the flag
 *   has to travel through a store that both sides can observe.
 *
 * Not persisted — demo mode is a session-scoped affordance, not a
 * user preference.
 */

import { create } from "zustand";

interface AskUiStore {
  /** When true, AskWorkbench renders the mock demo conversation. */
  showDemo: boolean;
  setShowDemo: (value: boolean) => void;
}

export const useAskUiStore = create<AskUiStore>((set) => ({
  showDemo: false,
  setShowDemo: (value) => set({ showDemo: value }),
}));

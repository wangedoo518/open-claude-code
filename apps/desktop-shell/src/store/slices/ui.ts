import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

/**
 * NavSection — all possible navigation pages inside Home.
 * "session" is the code-session view (needs a sessionId separately via ViewMode).
 */
export type NavSection =
  | "overview"
  | "session"
  | "search"
  | "scheduled"
  | "dispatch"
  | "customize"
  | "openclaw"
  | "settings";

/**
 * ViewMode — unified navigation state that prevents conflicts between
 * homeSection and activeHomeSessionId. Exactly one of:
 *   - nav:     viewing a navigation page (overview, search, settings, etc.)
 *   - session: viewing a specific code session
 */
export type ViewMode =
  | { kind: "nav"; section: NavSection }
  | { kind: "session"; sessionId: string };

interface UiState {
  sidebarOpen: boolean;
  sidebarWidth: number;
  commandPaletteOpen: boolean;
  viewMode: ViewMode;
}

const initialState: UiState = {
  sidebarOpen: true,
  sidebarWidth: 240,
  commandPaletteOpen: false,
  viewMode: { kind: "nav", section: "overview" },
};

const uiSlice = createSlice({
  name: "ui",
  initialState,
  reducers: {
    toggleSidebar(state) {
      state.sidebarOpen = !state.sidebarOpen;
    },
    setSidebarOpen(state, action: PayloadAction<boolean>) {
      state.sidebarOpen = action.payload;
    },
    setSidebarWidth(state, action: PayloadAction<number>) {
      state.sidebarWidth = action.payload;
    },
    setCommandPaletteOpen(state, action: PayloadAction<boolean>) {
      state.commandPaletteOpen = action.payload;
    },
    /**
     * Set the unified view mode. Replaces both setHomeSection and setActiveHomeSessionId.
     *
     * Usage:
     *   dispatch(setViewMode({ kind: "nav", section: "settings" }))
     *   dispatch(setViewMode({ kind: "session", sessionId: "abc-123" }))
     */
    setViewMode(state, action: PayloadAction<ViewMode>) {
      state.viewMode = action.payload;
    },
  },
});

export const {
  toggleSidebar,
  setSidebarOpen,
  setSidebarWidth,
  setCommandPaletteOpen,
  setViewMode,
} = uiSlice.actions;
export default uiSlice.reducer;

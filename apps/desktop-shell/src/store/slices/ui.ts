import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

interface UiState {
  sidebarOpen: boolean;
  sidebarWidth: number;
  commandPaletteOpen: boolean;
  homeSection:
    | "overview"
    | "session"
    | "search"
    | "scheduled"
    | "dispatch"
    | "customize"
    | "openclaw"
    | "settings";
  activeHomeSessionId: string | null;
}

const initialState: UiState = {
  sidebarOpen: true,
  sidebarWidth: 240,
  commandPaletteOpen: false,
  homeSection: "overview",
  activeHomeSessionId: null,
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
    setHomeSection(state, action: PayloadAction<UiState["homeSection"]>) {
      state.homeSection = action.payload;
    },
    setActiveHomeSessionId(state, action: PayloadAction<string | null>) {
      state.activeHomeSessionId = action.payload;
    },
  },
});

export const {
  toggleSidebar,
  setSidebarOpen,
  setSidebarWidth,
  setCommandPaletteOpen,
  setHomeSection,
  setActiveHomeSessionId,
} = uiSlice.actions;
export default uiSlice.reducer;

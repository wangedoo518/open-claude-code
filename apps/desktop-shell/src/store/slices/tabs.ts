import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

export type TabType =
  | "home"
  | "apps"
  | "code"
  | "minapp";

export interface Tab {
  id: string;
  type: TabType;
  path: string;
  title: string;
  icon?: string;
  closable: boolean;
  sessionId?: string;
}

interface TabsState {
  tabs: Tab[];
  activeTabId: string;
}

/**
 * System tabs are removed — navigation (首页/应用/设置) now lives
 * in Row 1 as fixed buttons driven by ViewMode.
 * Only session/minapp tabs remain in Redux for Row 2.
 */
const initialState: TabsState = {
  tabs: [],
  activeTabId: "",
};

const tabsSlice = createSlice({
  name: "tabs",
  initialState,
  reducers: {
    addTab(state, action: PayloadAction<Tab>) {
      const exists = state.tabs.find((t) => t.id === action.payload.id);
      if (!exists) {
        state.tabs.push(action.payload);
      }
      state.activeTabId = action.payload.id;
    },
    removeTab(state, action: PayloadAction<string>) {
      const idx = state.tabs.findIndex((t) => t.id === action.payload);
      if (idx === -1) return;
      const tab = state.tabs[idx];
      if (!tab.closable) return;

      state.tabs.splice(idx, 1);
      if (state.activeTabId === action.payload) {
        const newIdx = Math.min(idx, state.tabs.length - 1);
        state.activeTabId = state.tabs[newIdx]?.id ?? "";
      }
    },
    setActiveTab(state, action: PayloadAction<string>) {
      state.activeTabId = action.payload;
    },
    reorderTabs(
      state,
      action: PayloadAction<{ fromIndex: number; toIndex: number }>
    ) {
      const { fromIndex, toIndex } = action.payload;
      const [moved] = state.tabs.splice(fromIndex, 1);
      state.tabs.splice(toIndex, 0, moved);
    },
    updateTabTitle(
      state,
      action: PayloadAction<{ id: string; title: string }>
    ) {
      const tab = state.tabs.find((t) => t.id === action.payload.id);
      if (tab) tab.title = action.payload.title;
    },
    updateTabSession(
      state,
      action: PayloadAction<{ id: string; sessionId: string; title?: string }>
    ) {
      const tab = state.tabs.find((t) => t.id === action.payload.id);
      if (!tab) return;
      tab.sessionId = action.payload.sessionId;
      if (action.payload.title) {
        tab.title = action.payload.title;
      }
    },
  },
});

export const {
  addTab,
  removeTab,
  setActiveTab,
  reorderTabs,
  updateTabSession,
  updateTabTitle,
} = tabsSlice.actions;
export default tabsSlice.reducer;

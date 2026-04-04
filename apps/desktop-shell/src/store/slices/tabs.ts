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

export const SYSTEM_TABS: Tab[] = [
  {
    id: "home",
    type: "home",
    path: "/home",
    title: "首页",
    closable: false,
  },
  {
    id: "apps",
    type: "apps",
    path: "/apps",
    title: "应用",
    closable: false,
  },
];

const initialState: TabsState = {
  tabs: SYSTEM_TABS,
  activeTabId: "home",
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
    ensureSystemTabs(state) {
      for (const sysTab of SYSTEM_TABS) {
        if (!state.tabs.some((t) => t.id === sysTab.id)) {
          state.tabs.unshift(sysTab);
        }
      }
      if (!state.tabs.some((t) => t.id === state.activeTabId)) {
        state.activeTabId = SYSTEM_TABS[0].id;
      }
    },
    removeTab(state, action: PayloadAction<string>) {
      const idx = state.tabs.findIndex((t) => t.id === action.payload);
      if (idx === -1) return;
      const tab = state.tabs[idx];
      if (!tab.closable) return;

      state.tabs.splice(idx, 1);
      if (state.activeTabId === action.payload) {
        const newIdx = Math.min(idx, state.tabs.length - 1);
        state.activeTabId = state.tabs[newIdx]?.id ?? SYSTEM_TABS[0].id;
      }
    },
    setActiveTab(state, action: PayloadAction<string>) {
      if (state.tabs.some((t) => t.id === action.payload)) {
        state.activeTabId = action.payload;
      }
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
  ensureSystemTabs,
  removeTab,
  setActiveTab,
  reorderTabs,
  updateTabSession,
  updateTabTitle,
} = tabsSlice.actions;
export default tabsSlice.reducer;

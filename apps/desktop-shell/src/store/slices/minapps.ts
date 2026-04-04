import { createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type { MinAppType } from "@/types/minapp";
import { BUILTIN_APPS } from "@/config/minapps";

/**
 * MinApp runtime state, mirroring cherry-studio's runtime + minapps slices.
 *
 * - `enabled`  – visible apps in the gallery grid
 * - `disabled` – hidden by user
 * - `pinned`   – pinned to quick access
 * - `openedKeepAliveApps` – currently open with keep-alive webview
 * - `currentAppId` – the active/foreground app
 * - `appShow`  – whether the popup/view is visible (sidebar mode)
 */
interface MinAppsState {
  enabled: MinAppType[];
  disabled: MinAppType[];
  pinned: MinAppType[];
  openedKeepAliveApps: MinAppType[];
  currentAppId: string;
  appShow: boolean;
}

const initialState: MinAppsState = {
  enabled: [...BUILTIN_APPS],
  disabled: [],
  pinned: [],
  openedKeepAliveApps: [],
  currentAppId: "",
  appShow: false,
};

const minappsSlice = createSlice({
  name: "minapps",
  initialState,
  reducers: {
    setEnabledApps(state, action: PayloadAction<MinAppType[]>) {
      state.enabled = action.payload;
    },
    setDisabledApps(state, action: PayloadAction<MinAppType[]>) {
      state.disabled = action.payload;
    },
    setPinnedApps(state, action: PayloadAction<MinAppType[]>) {
      state.pinned = action.payload;
    },
    setOpenedKeepAliveApps(state, action: PayloadAction<MinAppType[]>) {
      state.openedKeepAliveApps = action.payload;
    },
    setCurrentAppId(state, action: PayloadAction<string>) {
      state.currentAppId = action.payload;
    },
    setAppShow(state, action: PayloadAction<boolean>) {
      state.appShow = action.payload;
    },
    addOpenedApp(state, action: PayloadAction<MinAppType>) {
      const exists = state.openedKeepAliveApps.some(
        (a) => a.id === action.payload.id
      );
      if (!exists) {
        state.openedKeepAliveApps.push(action.payload);
      }
      state.currentAppId = action.payload.id;
    },
    removeOpenedApp(state, action: PayloadAction<string>) {
      state.openedKeepAliveApps = state.openedKeepAliveApps.filter(
        (a) => a.id !== action.payload
      );
      if (state.currentAppId === action.payload) {
        state.currentAppId =
          state.openedKeepAliveApps[
            state.openedKeepAliveApps.length - 1
          ]?.id ?? "";
      }
    },
  },
});

export const {
  setEnabledApps,
  setDisabledApps,
  setPinnedApps,
  setOpenedKeepAliveApps,
  setCurrentAppId,
  setAppShow,
  addOpenedApp,
  removeOpenedApp,
} = minappsSlice.actions;

export default minappsSlice.reducer;

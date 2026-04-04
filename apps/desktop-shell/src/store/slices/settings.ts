import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

export type ThemeMode = "light" | "dark" | "system";
export type PermissionMode = "auto" | "ask" | "danger";

export interface ProviderConfig {
  type: "anthropic" | "openai" | "openrouter" | "custom";
  apiKey: string;
  baseUrl: string;
}

export interface AppSettings {
  theme: ThemeMode;
  warwolfTheme: boolean;
  language: string;
  fontSize: number;
  defaultModel: string;
  permissionMode: PermissionMode;
  defaultProjectPath: string;
  provider: ProviderConfig;
  showSessionSidebar: boolean;
}

const initialState: AppSettings = {
  theme: "system",
  warwolfTheme: true,
  language: "en",
  fontSize: 14,
  defaultModel: "claude-opus-4-6",
  permissionMode: "danger",
  defaultProjectPath: "",
  provider: {
    type: "anthropic",
    apiKey: "",
    baseUrl: "https://api.anthropic.com",
  },
  showSessionSidebar: true,
};

const settingsSlice = createSlice({
  name: "settings",
  initialState,
  reducers: {
    setTheme(state, action: PayloadAction<ThemeMode>) {
      state.theme = action.payload;
    },
    setWarwolfTheme(state, action: PayloadAction<boolean>) {
      state.warwolfTheme = action.payload;
    },
    setLanguage(state, action: PayloadAction<string>) {
      state.language = action.payload;
    },
    setFontSize(state, action: PayloadAction<number>) {
      state.fontSize = action.payload;
    },
    setDefaultModel(state, action: PayloadAction<string>) {
      state.defaultModel = action.payload;
    },
    setPermissionMode(state, action: PayloadAction<PermissionMode>) {
      state.permissionMode = action.payload;
    },
    setDefaultProjectPath(state, action: PayloadAction<string>) {
      state.defaultProjectPath = action.payload;
    },
    setProvider(state, action: PayloadAction<Partial<ProviderConfig>>) {
      state.provider = { ...state.provider, ...action.payload };
    },
    setShowSessionSidebar(state, action: PayloadAction<boolean>) {
      state.showSessionSidebar = action.payload;
    },
    updateSettings(state, action: PayloadAction<Partial<AppSettings>>) {
      return { ...state, ...action.payload };
    },
  },
});

export const {
  setTheme,
  setWarwolfTheme,
  setLanguage,
  setFontSize,
  setDefaultModel,
  setPermissionMode,
  setDefaultProjectPath,
  setProvider,
  setShowSessionSidebar,
  updateSettings,
} = settingsSlice.actions;
export default settingsSlice.reducer;

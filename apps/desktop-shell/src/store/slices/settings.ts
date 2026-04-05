import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

export type ThemeMode = "light" | "dark" | "system";
export type PermissionMode =
  | "default"
  | "acceptEdits"
  | "bypassPermissions"
  | "plan";
export type McpTransport = "stdio" | "sse" | "http" | "ws" | "sdk";
export type McpScope = "local" | "user" | "project";

export interface ProviderConfig {
  type: "anthropic" | "openai" | "openrouter" | "custom";
  apiKey: string;
  baseUrl: string;
}

export interface UserMcpServer {
  id: string;
  name: string;
  transport: McpTransport;
  target: string;
  scope: McpScope;
  enabled: boolean;
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
  mcpServers: UserMcpServer[];
}

const initialState: AppSettings = {
  theme: "system",
  warwolfTheme: true,
  language: "en",
  fontSize: 14,
  defaultModel: "claude-opus-4-6",
  permissionMode: "default",
  defaultProjectPath: "",
  provider: {
    type: "anthropic",
    apiKey: "",
    baseUrl: "https://api.anthropic.com",
  },
  showSessionSidebar: true,
  mcpServers: [],
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
    addMcpServer(state, action: PayloadAction<UserMcpServer>) {
      if (!state.mcpServers) state.mcpServers = [];
      state.mcpServers.push(action.payload);
    },
    updateMcpServer(
      state,
      action: PayloadAction<{ id: string; updates: Partial<UserMcpServer> }>
    ) {
      const servers = state.mcpServers ?? [];
      const idx = servers.findIndex(
        (s) => s.id === action.payload.id
      );
      if (idx >= 0) {
        servers[idx] = {
          ...servers[idx],
          ...action.payload.updates,
        };
        state.mcpServers = servers;
      }
    },
    removeMcpServer(state, action: PayloadAction<string>) {
      state.mcpServers = (state.mcpServers ?? []).filter(
        (s) => s.id !== action.payload
      );
    },
    toggleMcpServer(state, action: PayloadAction<string>) {
      const server = (state.mcpServers ?? []).find((s) => s.id === action.payload);
      if (server) {
        server.enabled = !server.enabled;
      }
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
  addMcpServer,
  updateMcpServer,
  removeMcpServer,
  toggleMcpServer,
} = settingsSlice.actions;
export default settingsSlice.reducer;

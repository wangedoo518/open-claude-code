import { configureStore, combineReducers } from "@reduxjs/toolkit";
import {
  persistStore,
  persistReducer,
  createTransform,
  FLUSH,
  REHYDRATE,
  PAUSE,
  PERSIST,
  PURGE,
  REGISTER,
} from "redux-persist";
import storage from "redux-persist/lib/storage";
import {
  type TypedUseSelectorHook,
  useDispatch,
  useSelector,
} from "react-redux";
import { CODE_TOOL_IDS, DEFAULT_CODE_TOOL } from "@/features/code-tools";

import tabsReducer from "./slices/tabs";
import settingsReducer from "./slices/settings";
import uiReducer from "./slices/ui";
import minappsReducer from "./slices/minapps";
import codeToolsReducer from "./slices/codeTools";
import permissionsReducer from "./slices/permissions";

const rootReducer = combineReducers({
  tabs: tabsReducer,
  settings: settingsReducer,
  ui: uiReducer,
  minapps: minappsReducer,
  codeTools: codeToolsReducer,
  permissions: permissionsReducer,
});

// Strip sensitive fields before persisting to localStorage
const sanitizeSettingsTransform = createTransform(
  // inbound: called when persisting (state → storage)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (inboundState: any) => {
    if (!inboundState || typeof inboundState !== "object") return inboundState;
    const state = { ...inboundState };
    // Never persist API keys or secrets to localStorage
    if (state.provider && typeof state.provider === "object") {
      state.provider = { ...state.provider, apiKey: "" };
    }
    return state;
  },
  // outbound: called when rehydrating (storage → state) — pass through
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (outboundState: any) => outboundState,
  { whitelist: ["settings"] }
);

const validCodeToolIds = new Set<string>(CODE_TOOL_IDS);

function normalizeCodeToolsState(outboundState: unknown) {
  if (!outboundState || typeof outboundState !== "object") {
    return outboundState;
  }

  const state = outboundState as Record<string, unknown>;
  const selectedCliTool =
    typeof state.selectedCliTool === "string" &&
    validCodeToolIds.has(state.selectedCliTool)
      ? state.selectedCliTool
      : DEFAULT_CODE_TOOL;
  const selectedModels =
    state.selectedModels && typeof state.selectedModels === "object"
      ? (state.selectedModels as Record<string, unknown>)
      : {};
  const environmentVariables =
    state.environmentVariables && typeof state.environmentVariables === "object"
      ? (state.environmentVariables as Record<string, unknown>)
      : {};

  return {
    ...state,
    selectedCliTool,
    selectedModels: Object.fromEntries(
      CODE_TOOL_IDS.map((toolId) => [toolId, selectedModels[toolId] ?? null])
    ),
    environmentVariables: Object.fromEntries(
      CODE_TOOL_IDS.map((toolId) => [
        toolId,
        typeof environmentVariables[toolId] === "string"
          ? environmentVariables[toolId]
          : "",
      ])
    ),
  };
}

const normalizeCodeToolsTransform = createTransform(
  (inboundState: unknown) => normalizeCodeToolsState(inboundState),
  (outboundState: unknown) => normalizeCodeToolsState(outboundState),
  { whitelist: ["codeTools"] }
);

const persistConfig = {
  key: "open-claude-code",
  version: 1,
  storage,
  blacklist: ["ui", "permissions"],
  transforms: [sanitizeSettingsTransform, normalizeCodeToolsTransform],
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const persistedReducer = persistReducer(persistConfig, rootReducer as any);

export const store = configureStore({
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  reducer: persistedReducer as any,
  middleware: (getDefaultMiddleware) =>
    getDefaultMiddleware({
      serializableCheck: {
        ignoredActions: [FLUSH, REHYDRATE, PAUSE, PERSIST, PURGE, REGISTER],
      },
    }),
});

export const persistor = persistStore(store);

export type RootState = ReturnType<typeof rootReducer>;
export type AppDispatch = typeof store.dispatch;

export const useAppDispatch: () => AppDispatch = useDispatch;
export const useAppSelector: TypedUseSelectorHook<RootState> = useSelector;

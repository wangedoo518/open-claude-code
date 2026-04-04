import { configureStore, combineReducers } from "@reduxjs/toolkit";
import {
  persistStore,
  persistReducer,
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

import tabsReducer from "./slices/tabs";
import sessionsReducer from "./slices/sessions";
import settingsReducer from "./slices/settings";
import uiReducer from "./slices/ui";
import minappsReducer from "./slices/minapps";
import codeToolsReducer from "./slices/codeTools";

const rootReducer = combineReducers({
  tabs: tabsReducer,
  sessions: sessionsReducer,
  settings: settingsReducer,
  ui: uiReducer,
  minapps: minappsReducer,
  codeTools: codeToolsReducer,
});

const persistConfig = {
  key: "open-claude-code",
  version: 1,
  storage,
  blacklist: ["sessions", "ui"],
};

const persistedReducer = persistReducer(persistConfig, rootReducer);

export const store = configureStore({
  reducer: persistedReducer,
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

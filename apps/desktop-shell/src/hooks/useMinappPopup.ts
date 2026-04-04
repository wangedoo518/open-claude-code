import { useCallback, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { useAppDispatch, useAppSelector } from "@/store";
import {
  addOpenedApp,
  removeOpenedApp,
  setCurrentAppId,
  setAppShow,
  setOpenedKeepAliveApps,
} from "@/store/slices/minapps";
import { clearWebviewState } from "@/utils/webviewStateManager";
import { findAppById } from "@/config/minapps";
import type { MinAppType } from "@/types/minapp";

/**
 * Primary control hook for opening / closing MinApps.
 *
 * In top-tab mode: navigates to `/apps/:id` and adds to keep-alive pool.
 * Mirrors cherry-studio's useMinappPopup.ts
 */
export function useMinappPopup() {
  const dispatch = useAppDispatch();
  const navigate = useNavigate();
  const openedKeepAliveApps = useAppSelector(
    (s) => s.minapps.openedKeepAliveApps
  );
  const currentAppId = useAppSelector((s) => s.minapps.currentAppId);

  // Keep a ref to avoid stale closures in closeAllMinapps
  const openedAppsRef = useRef(openedKeepAliveApps);
  openedAppsRef.current = openedKeepAliveApps;

  const openMinappKeepAlive = useCallback(
    (app: MinAppType) => {
      dispatch(addOpenedApp(app));
      dispatch(setAppShow(true));
    },
    [dispatch]
  );

  const openMinapp = useCallback(
    (app: MinAppType) => {
      openMinappKeepAlive(app);
      navigate(`/apps/${app.id}`);
    },
    [openMinappKeepAlive, navigate]
  );

  const openSmartMinapp = useCallback(
    (app: MinAppType) => {
      openMinappKeepAlive(app);
      navigate(`/apps/${app.id}`);
    },
    [openMinappKeepAlive, navigate]
  );

  const openMinappById = useCallback(
    (id: string) => {
      const app = findAppById(id);
      if (app) openMinapp(app);
    },
    [openMinapp]
  );

  const closeMinapp = useCallback(
    (appId: string) => {
      dispatch(removeOpenedApp(appId));
      clearWebviewState(appId);
    },
    [dispatch]
  );

  const closeAllMinapps = useCallback(() => {
    for (const app of openedAppsRef.current) {
      clearWebviewState(app.id);
    }
    dispatch(setOpenedKeepAliveApps([]));
    dispatch(setCurrentAppId(""));
    dispatch(setAppShow(false));
  }, [dispatch]);

  const hideMinappPopup = useCallback(() => {
    dispatch(setAppShow(false));
  }, [dispatch]);

  return {
    openMinapp,
    openMinappKeepAlive,
    openSmartMinapp,
    openMinappById,
    closeMinapp,
    closeAllMinapps,
    hideMinappPopup,
    openedKeepAliveApps,
    currentAppId,
  };
}

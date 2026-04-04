import { useMemo } from "react";
import { useLocation } from "react-router-dom";
import { useAppSelector } from "@/store";
import { AppContainer } from "./NativeAppContainer";
import { cn } from "@/lib/utils";


/**
 * MinApp WebView/content pool for tab mode.
 *
 * Mirrors cherry-studio's MinAppTabsPool.tsx:
 * - Only visible when navigated to /apps/:appId
 * - Keeps all opened apps mounted in the DOM (keep-alive)
 * - Shows only the current active app via visibility toggling
 *
 * The toolbar offset is handled by the parent flex layout in
 * MinAppDetailPage, so this component fills its container fully.
 */
export function MinAppTabsPool() {
  const openedApps = useAppSelector((s) => s.minapps.openedKeepAliveApps);
  const currentAppId = useAppSelector((s) => s.minapps.currentAppId);
  const location = useLocation();

  const isAppDetail = useMemo(() => {
    const pathname = location.pathname;
    if (pathname === "/apps") return false;
    if (!pathname.startsWith("/apps/")) return false;
    const parts = pathname.split("/").filter(Boolean);
    return parts.length >= 2;
  }, [location.pathname]);

  const shouldShow = isAppDetail;

  return (
    <div
      className="absolute inset-0 overflow-hidden"
      style={
        shouldShow
          ? { visibility: "visible", zIndex: 1 }
          : { visibility: "hidden" }
      }
      data-minapp-tabs-pool
      aria-hidden={!shouldShow}
    >
      {openedApps.map((app) => (
        <div
          key={app.id}
          className={cn(
            "absolute inset-0 h-full w-full",
            app.id === currentAppId && shouldShow
              ? "pointer-events-auto"
              : "pointer-events-none invisible"
          )}
        >
          <AppContainer
            app={app}
            visible={app.id === currentAppId && shouldShow}
          />
        </div>
      ))}
    </div>
  );
}

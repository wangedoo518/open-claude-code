import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Loader2 } from "lucide-react";
import { useMinappPopup } from "@/hooks/useMinappPopup";
import { useMinapps } from "@/hooks/useMinapps";
import { MinimalToolbar } from "@/components/MinApp/MinimalToolbar";
import { MinAppTabsPool } from "@/components/MinApp/MinAppTabsPool";
import { MinAppIcon } from "@/components/MinApp/MinAppIcon";
import { getAllMinApps } from "@/config/minapps";
import {
  getWebviewLoaded,
  onWebviewStateChange,
} from "@/utils/webviewStateManager";

/**
 * Detail page rendered at `/apps/:appId`.
 *
 * This is the tab-mode shell for an individual MinApp. It:
 * 1. Finds the app by id
 * 2. Opens it in the keep-alive pool
 * 3. Renders MinimalToolbar on top
 * 4. Renders MinAppTabsPool underneath (which contains the actual content)
 *
 * Mirrors cherry-studio's MinAppPage.tsx
 */
export function MinAppDetailPage() {
  const { appId } = useParams<{ appId: string }>();
  const { openMinappKeepAlive } = useMinappPopup();
  const { minapps } = useMinapps();
  const navigate = useNavigate();

  // Find the app from all available sources
  const app = useMemo(() => {
    if (!appId) return null;
    return (
      [...getAllMinApps(), ...minapps].find((a) => a.id === appId) ?? null
    );
  }, [appId, minapps]);

  // Open app in keep-alive pool on mount
  useEffect(() => {
    if (!app) {
      navigate("/apps");
      return;
    }
    openMinappKeepAlive(app);
  }, [app, navigate, openMinappKeepAlive]);

  // Track loaded state for native apps
  const isNative = app?.url.startsWith("warwolf://") ?? false;
  const [isReady, setIsReady] = useState(() =>
    app ? isNative || getWebviewLoaded(app.id) : false
  );

  useEffect(() => {
    if (!app || isNative) {
      setIsReady(true);
      return;
    }
    if (getWebviewLoaded(app.id)) {
      setIsReady(true);
      return;
    }
    const unsub = onWebviewStateChange(app.id, (loaded) => {
      if (loaded) {
        setIsReady(true);
        unsub();
      }
    });
    return unsub;
  }, [app, isNative]);

  if (!app) return null;

  return (
    <div className="relative flex h-full w-full flex-col">
      {/* Toolbar (z-index above pool) */}
      <div className="relative z-10 shrink-0">
        <MinimalToolbar app={app} isNative={isNative} />
      </div>

      {/* Content pool (keep-alive apps) */}
      <div className="relative flex-1">
        <MinAppTabsPool />
      </div>

      {/* Loading mask (only for non-native apps) */}
      {!isReady && (
        <div
          className="absolute inset-x-0 bottom-0 z-20 flex flex-col items-center justify-center gap-3 bg-background"
          style={{ top: 35 }}
        >
          <MinAppIcon app={app} size={60} />
          <Loader2 className="size-5 animate-spin text-muted-foreground" />
        </div>
      )}
    </div>
  );
}

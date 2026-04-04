import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { Webview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getMinAppWebviewLabel } from "@/config/minapps";
import { OpenClawPage } from "@/features/workbench/OpenClawPage";
import type { MinAppType } from "@/types/minapp";
import { setWebviewLoaded } from "@/utils/webviewStateManager";

interface NativeAppContainerProps {
  app: MinAppType;
  visible?: boolean;
}

/**
 * Container for built-in (native React) apps.
 *
 * Detects the `warwolf://` protocol URL and renders the corresponding
 * React component. External URL apps would use an iframe instead.
 *
 * This replaces cherry-studio's WebviewContainer for built-in apps.
 */
export function NativeAppContainer({ app }: NativeAppContainerProps) {
  const route = app.url.replace("warwolf://", "");

  switch (route) {
    case "openclaw":
      return <OpenClawPage />;
    default:
      return (
        <div className="flex h-full items-center justify-center text-muted-foreground">
          Unknown app: {app.url}
        </div>
      );
  }
}

/**
 * Native child-webview container for external URL apps.
 *
 * OpenClaw's dashboard rejects iframe embedding (`X-Frame-Options: DENY`),
 * so tab-mode external apps must use a real Tauri child webview.
 */
function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function IframeAppContainer({ app }: NativeAppContainerProps) {
  return (
    <iframe
      src={app.url}
      className="h-full w-full border-0"
      sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
      title={app.name}
      data-minapp-id={app.id}
      onLoad={() => setWebviewLoaded(app.id, true)}
      onError={() => setWebviewLoaded(app.id, false)}
    />
  );
}

function ExternalAppContainer({
  app,
  visible = true,
}: NativeAppContainerProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const webviewRef = useRef<Webview | null>(null);
  const currentUrlRef = useRef(app.url);
  const [fallbackToIframe, setFallbackToIframe] = useState(!isTauriRuntime());
  const label = useMemo(() => getMinAppWebviewLabel(app.id), [app.id]);

  const syncBounds = useEffectEvent(async () => {
    const host = hostRef.current;
    const webview = webviewRef.current;
    if (!host || !webview) {
      return;
    }

    const rect = host.getBoundingClientRect();
    const width = Math.max(1, Math.round(rect.width));
    const height = Math.max(1, Math.round(rect.height));

    if (width < 1 || height < 1) {
      return;
    }

    await Promise.all([
      webview.setPosition(
        new LogicalPosition(Math.round(rect.left), Math.round(rect.top))
      ),
      webview.setSize(new LogicalSize(width, height)),
    ]);
  });

  const syncVisibility = useEffectEvent(async () => {
    const webview = webviewRef.current;
    if (!webview) {
      return;
    }

    if (visible) {
      await webview.show();
    } else {
      await webview.hide();
    }
  });

  useEffect(() => {
    if (fallbackToIframe) {
      return;
    }

    let cancelled = false;

    async function ensureWebview() {
      let webview = await Webview.getByLabel(label);

      if (webview && currentUrlRef.current !== app.url) {
        await webview.close().catch(() => undefined);
        webview = null;
      }

      if (!webview) {
        const created = new Webview(getCurrentWindow(), label, {
          url: app.url,
          x: 0,
          y: 0,
          width: 1,
          height: 1,
          focus: false,
          devtools: import.meta.env.DEV,
        });

        await new Promise<void>((resolve, reject) => {
          void created.once("tauri://created", () => resolve());
          void created.once("tauri://error", (event) => {
            reject(new Error(String(event.payload ?? "Unknown webview error")));
          });
        });

        webview = created;
      }

      if (cancelled) {
        await webview.close().catch(() => undefined);
        return;
      }

      currentUrlRef.current = app.url;
      webviewRef.current = webview;
      await syncBounds();
      await syncVisibility();
      setWebviewLoaded(app.id, true);
    }

    void ensureWebview().catch((error) => {
      console.error("Failed to create child webview for minapp", error);
      setWebviewLoaded(app.id, false);
      setFallbackToIframe(true);
    });

    return () => {
      cancelled = true;
      const webview = webviewRef.current;
      webviewRef.current = null;
      setWebviewLoaded(app.id, false);
      if (webview) {
        void webview.close().catch(() => undefined);
      }
    };
  }, [app.id, app.url, fallbackToIframe, label, syncBounds, syncVisibility]);

  useEffect(() => {
    if (fallbackToIframe) {
      return;
    }

    let frame = 0;
    const host = hostRef.current;
    if (!host) {
      return;
    }

    const scheduleSync = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        void syncBounds();
      });
    };

    scheduleSync();

    const observer = new ResizeObserver(() => {
      scheduleSync();
    });
    observer.observe(host);
    window.addEventListener("resize", scheduleSync);

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", scheduleSync);
    };
  }, [fallbackToIframe, syncBounds]);

  useEffect(() => {
    if (fallbackToIframe) {
      return;
    }
    void syncVisibility();
  }, [fallbackToIframe, syncVisibility, visible]);

  if (fallbackToIframe) {
    return <IframeAppContainer app={app} visible={visible} />;
  }

  return <div ref={hostRef} className="h-full w-full bg-background" />;
}

/**
 * Resolves the correct container for a MinApp based on its URL protocol.
 */
export function AppContainer({
  app,
  visible = true,
}: NativeAppContainerProps) {
  if (app.url.startsWith("warwolf://")) {
    return <NativeAppContainer app={app} visible={visible} />;
  }
  return <ExternalAppContainer app={app} visible={visible} />;
}

import type { MinAppType } from "@/types/minapp";
import OpenClawLogo from "@/assets/openclaw-logo.svg";

export const MINAPP_ROUTE_PREFIX = "/apps/";
export const MINAPP_TAB_PREFIX = "apps:";
export const MINAPP_WEBVIEW_PREFIX = "minapp-webview:";

/**
 * Built-in app definitions.
 *
 * OpenClaw remains a built-in native page rendered through the
 * MinApp container. `Code` has been promoted to a top-level `/code`
 * route and is no longer modeled as a MinApp.
 */
export const BUILTIN_APPS: MinAppType[] = [
  {
    id: "openclaw",
    name: "OpenClaw",
    url: "warwolf://openclaw",
    logo: OpenClawLogo,
    type: "builtin",
    iconName: "Shell",
    gradient: "",
    description: "Provider hub and agent management",
  },
];

/**
 * All available mini-apps (built-in + custom).
 * Custom apps will be loaded from a JSON file at runtime.
 */
let allMinApps: MinAppType[] = [...BUILTIN_APPS];

export function getAllMinApps(): MinAppType[] {
  return allMinApps;
}

export function setAllMinApps(apps: MinAppType[]) {
  allMinApps = apps;
}

export function findAppById(id: string): MinAppType | undefined {
  return allMinApps.find((app) => app.id === id);
}

export function resolveMinApp(
  appId: string,
  ...sources: Array<readonly MinAppType[]>
): MinAppType | undefined {
  for (const source of sources) {
    const app = source.find((item) => item.id === appId);
    if (app) {
      return app;
    }
  }
  return findAppById(appId);
}

export function getMinAppIdFromPath(pathname: string): string | null {
  if (!pathname.startsWith(MINAPP_ROUTE_PREFIX)) {
    return null;
  }

  const appId = pathname.slice(MINAPP_ROUTE_PREFIX.length).split("/")[0];
  return appId || null;
}

export function getMinAppTabId(appId: string): string {
  return `${MINAPP_TAB_PREFIX}${appId}`;
}

export function getMinAppIdFromTabId(tabId: string): string | null {
  if (!tabId.startsWith(MINAPP_TAB_PREFIX)) {
    return null;
  }

  const appId = tabId.slice(MINAPP_TAB_PREFIX.length);
  return appId || null;
}

export function getMinAppWebviewLabel(appId: string): string {
  return `${MINAPP_WEBVIEW_PREFIX}${appId}`;
}

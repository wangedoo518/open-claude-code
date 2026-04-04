import type { MinAppType } from "@/types/minapp";
import WarwolfLogo from "@/assets/warwolf-logo.png";
import OpenClawLogo from "@/assets/openclaw-logo.svg";

/**
 * Built-in app definitions.
 *
 * Code uses a special `warwolf://code` protocol URL — the webview
 * container detects this and renders the native CodePage component.
 *
 * OpenClaw uses the same approach — `warwolf://openclaw` renders
 * the native OpenClawPage.
 */
export const BUILTIN_APPS: MinAppType[] = [
  {
    id: "code",
    name: "Code",
    url: "warwolf://code",
    logo: WarwolfLogo,
    type: "builtin",
    iconName: "Terminal",
    gradient: "",
    description: "Claude Code style terminal workspace",
  },
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

import type { NavigateFunction } from "react-router-dom";

export type NavSection =
  | "overview"
  | "session"
  | "search"
  | "scheduled"
  | "dispatch"
  | "customize"
  | "openclaw"
  | "settings";

const NAV_SECTIONS = new Set<NavSection>([
  "overview",
  "session",
  "search",
  "scheduled",
  "dispatch",
  "customize",
  "openclaw",
  "settings",
]);

export interface HomeRouteState {
  section: NavSection;
  sessionId: string | null;
}

export function parseHomeRouteState(search: string): HomeRouteState {
  const params = new URLSearchParams(search);
  const sessionId = params.get("sessionId");
  if (sessionId) {
    return { section: "session", sessionId };
  }

  const section = params.get("section");
  if (section && NAV_SECTIONS.has(section as NavSection)) {
    return { section: section as NavSection, sessionId: null };
  }

  return { section: "overview", sessionId: null };
}

export function buildHomeSectionHref(section: NavSection): string {
  return section === "overview" ? "/home" : `/home?section=${section}`;
}

export function buildHomeSessionHref(sessionId?: string | null): string {
  if (!sessionId) {
    return "/home?section=session";
  }

  return `/home?sessionId=${encodeURIComponent(sessionId)}`;
}

export function openHomeSession(
  navigate: NavigateFunction,
  sessionId: string | null
) {
  navigate(buildHomeSessionHref(sessionId));
}

export function openHomeOverview(navigate: NavigateFunction) {
  navigate(buildHomeSectionHref("overview"));
}

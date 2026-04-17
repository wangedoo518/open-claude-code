import { useEffect, useMemo } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { useSettingsStore } from "@/state/settings-store";
import {
  CLAWWIKI_ROUTES,
  type ClawWikiRoute,
  type ClawWikiSection,
} from "./clawwiki-routes";
import { useAskSessionContext } from "@/features/ask/AskSessionContext";
import { SessionSidebar } from "@/features/ask/SessionSidebar";
import { WikiFileTree } from "@/features/wiki/WikiFileTree";
import { WeChatStatusBadge } from "@/features/wechat-kefu/WeChatStatusBadge";
import {
  Sidebar as UiSidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";

/**
 * ClawWiki canonical Sidebar — Rowboat-style (256px expanded / 48px
 * icon-mode).
 *
 * This file is a thin adapter around the shared
 * `components/ui/sidebar.tsx` primitives (SidebarProvider lives in
 * ClawWikiShell). It renders:
 *
 *   Header  → logo + title + ModeToggle (Chat | Wiki)
 *   Content → PRIMARY group (7 routes) + FUNNEL group (1 route)
 *   Footer  → Settings link
 *
 * Design decisions captured here:
 * - Expanded width (256px) and icon-mode width (3rem / 48px) come from
 *   the shared sidebar.tsx CSS variables. Don't hard-code pixel widths
 *   in this file.
 * - Auto-collapse below 760px is handled inside SidebarProvider via
 *   matchMedia — we don't touch it here.
 * - Active state comes from `useLocation` (not zustand) so
 *   back/forward browser history and deep links work without extra
 *   plumbing.
 * - Badge overrides: Inbox's pending count comes from a
 *   react-query-backed useQuery; all other routes fall back to the
 *   static `badge` field on the route definition (currently none).
 */

function groupBySection(
  routes: readonly ClawWikiRoute[],
): Record<ClawWikiSection, ClawWikiRoute[]> {
  const grouped: Record<ClawWikiSection, ClawWikiRoute[]> = {
    primary: [],
    funnel: [],
    settings: [],
  };
  for (const r of routes) {
    grouped[r.section].push(r);
  }
  return grouped;
}

/** Match the exact path or any subpath (e.g. /ask/:sessionId → /ask). */
function isActive(currentPath: string, itemPath: string): boolean {
  if (currentPath === itemPath) return true;
  return currentPath.startsWith(`${itemPath}/`);
}

export function AppSidebar() {
  const location = useLocation();
  const appMode = useSettingsStore((s) => s.appMode);
  const grouped = useMemo(() => groupBySection(CLAWWIKI_ROUTES), []);

  const settingsRoute = grouped.settings[0];

  return (
    <UiSidebar collapsible="icon">
      <SidebarHeader className="gap-0 p-0">
        {/* Logo row */}
        <div className="flex h-14 flex-shrink-0 items-center gap-2.5 border-b border-sidebar-border px-3">
          <div className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg bg-primary font-bold text-primary-foreground">
            C
          </div>
          <div className="flex flex-col leading-tight group-data-[collapsible=icon]:hidden">
            <span className="text-sm font-semibold text-foreground">
              ClawWiki
            </span>
            <span className="text-[10px] text-muted-foreground">你的外脑</span>
          </div>
        </div>

        {/* v2 Chat/Wiki mode toggle — ia-layout.md §2 */}
        <ModeToggle />
      </SidebarHeader>

      <SidebarContent className="group-data-[collapsible=icon]:hidden">
        {appMode === "chat" ? <ChatSidebarContent /> : <WikiSidebarContent />}
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <WeChatStatusBadge />
          {settingsRoute && (
            <RouteItem
              route={settingsRoute}
              active={isActive(location.pathname, settingsRoute.path)}
              badge={undefined}
            />
          )}
        </SidebarMenu>
      </SidebarFooter>
    </UiSidebar>
  );
}

// Back-compat default export name. ClawWikiShell still imports
// `Sidebar` — keep the alias so other call sites (if any) don't break.
export { AppSidebar as Sidebar };

/**
 * Chat mode sidebar content — renders SessionSidebar (conversation list).
 */
function ChatSidebarContent() {
  const { sessionId, onSwitchSession, onResetSession } =
    useAskSessionContext();
  const navigate = useNavigate();

  return (
    <SessionSidebar
      activeSessionId={sessionId}
      onSelectSession={(id) => {
        onSwitchSession(id);
        navigate("/ask");
      }}
      onNewSession={() => {
        onResetSession();
        navigate("/ask");
      }}
    />
  );
}

/**
 * Wiki mode sidebar content — renders WikiFileTree (Inbox/Raw/Wiki/Schema).
 */
function WikiSidebarContent() {
  return <WikiFileTree embedded />;
}

interface RouteItemProps {
  route: ClawWikiRoute;
  active: boolean;
  badge?: string;
}

function RouteItem({ route, active, badge }: RouteItemProps) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        asChild
        isActive={active}
        tooltip={route.label}
      >
        <Link to={route.path} aria-current={active ? "page" : undefined}>
          <span className="text-base leading-none" aria-hidden="true">
            {route.icon}
          </span>
          <span>{route.label}</span>
        </Link>
      </SidebarMenuButton>
      {badge ? <SidebarMenuBadge>{badge}</SidebarMenuBadge> : null}
    </SidebarMenuItem>
  );
}

/**
 * Chat/Wiki mode toggle — per ia-layout.md §2.
 * Two buttons: [Chat] [Wiki]. Active mode has accent background +
 * primary text. Hidden in icon-mode via Tailwind's
 * group-data-[collapsible=icon] selector (no JS branch needed).
 */
function ModeToggle() {
  const appMode = useSettingsStore((s) => s.appMode);
  const setAppMode = useSettingsStore((s) => s.setAppMode);
  const navigate = useNavigate();
  const location = useLocation();

  // v2 bugfix: keep appMode in sync with the current route.
  // When the user clicks a Sidebar nav item (Ask/Wiki/etc.) the URL
  // changes without going through ModeToggle. This effect mirrors the
  // URL back into appMode so the toggle highlight and ChatSidePanel
  // visibility stay consistent with what's displayed.
  useEffect(() => {
    const path = location.pathname;
    if (path.startsWith("/ask") || path.startsWith("/chat")) {
      if (appMode !== "chat") setAppMode("chat");
    } else if (
      path.startsWith("/wiki") ||
      path.startsWith("/graph") ||
      path.startsWith("/schema") ||
      path.startsWith("/inbox") ||
      path.startsWith("/raw")
    ) {
      if (appMode !== "wiki") setAppMode("wiki");
    }
    // Other routes (dashboard, wechat, settings) preserve
    // whichever mode the user last chose.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.pathname]);

  const switchMode = (mode: "chat" | "wiki") => {
    setAppMode(mode);
    // Sync the route so main content re-renders for the new mode.
    if (mode === "chat" && !location.pathname.startsWith("/ask")) {
      navigate("/ask");
    } else if (mode === "wiki" && !location.pathname.startsWith("/wiki")) {
      navigate("/wiki");
    }
  };

  return (
    <div className="flex h-9 flex-shrink-0 items-center gap-1 border-b border-sidebar-border px-2 group-data-[collapsible=icon]:hidden">
      <button
        type="button"
        onClick={() => switchMode("chat")}
        className={`flex-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors ${
          appMode === "chat"
            ? "bg-sidebar-accent text-primary font-semibold"
            : "text-muted-foreground hover:bg-sidebar-accent/50"
        }`}
      >
        Chat
      </button>
      <button
        type="button"
        onClick={() => switchMode("wiki")}
        className={`flex-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors ${
          appMode === "wiki"
            ? "bg-sidebar-accent text-primary font-semibold"
            : "text-muted-foreground hover:bg-sidebar-accent/50"
        }`}
      >
        Wiki
      </button>
    </div>
  );
}

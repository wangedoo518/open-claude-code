import { useEffect } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { useSettingsStore } from "@/state/settings-store";
import {
  CLAWWIKI_ROUTES,
  type ClawWikiRoute,
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
 * ClawWiki canonical Sidebar — Chat / Wiki dual-tab layout.
 *
 * Structure:
 *
 *   Header  → logo + title + [Chat | Wiki] mode toggle
 *   Content → SessionSidebar (Chat) or WikiFileTree (Wiki)
 *   Footer  → WeChat status badge + Settings link
 *
 * The `appMode` zustand slice drives which content pane shows below
 * the toggle. An effect keeps it in sync with the current route so
 * browser back/forward + deep links don't desync the highlighted tab.
 *
 * Design notes:
 * - Expanded width (256px) / icon-mode (48px) come from the shared
 *   sidebar.tsx CSS variables — don't hard-code them here.
 * - Auto-collapse below 760px is handled inside SidebarProvider.
 * - Active state for the Settings footer link reads useLocation
 *   directly (not zustand) so history + deep links work.
 */

/** Match the exact path or any subpath (e.g. /ask/:sessionId → /ask). */
function isActive(currentPath: string, itemPath: string): boolean {
  if (currentPath === itemPath) return true;
  return currentPath.startsWith(`${itemPath}/`);
}

export function AppSidebar() {
  const location = useLocation();
  const appMode = useSettingsStore((s) => s.appMode);
  const settingsRoute = CLAWWIKI_ROUTES.find((r) => r.key === "settings");

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

        {/* Chat / Wiki mode toggle */}
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
// `Sidebar` — keep the alias so other call sites don't break.
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
 * Wiki mode sidebar content — renders WikiFileTree (Inbox / Raw / Wiki
 * / Schema tree with an opt-in advanced section for power users).
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
  const Icon = route.icon;
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={active} tooltip={route.label}>
        <Link to={route.path} aria-current={active ? "page" : undefined}>
          <Icon
            aria-hidden="true"
            className="size-4 shrink-0"
            strokeWidth={1.5}
          />
          <span>{route.label}</span>
        </Link>
      </SidebarMenuButton>
      {badge ? <SidebarMenuBadge>{badge}</SidebarMenuBadge> : null}
    </SidebarMenuItem>
  );
}

/**
 * Chat / Wiki mode toggle. Two buttons flush with the sidebar header;
 * the active mode gets an accent background + primary-color text.
 * Hidden in icon-mode via Tailwind's `group-data-[collapsible=icon]`
 * selector (no JS branch needed).
 */
function ModeToggle() {
  const appMode = useSettingsStore((s) => s.appMode);
  const setAppMode = useSettingsStore((s) => s.setAppMode);
  const navigate = useNavigate();
  const location = useLocation();

  // Mirror the current route back into appMode so the highlight
  // stays correct when users navigate via links / back-button.
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.pathname]);

  const switchMode = (mode: "chat" | "wiki") => {
    setAppMode(mode);
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

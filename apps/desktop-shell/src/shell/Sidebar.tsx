import { useEffect } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { useSettingsStore } from "@/state/settings-store";
import {
  CLAWWIKI_ROUTES,
  type ClawWikiRoute,
} from "./clawwiki-routes";
import { useAskSessionContext } from "@/features/ask/AskSessionContext";
import { SessionSidebar } from "@/features/ask/SessionSidebar";
import { PanelLeftOpen } from "lucide-react";

/**
 * AppSidebar — DS1.1 compact rail.
 *
 * Replaces the pre-DS1.1 256px shadcn `<Sidebar>` primitive with a
 * narrow 80px rail that follows `ClawWiki Design System/desktop-shell-v2`:
 *
 *   ┌───────┬─────────────────────────┐
 *   │  C    │                         │
 *   │ Home  │                         │
 *   │ Ask   │       (main area)        │
 *   │ Inbox │                         │
 *   │ Wiki  │                         │
 *   │ ⇢     │                         │
 *   │ Wech. │                         │
 *   │  ⚙    │                         │
 *   └───────┴─────────────────────────┘
 *
 * - 80px fixed width, icon + small Chinese label (10.5px). Keeps text
 *   nav per user constraint ("不要切成纯 icon rail"), but the visual
 *   weight drops from 256px to 80px so the main content is no longer
 *   squeezed.
 * - Active state: warm Terracotta pill background (via
 *   `data-active="true"` + `.ds-rail-btn` in globals.css).
 * - No more Chat/Wiki ModeToggle.
 * - No WikiFileTree embedded in the shell — wiki navigation now lives
 *   inside the `/wiki` Knowledge Hub page itself.
 * - SessionSidebar only mounts when the route is /ask, and it becomes
 *   a lightweight second column next to the rail (not a permanent part
 *   of the shell). Other routes see just the rail.
 * - `appMode` is kept in sync with the URL so any remaining consumer
 *   of `useSettingsStore.appMode` (only ChatSidePanel mount gating,
 *   which DS1.1 also retires) stays consistent if it gets read again.
 */

/** Match the exact path or any subpath (e.g. /ask/:sessionId → /ask). */
function isActive(currentPath: string, itemPath: string): boolean {
  if (currentPath === itemPath) return true;
  return currentPath.startsWith(`${itemPath}/`);
}

export function AppSidebar() {
  const location = useLocation();
  const appMode = useSettingsStore((s) => s.appMode);
  const setAppMode = useSettingsStore((s) => s.setAppMode);

  const primaryItems = CLAWWIKI_ROUTES.filter(
    (r) => r.section === "primary" || r.section === "funnel",
  );
  const settingsRoute = CLAWWIKI_ROUTES.find((r) => r.key === "settings");

  // Keep `appMode` auto-synced with the route. Legacy consumers still
  // read this value; DS1.1 doesn't cancel the contract even though the
  // right-side ChatSidePanel no longer mounts.
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

  const onAsk =
    location.pathname.startsWith("/ask") || location.pathname.startsWith("/chat");

  return (
    <>
      <aside className="ds-rail" aria-label="主导航">
        <Link to="/dashboard" className="ds-rail-brand" title="ClawWiki · 你的外脑">
          C
        </Link>
        <div className="ds-rail-items">
          {primaryItems.map((route) => (
            <RailItem
              key={route.key}
              route={route}
              active={isActive(location.pathname, route.path)}
            />
          ))}
        </div>
        <div className="ds-rail-spacer" />
        {/* Rail footer · just Settings. The pre-DS1.1 `WeChatStatusBadge`
            used shadcn SidebarMenuButton primitives that expected a 256px
            sidebar width; at 80px rail it's visually broken. The WeChat
            status is still reachable one click away via the primary nav
            「微信接入」 and via the Dashboard 快速开始 card. */}
        <div className="ds-rail-footer">
          {settingsRoute && (
            <RailItem
              route={settingsRoute}
              active={isActive(location.pathname, settingsRoute.path)}
            />
          )}
        </div>
      </aside>

      {/* Secondary column — session list on /ask only. Keeps Ask users
          able to switch conversations without re-introducing a 256px
          sidebar on every other route. */}
      {onAsk && <AskSecondaryColumn />}
    </>
  );
}

// Back-compat default export name. ClawWikiShell still imports
// `Sidebar` — keep the alias so other call sites don't break.
export { AppSidebar as Sidebar };

function RailItem({ route, active }: { route: ClawWikiRoute; active: boolean }) {
  const Icon = route.icon;
  return (
    <Link
      to={route.path}
      className="ds-rail-btn"
      data-active={active || undefined}
      title={route.label}
      aria-current={active ? "page" : undefined}
    >
      <Icon aria-hidden="true" className="size-5" strokeWidth={1.5} />
      <span className="ds-rail-label">{route.label}</span>
      {route.badge && route.badge !== "—" && (
        <span className="ds-rail-badge-dot" aria-hidden="true" />
      )}
    </Link>
  );
}

function AskSecondaryColumn() {
  const { sessionId, onSwitchSession, onResetSession } =
    useAskSessionContext();
  const navigate = useNavigate();
  const showSessionSidebar = useSettingsStore((s) => s.showSessionSidebar);
  const setShowSessionSidebar = useSettingsStore((s) => s.setShowSessionSidebar);

  return (
    <div
      className={`ds-rail-secondary ${
        showSessionSidebar
          ? "ds-rail-secondary--expanded"
          : "ds-rail-secondary--collapsed"
      }`}
      data-collapsed={!showSessionSidebar || undefined}
    >
      <div
        className="ds-rail-secondary-content"
        aria-hidden={!showSessionSidebar}
        inert={!showSessionSidebar}
      >
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
          onToggleCollapse={() => setShowSessionSidebar(false)}
        />
      </div>
      <div
        className="ds-rail-secondary-restore"
        aria-hidden={showSessionSidebar}
      >
        <button
          type="button"
          onClick={() => setShowSessionSidebar(true)}
          className="ds-rail-secondary-toggle"
          title="展开对话历史"
          aria-label="展开对话历史"
          tabIndex={showSessionSidebar ? -1 : 0}
        >
          <PanelLeftOpen className="size-4" />
        </button>
      </div>
    </div>
  );
}

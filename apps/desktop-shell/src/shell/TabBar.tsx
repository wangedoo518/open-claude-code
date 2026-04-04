import { useEffect } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import {
  Plus,
  Sun,
  Moon,
  Monitor,
  House,
  LayoutGrid,
  Settings,
} from "lucide-react";
import { useAppDispatch, useAppSelector } from "@/store";
import {
  addTab,
  setActiveTab,
  removeTab,
  sanitizePersistedTabs,
  updateTabTitle,
  type Tab,
} from "@/store/slices/tabs";
import {
  removeOpenedApp,
  setCurrentAppId,
} from "@/store/slices/minapps";
import { setViewMode } from "@/store/slices/ui";
import { useTheme } from "@/components/ThemeProvider";
import {
  getAllMinApps,
  getMinAppIdFromPath,
  getMinAppTabId,
  resolveMinApp,
} from "@/config/minapps";
import { clearWebviewState } from "@/utils/webviewStateManager";
import { TabItem } from "./TabItem";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

/**
 * Dual-row top bar — Claude Code desktop style:
 *
 * Row 1 (36px): [traffic lights] [Nav: 首页 | 应用 | 设置] [spacer] [Theme toggle]
 * Row 2 (32px): [traffic lights spacer] [Session tabs...] [+ New]  — conditional
 *
 * Row 2 auto-hides when no session/minapp tabs are open.
 */
export function TabBar() {
  const dispatch = useAppDispatch();
  const navigate = useNavigate();
  const location = useLocation();
  const { tabs, activeTabId } = useAppSelector((s) => s.tabs);
  const viewMode = useAppSelector((s) => s.ui.viewMode);
  const openedKeepAliveApps = useAppSelector(
    (s) => s.minapps.openedKeepAliveApps
  );
  const { theme, setThemeMode } = useTheme();

  const pathname = location.pathname;

  useEffect(() => {
    dispatch(sanitizePersistedTabs());
  }, [dispatch]);

  // ─── Derived nav state ────────────────────────────────────────
  const isOnHome = pathname === "/home" || pathname === "/";
  const isOnApps = pathname.startsWith("/apps");
  const isOnCode = pathname === "/code";
  const isSettingsActive =
    isOnHome && viewMode.kind === "nav" && viewMode.section === "settings";
  const isHomeActive = isOnHome && !isSettingsActive;
  const isAppsActive = isOnApps;

  // Session/minapp tabs only (no system tabs)
  const sessionTabs = tabs.filter(
    (tab) => tab.type !== "home" && tab.type !== "apps"
  );

  // ─── MinApp title resolver ────────────────────────────────────
  const resolveMinAppTitle = (appId: string) => {
    const app = resolveMinApp(appId, openedKeepAliveApps, getAllMinApps());
    if (app?.name) return app.name;
    if (appId.startsWith("openclaw")) return "OpenClaw";
    return appId;
  };

  // ─── Route → tab sync (`/code`) ──────────────────────────────
  useEffect(() => {
    if (!isOnCode) return;

    const existingTab = tabs.find((tab) => tab.id === "route:code");
    if (!existingTab) {
      dispatch(
        addTab({
          id: "route:code",
          type: "code",
          path: "/code",
          title: "Code",
          closable: true,
        })
      );
      return;
    }

    if (existingTab.title !== "Code") {
      dispatch(updateTabTitle({ id: "route:code", title: "Code" }));
    }

    dispatch(setActiveTab("route:code"));
  }, [dispatch, isOnCode, tabs]);

  // ─── Route → tab sync (minapps only) ─────────────────────────
  useEffect(() => {
    const minAppId = getMinAppIdFromPath(pathname);
    if (!minAppId) return;

    const tabId = getMinAppTabId(minAppId);
    const title = resolveMinAppTitle(minAppId);
    const existingTab = tabs.find((tab) => tab.id === tabId);

    dispatch(setCurrentAppId(minAppId));

    if (!existingTab) {
      dispatch(
        addTab({
          id: tabId,
          type: "minapp",
          path: pathname,
          title,
          closable: true,
        })
      );
      return;
    }

    if (existingTab.title !== title) {
      dispatch(updateTabTitle({ id: tabId, title }));
    }

    dispatch(setActiveTab(tabId));
  }, [dispatch, pathname, openedKeepAliveApps, tabs]);

  // ─── Row 1 nav handlers ──────────────────────────────────────
  const handleNavHome = () => {
    dispatch(setViewMode({ kind: "nav", section: "overview" }));
    navigate("/home");
  };

  const handleNavApps = () => {
    navigate("/apps");
  };

  const handleNavSettings = () => {
    dispatch(setViewMode({ kind: "nav", section: "settings" }));
    navigate("/home");
  };

  // ─── Row 2 tab handlers ──────────────────────────────────────
  const handleTabSelect = (tab: Tab) => {
    const minAppId = getMinAppIdFromPath(tab.path);
    if (minAppId) {
      dispatch(setCurrentAppId(minAppId));
    }
    dispatch(setActiveTab(tab.id));
    navigate(tab.path);
  };

  const handleNewSession = () => {
    dispatch(setViewMode({ kind: "nav", section: "session" }));
    navigate("/home");
  };

  const handleTabClose = (tabId: string) => {
    const closeIndex = tabs.findIndex((tab) => tab.id === tabId);
    if (closeIndex === -1) return;

    const closingTab = tabs[closeIndex];
    if (!closingTab.closable) return;

    const remainingTabs = tabs.filter((tab) => tab.id !== tabId);
    const nextTab =
      activeTabId === tabId
        ? remainingTabs[Math.min(closeIndex, remainingTabs.length - 1)] ?? null
        : null;

    const closingAppId = getMinAppIdFromPath(closingTab.path);
    if (closingAppId) {
      dispatch(removeOpenedApp(closingAppId));
      clearWebviewState(closingAppId);
    }

    dispatch(removeTab(tabId));

    if (nextTab) {
      const nextAppId = getMinAppIdFromPath(nextTab.path);
      if (nextAppId) {
        dispatch(setCurrentAppId(nextAppId));
      }
      dispatch(setActiveTab(nextTab.id));
      navigate(nextTab.path);
    } else {
      // No tabs left, go home
      dispatch(setViewMode({ kind: "nav", section: "overview" }));
      navigate("/home");
    }
  };

  // ─── Theme cycling ────────────────────────────────────────────
  const cycleTheme = () => {
    const order = ["light", "dark", "system"] as const;
    const idx = order.indexOf(theme);
    setThemeMode(order[(idx + 1) % order.length]);
  };

  const ThemeIcon =
    theme === "light" ? Sun : theme === "dark" ? Moon : Monitor;

  return (
    <div className="flex flex-col">
      {/* ══ Row 1 — Navigation ══════════════════════════════════ */}
      <div
        className="flex h-9 items-center border-b border-border/50 bg-muted/30"
        data-tauri-drag-region
      >
        {/* macOS traffic light spacing */}
        <div className="w-[78px] shrink-0" data-tauri-drag-region />

        {/* Nav buttons */}
        <nav className="flex items-center gap-0.5 px-1">
          <NavButton
            icon={House}
            label="首页"
            active={isHomeActive}
            onClick={handleNavHome}
          />
          <NavButton
            icon={LayoutGrid}
            label="应用"
            active={isAppsActive}
            onClick={handleNavApps}
          />
          <NavButton
            icon={Settings}
            label="设置"
            active={isSettingsActive}
            onClick={handleNavSettings}
          />
        </nav>

        {/* Draggable spacer */}
        <div className="flex-1" data-tauri-drag-region />

        {/* Right controls */}
        <div className="flex shrink-0 items-center gap-1 pr-3">
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                className="flex size-7 items-center justify-center rounded-md cursor-pointer text-foreground/70 transition-colors hover:bg-accent hover:text-foreground"
                onClick={cycleTheme}
              >
                <ThemeIcon className="size-3.5" />
              </button>
            </TooltipTrigger>
            <TooltipContent>
              Theme: {theme.charAt(0).toUpperCase() + theme.slice(1)}
            </TooltipContent>
          </Tooltip>
        </div>
      </div>

      {/* ══ Row 2 — Session Tabs (conditional) ══════════════════ */}
      {sessionTabs.length > 0 && (
        <div
          className="flex h-8 items-center border-b border-border/30 bg-muted/20 shadow-sm"
        >
          {/* Traffic light alignment spacer */}
          <div className="w-[78px] shrink-0" />

          {/* Tab list — scrollable */}
          <div className="flex flex-1 items-center gap-0.5 overflow-x-auto px-1 scrollbar-none">
            {sessionTabs.map((tab) => (
              <TabItem
                key={tab.id}
                id={tab.id}
                title={tab.title}
                type={tab.type}
                active={tab.id === activeTabId}
                closable={tab.closable}
                onSelect={() => handleTabSelect(tab)}
                onClose={() => handleTabClose(tab.id)}
                onMiddleClick={() => {
                  if (tab.closable) handleTabClose(tab.id);
                }}
              />
            ))}

            {/* Add session button */}
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  className="flex size-7 shrink-0 items-center justify-center rounded-md cursor-pointer text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  onClick={handleNewSession}
                >
                  <Plus className="size-3.5" />
                </button>
              </TooltipTrigger>
              <TooltipContent>New Session</TooltipContent>
            </Tooltip>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Row 1 navigation button — fixed items (not Redux tabs).
 */
function NavButton({
  icon: Icon,
  label,
  active,
  onClick,
}: {
  icon: typeof House;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "flex h-[26px] items-center gap-1.5 rounded-md px-2 text-[12px] cursor-pointer transition-colors select-none",
        active
          ? "bg-background text-foreground shadow-sm font-medium"
          : "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
      )}
      onClick={onClick}
    >
      <Icon className="size-3.5" />
      <span>{label}</span>
    </button>
  );
}

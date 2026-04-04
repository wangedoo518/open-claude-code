import { useEffect } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { Plus, Sun, Moon, Monitor, Settings } from "lucide-react";
import { useAppDispatch, useAppSelector } from "@/store";
import {
  addTab,
  ensureSystemTabs,
  setActiveTab,
  removeTab,
  updateTabTitle,
  type Tab,
} from "@/store/slices/tabs";
import {
  removeOpenedApp,
  setCurrentAppId,
} from "@/store/slices/minapps";
import { setActiveHomeSessionId, setHomeSection } from "@/store/slices/ui";
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

/**
 * Top tab bar matching cherry-studio's TabContainer layout:
 *
 * [macOS traffic lights] [Tabs...] [+ Add] [Theme] [Settings]
 *
 * - Height: matches --navbar-height
 * - Tabs: 30px height, 90px min-width
 * - Right controls: 30×30px icon buttons
 * - macOS: draggable title bar region
 */
export function TabBar() {
  const dispatch = useAppDispatch();
  const navigate = useNavigate();
  const location = useLocation();
  const { tabs, activeTabId } = useAppSelector((s) => s.tabs);
  const openedKeepAliveApps = useAppSelector(
    (s) => s.minapps.openedKeepAliveApps
  );
  const { theme, setThemeMode } = useTheme();

  const resolveMinAppTitle = (appId: string) => {
    const app = resolveMinApp(appId, openedKeepAliveApps, getAllMinApps());
    if (app?.name) {
      return app.name;
    }
    if (appId.startsWith("openclaw")) {
      return "OpenClaw";
    }
    return appId;
  };

  useEffect(() => {
    dispatch(ensureSystemTabs());
  }, [dispatch]);

  // Sync active tab with current route
  useEffect(() => {
    const pathname = location.pathname;
    if (pathname === "/home" || pathname === "/") {
      dispatch(setActiveTab("home"));
      return;
    }

    if (pathname === "/apps") {
      dispatch(setActiveTab("apps"));
      return;
    }

    const minAppId = getMinAppIdFromPath(pathname);
    if (!minAppId) {
      return;
    }

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
  }, [dispatch, location.pathname, openedKeepAliveApps, tabs]);

  const handleTabSelect = (tab: Tab) => {
    const minAppId = getMinAppIdFromPath(tab.path);
    if (minAppId) {
      dispatch(setCurrentAppId(minAppId));
    }
    dispatch(setActiveTab(tab.id));
    navigate(tab.path);
  };

  const handleNewTab = () => {
    dispatch(setActiveTab("home"));
    dispatch(setActiveHomeSessionId(null));
    dispatch(setHomeSection("session"));
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

    if (!nextTab) {
      return;
    }

    const nextAppId = getMinAppIdFromPath(nextTab.path);
    if (nextAppId) {
      dispatch(setCurrentAppId(nextAppId));
    }

    dispatch(setActiveTab(nextTab.id));
    navigate(nextTab.path);
  };

  const handleOpenSettings = () => {
    dispatch(setActiveTab("home"));
    dispatch(setHomeSection("settings"));
    navigate("/home");
  };

  const cycleTheme = () => {
    const order = ["light", "dark", "system"] as const;
    const idx = order.indexOf(theme);
    setThemeMode(order[(idx + 1) % order.length]);
  };

  const ThemeIcon =
    theme === "light" ? Sun : theme === "dark" ? Moon : Monitor;

  return (
    <div
      className="flex h-10 items-center border-b border-border bg-muted/30"
      data-tauri-drag-region
    >
      {/* macOS traffic light spacing */}
      <div className="w-[78px] shrink-0" data-tauri-drag-region />

      {/* Tab list — scrollable container */}
      <div
        className="flex flex-1 items-center gap-0.5 overflow-x-auto px-1 scrollbar-none"
        data-tauri-drag-region
      >
        {tabs.map((tab) => (
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

        {/* Add tab button — 30×30px matching cherry-studio */}
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className="flex size-[30px] shrink-0 items-center justify-center rounded-md cursor-pointer text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              onClick={handleNewTab}
            >
              <Plus className="size-3.5" />
            </button>
          </TooltipTrigger>
          <TooltipContent>New Session</TooltipContent>
        </Tooltip>
      </div>

      {/* Right controls — matching cherry-studio RightButtonsContainer */}
      <div className="flex shrink-0 items-center gap-1.5 pr-3">
        {/* Theme toggle — 30×30px */}
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className="flex size-[30px] items-center justify-center rounded-lg cursor-pointer text-foreground transition-colors hover:bg-accent"
              onClick={cycleTheme}
            >
              <ThemeIcon className="size-4" />
            </button>
          </TooltipTrigger>
          <TooltipContent>
            Theme: {theme.charAt(0).toUpperCase() + theme.slice(1)}
          </TooltipContent>
        </Tooltip>

        {/* Settings button — 30×30px */}
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className="flex size-[30px] items-center justify-center rounded-lg cursor-pointer text-foreground transition-colors hover:bg-accent"
              onClick={handleOpenSettings}
            >
              <Settings className="size-4" />
            </button>
          </TooltipTrigger>
          <TooltipContent>Settings</TooltipContent>
        </Tooltip>
      </div>
    </div>
  );
}

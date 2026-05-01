import { Link, Navigate, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { useEffect, type ReactNode } from "react";
import { MessageCircle } from "lucide-react";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { BrowserDrawer } from "@/components/BrowserDrawer";
import { AskSessionProvider } from "@/features/ask/AskSessionContext";
import { CommandPalette } from "@/features/palette/CommandPalette";
import { SettingsModal } from "@/features/settings/SettingsModal";
import { AbsorbEventsBridge } from "@/features/wiki/AbsorbEventsBridge";
import { ChannelStatusModal } from "@/features/wechat-kefu/ChannelStatusModal";
import { ConnectWeChatModal } from "@/features/wechat-kefu/ConnectWeChatModal";
import { AppSidebar } from "./Sidebar";
import { BuddyStatusBar } from "./BuddyStatusBar";
import { ShellInspector } from "./ShellInspector";
import {
  CLAWWIKI_DEFAULT_ROUTE,
  CLAWWIKI_ROUTER_ROUTES,
} from "./clawwiki-routes";

function PageTransition({ children }: { children: ReactNode }) {
  const location = useLocation();
  return (
    <div key={location.pathname} className="ds-page-transition flex h-full flex-col">
      {children}
    </div>
  );
}

export function ClawWikiShell() {
  const location = useLocation();
  const navigate = useNavigate();

  // Slice 46 — global keyboard-first shortcuts: Mod+[ and Mod+] map
  // to browser-style back/forward, so power users can pivot without
  // reaching for the mouse. Capture phase + IME guard mirror the
  // existing CommandPalette listener. Skip when focus is in an
  // editable surface so users can still type "[" and "]" inside an
  // article body or CodeMirror buffer.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.isComposing || e.keyCode === 229) return;
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key !== "[" && e.key !== "]") return;
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      if (
        tag === "input" ||
        tag === "textarea" ||
        target?.isContentEditable
      ) {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "[") navigate(-1);
      else navigate(1);
    };
    document.addEventListener("keydown", handler, /* capture */ true);
    return () =>
      document.removeEventListener("keydown", handler, /* capture */ true);
  }, [navigate]);

  const isAskRoute =
    location.pathname.startsWith("/ask") || location.pathname.startsWith("/chat");
  const isDashboardRoute =
    location.pathname === "/dashboard" || location.pathname === "/";
  const isWikiGraphView =
    location.pathname === "/wiki" &&
    new URLSearchParams(location.search).get("view") === "graph";
  const isGraphRoute = location.pathname === "/graph";
  const isGraphSurface = isGraphRoute || isWikiGraphView;
  const isInboxRoute = location.pathname === "/inbox";
  const isWechatRoute = location.pathname === "/wechat";
  const isSettingsRoute = location.pathname.startsWith("/settings");
  // Slice 45 — hide ShellInspector on routes that already render their
  // own right column (Knowledge article view, Inbox) or that need the
  // full canvas (graph immersive layout). Saves the user from a 320px
  // shell aside fighting another 250-360px page-level aside on
  // narrower screens.
  const isWikiArticleRoute =
    location.pathname.startsWith("/wiki/") && location.pathname.length > "/wiki/".length;
  const showShellInspector =
    !isGraphRoute && !isWikiArticleRoute && !isInboxRoute;

  return (
    <ErrorBoundary>
      <AskSessionProvider>
        <AbsorbEventsBridge />
        <div
          className={`ds-canvas flex h-screen overflow-hidden ${
            isGraphRoute ? "ds-graph-immersive-shell" : ""
          }`}
        >
          <AppSidebar />
          <div className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
            <main className="relative flex min-h-0 flex-1 flex-col">
              <ErrorBoundary>
                <div className="min-h-0 flex-1 overflow-y-auto">
                  <Routes>
                    {CLAWWIKI_ROUTER_ROUTES.map((route) => (
                      <Route
                        key={route.key}
                        path={route.routePath ?? route.path}
                        element={
                          <PageTransition>{route.render()}</PageTransition>
                        }
                      />
                    ))}
                    <Route
                      path="*"
                      element={<Navigate to={CLAWWIKI_DEFAULT_ROUTE} replace />}
                    />
                  </Routes>
                </div>
              </ErrorBoundary>
            </main>
            <BuddyStatusBar />

            {!isAskRoute &&
              !isDashboardRoute &&
              !isGraphSurface &&
              !isInboxRoute &&
              !isWechatRoute &&
              !isSettingsRoute && <FloatingAskCTA />}
          </div>
          {showShellInspector && <ShellInspector />}
          <BrowserDrawer />
        </div>
      </AskSessionProvider>

      <SettingsModal />
      <ConnectWeChatModal />
      <ChannelStatusModal />
      <CommandPalette />
    </ErrorBoundary>
  );
}

function FloatingAskCTA() {
  return (
    <Link
      to="/ask"
      className="ds-ask-fab"
      aria-label="打开 Ask 问问题"
      title="问问你的外脑"
    >
      <MessageCircle className="size-4" strokeWidth={1.5} />
      <span>问 AI</span>
    </Link>
  );
}

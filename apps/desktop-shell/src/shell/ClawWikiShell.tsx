import { Link, Navigate, Route, Routes, useLocation } from "react-router-dom";
import type { ReactNode } from "react";
import { MessageCircle } from "lucide-react";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { AskSessionProvider } from "@/features/ask/AskSessionContext";
import { AppSidebar } from "./Sidebar";
import { CLAWWIKI_DEFAULT_ROUTE } from "./clawwiki-routes";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { AskPage } from "@/features/ask/AskPage";
import { InboxPage } from "@/features/inbox/InboxPage";
import { RawLibraryPage } from "@/features/raw/RawLibraryPage";
import { KnowledgeHubPage } from "@/features/wiki/KnowledgeHubPage";
import { GraphPage } from "@/features/graph/GraphPage";
import { SchemaEditorPage } from "@/features/schema/SchemaEditorPage";
import { WeChatBridgePage } from "@/features/wechat/WeChatBridgePage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { SettingsModal } from "@/features/settings/SettingsModal";
import { ConnectWeChatModal } from "@/features/wechat-kefu/ConnectWeChatModal";
import { ConnectWeChatPipelinePage } from "@/features/wechat-kefu/ConnectWeChatPipelinePage";
import { ChannelStatusModal } from "@/features/wechat-kefu/ChannelStatusModal";
import { CommandPalette } from "@/features/palette/CommandPalette";
import { BrowserDrawer } from "@/components/BrowserDrawer";

/**
 * ClawWikiShell — DS1.1 visual shell.
 *
 * Layout (post-DS1.1):
 *
 *   ┌───────────┬─────────────────────────────────────────┐
 *   │ Rail 80px │              main content                │
 *   │           │  (full-width on Dashboard/Wiki/WeChat/   │
 *   │           │   Inbox/Settings — no permanent Ask      │
 *   │           │   side panel anymore)                    │
 *   └───────────┴─────────────────────────────────────────┘
 *
 *   When the current route is /ask, AppSidebar itself renders an
 *   additional narrow column (SessionSidebar · 224px) next to the rail.
 *
 * Why we replaced the shadcn `<SidebarProvider>` + `<Sidebar>` + `<SidebarInset>`:
 *
 * - Pre-DS1.1 the shell used `collapsible="icon"` on a 256px shadcn
 *   Sidebar and mounted a 320px ChatSidePanel on Wiki-mode routes. Net
 *   visible chrome was 576px — main content was squeezed and the product
 *   read as a developer console.
 * - DS1.1 retires the ChatSidePanel from all non-/ask routes (per the
 *   design-system "one place for conversations" principle) and replaces
 *   the shadcn sidebar with a 80px compact rail. Main content expands
 *   by ~400px which is the visible change the user's截图 was missing.
 * - A small floating "问 AI" pill sits in the bottom-right of non-/ask
 *   routes so users who want to start a chat from Dashboard/Wiki/WeChat
 *   can still reach it in one click — without the panel squatting on
 *   the page the whole time.
 *
 * What we kept:
 *
 * - All existing routes and their pages. No Route was deleted.
 * - `AskSessionProvider` still wraps the tree so useAskSession context
 *   stays shared across the Ask page and the floating CTA.
 * - `SettingsModal`, `ConnectWeChatModal`, `ChannelStatusModal`,
 *   `CommandPalette` still mount as global overlays.
 * - `BrowserDrawer` still mounts — it's a user-opened drawer, not a
 *   permanent panel, so it doesn't violate DS1.1's rail-only premise.
 */
function PageTransition({ children }: { children: ReactNode }) {
  const location = useLocation();
  return (
    <div key={location.pathname} className="flex h-full flex-col animate-fade-in">
      {children}
    </div>
  );
}

export function ClawWikiShell() {
  const location = useLocation();
  const isAskRoute =
    location.pathname.startsWith("/ask") || location.pathname.startsWith("/chat");

  return (
    <ErrorBoundary>
      <AskSessionProvider>
        <div className="ds-canvas flex h-screen overflow-hidden">
          <AppSidebar />
          <div className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
            <main className="relative flex min-h-0 flex-1 flex-col">
              <ErrorBoundary>
                <div className="min-h-0 flex-1 overflow-y-auto">
                  <Routes>
                    <Route
                      path="/dashboard"
                      element={
                        <PageTransition>
                          <DashboardPage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/ask/*"
                      element={
                        <PageTransition>
                          <AskPage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/inbox"
                      element={
                        <PageTransition>
                          <InboxPage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/raw/*"
                      element={
                        <PageTransition>
                          <RawLibraryPage />
                        </PageTransition>
                      }
                    />
                    {/* DS1-B: /wiki/* renders the Knowledge Hub (pill-tabs
                        for 页面 / 关系图 / 素材库). Direct-access /raw
                        and /graph routes remain below for backwards
                        compat. */}
                    <Route
                      path="/wiki/*"
                      element={
                        <PageTransition>
                          <KnowledgeHubPage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/graph"
                      element={
                        <PageTransition>
                          <GraphPage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/schema/*"
                      element={
                        <PageTransition>
                          <SchemaEditorPage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/wechat"
                      element={
                        <PageTransition>
                          <WeChatBridgePage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/settings"
                      element={
                        <PageTransition>
                          <SettingsPage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="/connect-wechat"
                      element={
                        <PageTransition>
                          <ConnectWeChatPipelinePage />
                        </PageTransition>
                      }
                    />
                    <Route
                      path="*"
                      element={<Navigate to={CLAWWIKI_DEFAULT_ROUTE} replace />}
                    />
                  </Routes>
                </div>
              </ErrorBoundary>
            </main>

            {/* DS1.1 · floating "问 AI" CTA. Shown on every non-Ask
                route as a gentle way back into the Ask page. Replaces
                the permanent 320px right-side ChatSidePanel that made
                Dashboard/Wiki/WeChat feel like a developer console. */}
            {!isAskRoute && <FloatingAskCTA />}
          </div>
          <BrowserDrawer />
        </div>
      </AskSessionProvider>
      {/* Global modal / overlay mounts — unchanged */}
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

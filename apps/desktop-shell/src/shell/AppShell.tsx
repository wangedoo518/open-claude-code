import { Routes, Route, Navigate } from "react-router-dom";
import { TabBar } from "./TabBar";
import { HomePage } from "@/features/workbench/HomePage";
import { AppsGalleryPage } from "@/features/apps/AppsGalleryPage";
import { MinAppDetailPage } from "@/features/apps/MinAppDetailPage";
import { CodeToolsPage } from "@/features/code-tools/CodeToolsPage";

/**
 * Root application shell.
 *
 * Uses React Router for content routing while the TabBar provides
 * the cherry-studio-style top tab navigation.
 *
 * Route structure:
 *   /home      -> HomePage (workbench with sessions, search, settings, etc.)
 *   /apps      -> AppsGalleryPage (cherry-studio grid of MinApps)
 *   /code      -> CodeToolsPage (strict Cherry Code clone entry)
 *   /apps/:id  -> MinAppDetailPage (toolbar + keep-alive content pool)
 */
export function AppShell() {
  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden">
      <TabBar />
      <main className="relative flex-1 overflow-hidden">
        <Routes>
          <Route path="/home" element={<HomePage />} />
          <Route path="/apps" element={<AppsGalleryPage />} />
          <Route path="/code" element={<CodeToolsPage />} />
          <Route path="/apps/:appId" element={<MinAppDetailPage />} />
          <Route path="*" element={<Navigate to="/home" replace />} />
        </Routes>
      </main>
    </div>
  );
}

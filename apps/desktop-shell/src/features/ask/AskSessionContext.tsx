/**
 * AskSessionContext — shared session state between Sidebar (SessionSidebar)
 * and content area (AskPage, ChatSidePanel).
 *
 * Wraps `useAskSession` once at the ClawWikiShell level so all consumers
 * share one instance. Eliminates the dual-instantiation race where AskPage
 * and ChatSidePanel each called useAskSession independently and coordinated
 * via localStorage.
 *
 * SSE subscriptions (useAskSSE) remain local to the rendering component —
 * they should only be active when the chat UI is visible, not globally.
 */

import { createContext, useContext, type ReactNode } from "react";
import { useAskSession, type UseAskSessionResult } from "./useAskSession";

const AskSessionCtx = createContext<UseAskSessionResult | null>(null);

export function AskSessionProvider({ children }: { children: ReactNode }) {
  const session = useAskSession();
  return (
    <AskSessionCtx.Provider value={session}>{children}</AskSessionCtx.Provider>
  );
}

/**
 * Read the shared session state. Throws if called outside AskSessionProvider.
 */
export function useAskSessionContext(): UseAskSessionResult {
  const ctx = useContext(AskSessionCtx);
  if (!ctx) {
    throw new Error(
      "useAskSessionContext must be used within <AskSessionProvider>"
    );
  }
  return ctx;
}

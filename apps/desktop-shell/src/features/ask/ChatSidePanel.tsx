/**
 * ChatSidePanel — compact chat panel on the right side of Wiki mode.
 * Per component-spec.md §4 and 03-chat-tab.md §1 (compact variant).
 *
 * v2 bugfix (session visualization): the side panel now wraps AskWorkbench
 * with `compact` + `hideHeader` flags so it inherits all state UI:
 *   - streaming cursor + shimmer
 *   - token counts
 *   - tool call blocks
 *   - permission request dialog
 *   - error banner
 *   - provider switching
 *
 * Shares the same session as the main Chat Tab via useAskSession's
 * localStorage persistence (ACTIVE_SESSION_STORAGE_KEY).
 */

import { useCallback, useMemo } from "react";
import { ChevronLeft, ChevronRight, Loader2 } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useSettingsStore } from "@/state/settings-store";
import { useStreamingStore } from "@/state/streaming-store";
import { useAskSession } from "./useAskSession";
import { useAskSSE } from "./useAskSSE";
import { AskWorkbench } from "./AskWorkbench";
import { listProviders, activateProvider } from "@/features/settings/api/client";

export function ChatSidePanel() {
  const collapsed = useSettingsStore((s) => s.chatPanelCollapsed);
  const setCollapsed = useSettingsStore((s) => s.setChatPanelCollapsed);

  return (
    <div className="relative flex h-full">
      {/* Collapse toggle — always visible, positioned OUTSIDE the panel. */}
      <button
        onClick={() => setCollapsed(!collapsed)}
        aria-label={collapsed ? "展开 Chat 面板" : "折叠 Chat 面板"}
        className="absolute top-1/2 z-20 flex size-6 -translate-y-1/2 items-center justify-center rounded-l-md border border-r-0 border-[var(--color-border)] bg-[var(--color-background)] shadow-sm hover:bg-[var(--color-accent)] transition-colors"
        style={{
          right: collapsed ? 0 : 320,
          boxShadow: "var(--deeptutor-shadow-sm, 0 1px 3px rgba(45,43,40,0.04))",
          transition: "right 200ms linear, background-color 150ms",
        }}
      >
        {collapsed ? <ChevronLeft className="size-3.5" /> : <ChevronRight className="size-3.5" />}
      </button>

      {/* Actual panel — collapsed width = 0, kept in DOM so session
          hooks don't unmount/remount. */}
      <aside
        className="flex flex-col border-l border-[var(--color-border)] bg-[var(--color-background)] overflow-hidden"
        style={{
          width: collapsed ? 0 : 320,
          minWidth: collapsed ? 0 : 320,
          transition: "width 200ms linear, min-width 200ms linear",
        }}
      >
        {!collapsed && <ChatSidePanelBody />}
      </aside>
    </div>
  );
}

/**
 * Inner body — only mounts when expanded. This keeps useAskSession
 * from firing ensure-mutations while the panel is collapsed.
 */
function ChatSidePanelBody() {
  // Same wiring as AskPage: session + SSE + providers.
  const {
    sessionId,
    session,
    isLoadingSession,
    isSending,
    isTurnActive,
    errorMessage,
    onSend,
    onResetSession,
  } = useAskSession();

  useAskSSE(sessionId, isTurnActive);

  // Needed to decide whether the turn is visibly streaming text. OpenAiCompat
  // providers (Kimi, DeepSeek, ...) don't broadcast `text_delta` SSE events,
  // so streamingContent stays empty for the entire thinking phase — without
  // an explicit indicator the side panel looks frozen to the user.
  const streamingContent = useStreamingStore((s) => s.streamingContent);
  const showLoadingFallback = isTurnActive && !streamingContent;

  const providersQuery = useQuery({
    queryKey: ["desktop", "providers"],
    queryFn: () => listProviders(),
    staleTime: 30_000,
  });
  const queryClient = useQueryClient();

  const activeProvider = providersQuery.data
    ? providersQuery.data.providers.find((p) => p.id === providersQuery.data.active)
    : null;
  const realModelLabel = useMemo(() => {
    if (activeProvider) {
      return activeProvider.display_name || activeProvider.model || activeProvider.id;
    }
    return session?.model_label;
  }, [activeProvider, session?.model_label]);

  const providerOptions = useMemo(
    () =>
      (providersQuery.data?.providers ?? []).map((p) => ({
        id: p.id,
        label: p.display_name || p.id,
        model: p.model,
        isActive: p.id === providersQuery.data?.active,
      })),
    [providersQuery.data],
  );

  const handleSwitchProvider = useCallback(
    async (id: string) => {
      try {
        await activateProvider(id);
        void queryClient.invalidateQueries({ queryKey: ["desktop", "providers"] });
      } catch (err) {
        console.error("[chat-side-panel] switch provider failed:", err);
      }
    },
    [queryClient],
  );

  return (
    <div className="flex h-full flex-col">
      {/* Tiny header strip with model badge + streaming indicator.
          Kept because AskHeader is hidden via hideHeader prop — users
          still need to see which model is answering. */}
      <div className="sticky top-0 z-10 flex h-9 shrink-0 items-center justify-between border-b border-border bg-card px-3">
        <span className="text-[12px] font-semibold text-foreground">Ask</span>
        <span className="truncate text-right text-xs text-muted-foreground">
          {realModelLabel ?? "..."}
          {isTurnActive && <span className="ml-1.5 text-primary">思考中...</span>}
        </span>
      </div>

      {/* Loading fallback — visible while the backend is working but no
          text_delta has arrived yet. Covers providers without streaming
          (Kimi/DeepSeek via OpenAiCompat). StreamingMessage inside
          AskWorkbench will take over once content begins to flow. */}
      {showLoadingFallback && (
        <div
          role="status"
          aria-live="polite"
          className="flex items-center gap-2 border-b border-[var(--color-border)] bg-[var(--color-muted)]/40 px-3 py-2"
        >
          <Loader2 className="size-3 animate-spin text-[var(--color-primary)]" />
          <span className="text-[11px] text-[var(--color-muted-foreground)]">
            思考中...
          </span>
        </div>
      )}

      {/* AskWorkbench in compact mode — inherits ALL state visualization. */}
      <div className="min-h-0 flex-1">
        <AskWorkbench
          session={session}
          isLoadingSession={isLoadingSession}
          isSending={isSending}
          errorMessage={errorMessage}
          onSend={onSend}
          onCreateSession={onResetSession}
          modelLabel={realModelLabel}
          environmentLabel={session?.environment_label}
          providers={providerOptions}
          onSwitchProvider={handleSwitchProvider}
          compact={true}
          hideHeader={true}
        />
      </div>
    </div>
  );
}

/**
 * Ask · CCD 工作台 + 流式会话 + 会话侧边栏
 */

import { useState, useCallback } from "react";
import { Loader2, AlertTriangle, RefreshCw } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AskWorkbench } from "./AskWorkbench";
import { SessionSidebar } from "./SessionSidebar";
import { useAskSession } from "./useAskSession";
import { useAskSSE } from "./useAskSSE";
import { listProviders, activateProvider } from "@/features/settings/api/client";

export function AskPage() {
  const {
    sessionId,
    session,
    isLoadingSession,
    isSending,
    isTurnActive,
    errorMessage,
    onSend,
    onResetSession,
    onSwitchSession,
  } = useAskSession();

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  // Wire SSE subscription for real-time streaming + permission requests
  useAskSSE(sessionId, isTurnActive);

  // Read the real active provider model name from providers API
  const providersQuery = useQuery({
    queryKey: ["desktop", "providers"],
    queryFn: () => listProviders(),
    staleTime: 30_000,
  });

  const queryClient = useQueryClient();

  // Derive real model label from active provider
  const activeProvider = providersQuery.data
    ? providersQuery.data.providers.find((p) => p.id === providersQuery.data.active)
    : null;
  const realModelLabel = activeProvider
    ? (activeProvider.display_name || activeProvider.model || activeProvider.id)
    : session?.model_label;

  // Build provider options for model selector
  const providerOptions = (providersQuery.data?.providers ?? []).map((p) => ({
    id: p.id,
    label: p.display_name || p.id,
    model: p.model,
    isActive: p.id === providersQuery.data?.active,
  }));

  const handleSwitchProvider = useCallback(async (id: string) => {
    try {
      await activateProvider(id);
      void queryClient.invalidateQueries({ queryKey: ["desktop", "providers"] });
    } catch (err) {
      console.error("[ask] failed to switch provider:", err);
    }
  }, [queryClient]);

  // Main content area (workbench or loading/error states)
  let content: React.ReactNode;

  if (isLoadingSession && !session) {
    content = (
      <div className="flex h-full items-center justify-center">
        <div className="flex items-center gap-2 text-caption text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          <span>正在准备对话…</span>
        </div>
      </div>
    );
  } else if (errorMessage && !session) {
    content = (
      <div className="flex h-full items-center justify-center">
        <div
          className="max-w-md rounded-lg border px-6 py-5 text-center"
          style={{
            borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
            backgroundColor: "color-mix(in srgb, var(--color-error) 4%, transparent)",
          }}
        >
          <AlertTriangle className="mx-auto mb-2 size-6" style={{ color: "var(--color-error)" }} />
          <div className="mb-1 text-body font-semibold" style={{ color: "var(--color-error)" }}>
            无法启动对话
          </div>
          <div className="mb-4 text-caption text-muted-foreground">{errorMessage}</div>
          <button
            type="button"
            onClick={onResetSession}
            className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-body-sm font-medium text-primary-foreground hover:bg-primary/90"
          >
            <RefreshCw className="size-3" />
            重试
          </button>
        </div>
      </div>
    );
  } else {
    content = (
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
      />
    );
  }

  return (
    <div className="flex h-full overflow-hidden">
      <SessionSidebar
        activeSessionId={sessionId}
        onSelectSession={onSwitchSession}
        onNewSession={onResetSession}
        collapsed={sidebarCollapsed}
        onToggleCollapse={() => setSidebarCollapsed((v) => !v)}
      />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {content}
      </div>
    </div>
  );
}

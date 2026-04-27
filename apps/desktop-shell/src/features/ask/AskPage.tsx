/**
 * Ask · CCD 工作台 + 流式会话
 *
 * Session sidebar has been lifted to the shell Sidebar (Chat mode).
 * AskPage now reads shared session state from AskSessionContext.
 */

import { useCallback } from "react";
import { Loader2, AlertTriangle, RefreshCw } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AskWorkbench } from "./AskWorkbench";
import { useAskSessionContext } from "./AskSessionContext";
import { useAskSSE } from "./useAskSSE";
import {
  activateProvider,
  getSettings,
  listProviders,
} from "@/api/desktop/settings";

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
    onEnsureAndBind,
  } = useAskSessionContext();

  // Wire SSE subscription for real-time streaming + permission requests
  useAskSSE(sessionId, isTurnActive);

  // A5.2 — resolve canonical project_path so the providers registry
  // query targets the directory that actually contains
  // `.claw/providers.json`. Without this the backend falls back to
  // `std::env::current_dir()` (outer repo root on the gray build) and
  // returns `{active:"",providers:[]}`, leaving `model_label` stuck on
  // the "Opus 4.6" placeholder.
  const settingsQuery = useQuery({
    queryKey: ["desktop", "settings"],
    queryFn: getSettings,
    staleTime: 5 * 60 * 1000,
  });
  const projectPath = settingsQuery.data?.settings?.project_path;

  // Read the real active provider model name from providers API —
  // scoped by projectPath. The query is `enabled` only after settings
  // resolve so we don't waste a round-trip on the known-empty fallback.
  const providersQuery = useQuery({
    queryKey: ["desktop", "providers", projectPath ?? ""],
    queryFn: () => listProviders(projectPath),
    enabled: !!projectPath,
    staleTime: 30_000,
  });

  const queryClient = useQueryClient();

  // Derive real model label from active provider.
  // Fallback ladder (A5.2 truth-aware):
  //   1. resolved activeProvider → its display_name / model / id
  //   2. providers list is empty AND session's label is the known
  //      "Opus 4.6" placeholder → show "模型未解析" (honest: we
  //      couldn't resolve a provider, so don't pretend Opus is active)
  //   3. anything else → session.model_label (may still be a real
  //      backend-reported label for legitimately Anthropic-native setups)
  const activeProvider = providersQuery.data
    ? providersQuery.data.providers.find((p) => p.id === providersQuery.data.active)
    : null;
  const providersResolvedEmpty =
    providersQuery.isSuccess &&
    providersQuery.data.providers.length === 0;
  const realModelLabel = activeProvider
    ? (activeProvider.display_name || activeProvider.model || activeProvider.id)
    : providersResolvedEmpty && session?.model_label === "Opus 4.6"
      ? "模型未解析"
      : session?.model_label;

  // P1-2 fallback signal: the header-level banner should surface any
  // failure to resolve a provider. Either settings errored entirely or
  // the providers list resolved empty and the session's label is still
  // the known "Opus 4.6" placeholder.
  const settingsUnready =
    settingsQuery.isError ||
    (providersResolvedEmpty && session?.model_label === "Opus 4.6");

  // Build provider options for model selector
  const providerOptions = (providersQuery.data?.providers ?? []).map((p) => ({
    id: p.id,
    label: p.display_name || p.id,
    model: p.model,
    kind: p.kind,
    isActive: p.id === providersQuery.data?.active,
  }));

  // P1-2: activateProvider must scope by canonical projectPath. Pre-fix
  // this silently fell back to the backend's current_dir() provider
  // registry — the activation succeeded against the wrong registry and
  // the header pill kept showing stale state. Passing projectPath here
  // closes the loop with the same truth source as listProviders above.
  const handleSwitchProvider = useCallback(async (id: string) => {
    try {
      await activateProvider(id, projectPath);
      void queryClient.invalidateQueries({ queryKey: ["desktop", "providers"] });
    } catch (err) {
      console.error("[ask] failed to switch provider:", err);
    }
  }, [queryClient, projectPath]);

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
        onEnsureAndBind={onEnsureAndBind}
        settingsUnready={settingsUnready}
      />
    );
  }

  return (
    <div className="flex h-full overflow-hidden">
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {content}
      </div>
    </div>
  );
}

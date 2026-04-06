import { useCallback, useEffect, useLayoutEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { SessionWorkbenchSidebar } from "./SessionWorkbenchSidebar";
import { SessionWorkbenchTerminal } from "./SessionWorkbenchTerminal";
import { useSessionLifecycle } from "./useSessionLifecycle";
import { sessionWorkbenchKeys } from "./api/query";
import { workbenchKeys } from "@/features/workbench/api/query";
import {
  appendMessage,
  createSession,
  getSession,
  getWorkbench,
  subscribeToSessionEvents,
  type DesktopSessionDetail,
} from "@/lib/tauri";
import { useSettingsStore } from "@/state/settings-store";
import { useTabsStore } from "@/state/tabs-store";

interface SessionWorkbenchPageProps {
  tabId: string;
  sessionId?: string;
  showSessionSidebar?: boolean;
  syncTabState?: boolean;
  autoSelectFallbackSession?: boolean;
}

export function SessionWorkbenchPage({
  tabId,
  sessionId,
  showSessionSidebar,
  syncTabState = true,
  autoSelectFallbackSession = true,
}: SessionWorkbenchPageProps) {
  const showSidebarPreference = useSettingsStore(
    (state) => state.showSessionSidebar
  );
  const updateTabSession = useTabsStore((state) => state.updateTabSession);
  const queryClient = useQueryClient();
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(
    sessionId ?? null
  );
  // Auto-hide sidebar on narrow viewports
  const [isNarrow, setIsNarrow] = useState(false);
  useLayoutEffect(() => {
    const mq = window.matchMedia("(max-width: 640px)");
    setIsNarrow(mq.matches);
    const handler = (e: MediaQueryListEvent) => setIsNarrow(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  const resolvedShowSidebar =
    (showSessionSidebar ?? showSidebarPreference) && !isNarrow;

  const workbenchQuery = useQuery({
    queryKey: workbenchKeys.root(),
    queryFn: getWorkbench,
  });

  const activeSessionId = sessionId ?? selectedSessionId;

  const activeSessionQuery = useQuery({
    queryKey: sessionWorkbenchKeys.detail(activeSessionId),
    queryFn: () => getSession(activeSessionId!),
    enabled: Boolean(activeSessionId),
  });

  // Session lifecycle (cancel, delete, rename, resume)
  const lifecycle = useSessionLifecycle({
    activeSessionId,
    onSessionDeleted: useCallback(
      (deletedId: string) => {
        if (selectedSessionId === deletedId) {
          setSelectedSessionId(null);
        }
      },
      [selectedSessionId]
    ),
  });

  useEffect(() => {
    setSelectedSessionId(sessionId ?? null);
  }, [sessionId]);

  useEffect(() => {
    if (!autoSelectFallbackSession) return;
    if (sessionId || selectedSessionId) return;
    const fallbackSessionId =
      workbenchQuery.data?.active_session_id ??
      workbenchQuery.data?.session_sections[0]?.sessions[0]?.id ??
      null;
    if (fallbackSessionId) {
      setSelectedSessionId(fallbackSessionId);
    }
  }, [
    autoSelectFallbackSession,
    sessionId,
    selectedSessionId,
    workbenchQuery.data,
  ]);

  useEffect(() => {
    if (!activeSessionId) return;

    let cancelled = false;
    let dispose: (() => void) | null = null;

    void subscribeToSessionEvents(activeSessionId, {
      onSnapshot: (session) => {
        queryClient.setQueryData(sessionWorkbenchKeys.detail(session.id), session);
        void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
      },
      onMessage: (nextSessionId, message) => {
        queryClient.setQueryData(
          sessionWorkbenchKeys.detail(nextSessionId),
          (
            current: DesktopSessionDetail | undefined
          ): DesktopSessionDetail | undefined => {
            if (!current) return current;
            return {
              ...current,
              session: {
                ...current.session,
                messages: [...current.session.messages, message],
              },
            };
          }
        );
        void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
      },
    }).then((nextDispose) => {
      if (cancelled) {
        nextDispose();
        return;
      }
      dispose = nextDispose;
    });

    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [activeSessionId, queryClient]);

  useEffect(() => {
    if (!syncTabState) return;
    if (!activeSessionId) return;
    updateTabSession({
      id: tabId,
      sessionId: activeSessionId,
      title: activeSessionQuery.data?.title,
    });
  }, [
    activeSessionId,
    activeSessionQuery.data?.title,
    syncTabState,
    tabId,
    updateTabSession,
  ]);

  const createSessionMutation = useMutation({
    mutationFn: () =>
      createSession({
        title: "New session",
        project_name: workbenchQuery.data?.project_name,
      }),
    onSuccess: (response) => {
      queryClient.setQueryData(
        sessionWorkbenchKeys.detail(response.session.id),
        response.session
      );
      setSelectedSessionId(response.session.id);
      void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
    },
  });

  const sendMessageMutation = useMutation({
    mutationFn: ({
      nextSessionId,
      message,
    }: {
      nextSessionId: string;
      message: string;
    }) => appendMessage(nextSessionId, message),
    onSuccess: (response) => {
      queryClient.setQueryData(
        sessionWorkbenchKeys.detail(response.session.id),
        response.session
      );
      void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
    },
  });

  const activeSession = activeSessionQuery.data ?? null;
  const errorMessage = extractErrorMessage(
    workbenchQuery.error,
    activeSessionQuery.error,
    sendMessageMutation.error,
    createSessionMutation.error,
    lifecycle.cancelError,
    lifecycle.deleteError
  );

  async function handleCreateSession() {
    await createSessionMutation.mutateAsync();
  }

  async function handleSend(message: string) {
    let nextSessionId = activeSessionId;

    if (!nextSessionId) {
      const response = await createSessionMutation.mutateAsync();
      nextSessionId = response.session.id;
    }

    await sendMessageMutation.mutateAsync({ nextSessionId, message });
  }

  return (
    <div className="flex h-full">
      {resolvedShowSidebar && (
        <SessionWorkbenchSidebar
          sessionSections={workbenchQuery.data?.session_sections ?? []}
          activeSessionId={activeSessionId}
          projectLabel={workbenchQuery.data?.project_label ?? "All projects"}
          onSelectSession={setSelectedSessionId}
          onCreateSession={() => {
            void handleCreateSession();
          }}
          onDeleteSession={lifecycle.handleDelete}
          isCreatingSession={createSessionMutation.isPending}
        />
      )}
      <div className="flex flex-1 flex-col overflow-hidden">
        <SessionWorkbenchTerminal
          session={activeSession}
          isLoadingSession={activeSessionQuery.isLoading}
          isSending={sendMessageMutation.isPending}
          errorMessage={errorMessage}
          onSend={handleSend}
          onStop={lifecycle.handleCancel}
          onCreateSession={() => void handleCreateSession()}
          modelLabel={
            activeSession?.model_label ??
            workbenchQuery.data?.composer.model_label ??
            "Opus 4.6"
          }
          environmentLabel={
            activeSession?.environment_label ??
            workbenchQuery.data?.composer.environment_label ??
            "Local"
          }
          projectPath={workbenchQuery.data?.project_label}
        />
      </div>
    </div>
  );
}

function extractErrorMessage(...errors: Array<unknown>): string | undefined {
  for (const error of errors) {
    if (error instanceof Error) {
      return error.message;
    }
  }

  return undefined;
}

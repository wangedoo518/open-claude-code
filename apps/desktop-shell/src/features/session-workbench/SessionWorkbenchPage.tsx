import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useAppDispatch, useAppSelector } from "@/store";
import { SessionWorkbenchSidebar } from "./SessionWorkbenchSidebar";
import { SessionWorkbenchTerminal } from "./SessionWorkbenchTerminal";
import { updateTabSession } from "@/store/slices/tabs";
import {
  appendMessage,
  createSession,
  getSession,
  getWorkbench,
  subscribeToSessionEvents,
  type DesktopSessionDetail,
} from "@/lib/tauri";

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
  const dispatch = useAppDispatch();
  const showSidebarPreference = useAppSelector(
    (s) => s.settings.showSessionSidebar
  );
  const queryClient = useQueryClient();
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(
    sessionId ?? null
  );
  const resolvedShowSidebar = showSessionSidebar ?? showSidebarPreference;

  const workbenchQuery = useQuery({
    queryKey: ["desktop-workbench"],
    queryFn: getWorkbench,
  });

  const activeSessionId = sessionId ?? selectedSessionId;

  const activeSessionQuery = useQuery({
    queryKey: ["desktop-session", activeSessionId],
    queryFn: () => getSession(activeSessionId!),
    enabled: Boolean(activeSessionId),
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
        queryClient.setQueryData(["desktop-session", session.id], session);
        void queryClient.invalidateQueries({ queryKey: ["desktop-workbench"] });
      },
      onMessage: (nextSessionId, message) => {
        queryClient.setQueryData(
          ["desktop-session", nextSessionId],
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
        void queryClient.invalidateQueries({ queryKey: ["desktop-workbench"] });
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
    dispatch(
      updateTabSession({
        id: tabId,
        sessionId: activeSessionId,
        title: activeSessionQuery.data?.title,
      })
    );
  }, [
    activeSessionId,
    activeSessionQuery.data?.title,
    dispatch,
    syncTabState,
    tabId,
  ]);

  const createSessionMutation = useMutation({
    mutationFn: () =>
      createSession({
        title: "New session",
        project_name: workbenchQuery.data?.project_name,
      }),
    onSuccess: (response) => {
      queryClient.setQueryData(
        ["desktop-session", response.session.id],
        response.session
      );
      setSelectedSessionId(response.session.id);
      void queryClient.invalidateQueries({ queryKey: ["desktop-workbench"] });
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
        ["desktop-session", response.session.id],
        response.session
      );
      void queryClient.invalidateQueries({ queryKey: ["desktop-workbench"] });
    },
  });

  const activeSession = activeSessionQuery.data ?? null;
  const errorMessage = extractErrorMessage(
    workbenchQuery.error,
    activeSessionQuery.error,
    sendMessageMutation.error,
    createSessionMutation.error
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
          modelLabel={
            activeSession?.model_label ??
            workbenchQuery.data?.composer.model_label ??
            "Opus 4.6"
          }
          permissionModeLabel={
            workbenchQuery.data?.composer.permission_mode_label ??
            "Ask permissions"
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

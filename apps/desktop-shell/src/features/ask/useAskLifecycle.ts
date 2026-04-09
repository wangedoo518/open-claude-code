/**
 * S0.3 extraction target: Ask session lifecycle hook.
 *
 * Original: features/session-workbench/useSessionLifecycle.ts. Verbatim
 * port; only the query-key imports are rewritten to their new path:
 *   - `./api/query` → `../session-workbench/api/query`
 *
 * The `session-workbench/api/query` module will be deleted in S0.4. At
 * that point, S3 (ask_runtime) will replace these imports with
 * `features/ask/api/query`. Until then this hook bridges the old query
 * keys so the extracted Ask page stays wired to the current backend.
 *
 * Wraps Tauri API calls with React Query mutations and provides error
 * handling + query cache invalidation.
 */

import { useCallback } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { workbenchKeys } from "@/features/workbench/api/query";
import {
  cancelSession,
  deleteSession,
  renameSession,
  resumeSession,
  type DesktopSessionDetail,
} from "@/lib/tauri";
import { sessionWorkbenchKeys } from "../session-workbench/api/query";

interface UseAskLifecycleOptions {
  activeSessionId?: string | null;
  onSessionDeleted?: (sessionId: string) => void;
  onSessionCancelled?: (session: DesktopSessionDetail) => void;
}

export function useAskLifecycle({
  activeSessionId,
  onSessionDeleted,
  onSessionCancelled,
}: UseAskLifecycleOptions = {}) {
  const queryClient = useQueryClient();

  // Cancel (stop) a running session
  const cancelMutation = useMutation({
    mutationFn: (sessionId: string) => cancelSession(sessionId),
    onSuccess: (session) => {
      queryClient.setQueryData(sessionWorkbenchKeys.detail(session.id), session);
      void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
      onSessionCancelled?.(session);
    },
  });

  // Delete a session
  const deleteMutation = useMutation({
    mutationFn: (sessionId: string) => deleteSession(sessionId),
    onSuccess: (_result, sessionId) => {
      queryClient.removeQueries({
        queryKey: sessionWorkbenchKeys.detail(sessionId),
      });
      void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
      onSessionDeleted?.(sessionId);
    },
  });

  // Rename a session
  const renameMutation = useMutation({
    mutationFn: ({ sessionId, title }: { sessionId: string; title: string }) =>
      renameSession(sessionId, title),
    onSuccess: (session) => {
      queryClient.setQueryData(sessionWorkbenchKeys.detail(session.id), session);
      void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
    },
  });

  // Resume a detached session
  const resumeMutation = useMutation({
    mutationFn: (sessionId: string) => resumeSession(sessionId),
    onSuccess: (session) => {
      queryClient.setQueryData(sessionWorkbenchKeys.detail(session.id), session);
      void queryClient.invalidateQueries({ queryKey: workbenchKeys.root() });
    },
  });

  // Convenience wrappers that use activeSessionId
  const handleCancel = useCallback(() => {
    if (activeSessionId) {
      cancelMutation.mutate(activeSessionId);
    }
  }, [activeSessionId, cancelMutation]);

  const handleDelete = useCallback(
    (sessionId?: string) => {
      const id = sessionId ?? activeSessionId;
      if (id) {
        deleteMutation.mutate(id);
      }
    },
    [activeSessionId, deleteMutation]
  );

  const handleRename = useCallback(
    (title: string, sessionId?: string) => {
      const id = sessionId ?? activeSessionId;
      if (id) {
        renameMutation.mutate({ sessionId: id, title });
      }
    },
    [activeSessionId, renameMutation]
  );

  const handleResume = useCallback(
    (sessionId?: string) => {
      const id = sessionId ?? activeSessionId;
      if (id) {
        resumeMutation.mutate(id);
      }
    },
    [activeSessionId, resumeMutation]
  );

  return {
    // Mutations
    cancelMutation,
    deleteMutation,
    renameMutation,
    resumeMutation,

    // Convenience handlers
    handleCancel,
    handleDelete,
    handleRename,
    handleResume,

    // Loading states
    isCancelling: cancelMutation.isPending,
    isDeleting: deleteMutation.isPending,
    isRenaming: renameMutation.isPending,
    isResuming: resumeMutation.isPending,

    // Error states
    cancelError: cancelMutation.error,
    deleteError: deleteMutation.error,
    renameError: renameMutation.error,
    resumeError: resumeMutation.error,
  };
}

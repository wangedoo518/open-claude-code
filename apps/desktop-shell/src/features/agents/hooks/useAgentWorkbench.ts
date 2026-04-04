/**
 * Hook that composes the AgentWorkbenchState from React Query data.
 *
 * Port from clawhub123/src/v2/features/agents/hooks/useAgentWorkbench.ts
 */

import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type {
  AgentId,
  AgentStatusNotice,
  AgentWorkbenchState,
} from "@/types/agent";
import {
  buildWorkbench,
  buildLoadingWorkbench,
  buildErrorWorkbench,
  resolvePrimaryActionKind,
  openclawActions,
} from "../services/openclawAgentController";
import { agentKeys, useAgentDetailQuery } from "../services/agentQueries";
import {
  useInstallAgentMutation,
  useStartAgentMutation,
  useStopAgentMutation,
  useUninstallAgentMutation,
} from "../services/agentMutations";

// ---------------------------------------------------------------------------
// useAgentWorkbench — Returns the computed workbench state
// ---------------------------------------------------------------------------

export function useAgentWorkbench(agentId: AgentId): AgentWorkbenchState {
  const detailQuery = useAgentDetailQuery(agentId);

  return useMemo(() => {
    if (detailQuery.isError && !detailQuery.data) {
      return buildErrorWorkbench(
        detailQuery.error instanceof Error
          ? detailQuery.error.message
          : String(detailQuery.error)
      );
    }
    if (!detailQuery.data) {
      return buildLoadingWorkbench();
    }
    return buildWorkbench(detailQuery.data);
  }, [detailQuery.data, detailQuery.error, detailQuery.isError]);
}

// ---------------------------------------------------------------------------
// useAgentPanelActions — Returns action handlers and pending flags
// ---------------------------------------------------------------------------

export function useAgentPanelActions(agentId: AgentId) {
  const queryClient = useQueryClient();
  const [actionNotice, setActionNotice] = useState<AgentStatusNotice | null>(
    null
  );
  const workbench = useAgentWorkbench(agentId);
  const installMutation = useInstallAgentMutation(agentId);
  const startMutation = useStartAgentMutation(agentId);
  const stopMutation = useStopAgentMutation(agentId);
  const uninstallMutation = useUninstallAgentMutation(agentId);

  const asErrorMessage = (error: unknown) =>
    error instanceof Error ? error.message : String(error);

  const refreshDetail = async () => {
    await queryClient.invalidateQueries({
      queryKey: agentKeys.detail(agentId),
    });
    await queryClient.refetchQueries({
      queryKey: agentKeys.detail(agentId),
    });
  };

  const stopThenStart = async (notice: AgentStatusNotice) => {
    setActionNotice(notice);
    try {
      await stopMutation.mutateAsync();
    } catch {
      // Stop may fail if service not running — continue to start
    }
    await startMutation.mutateAsync();
    await refreshDetail();
    setActionNotice(null);
  };

  const primaryAction = async () => {
    // Handle error/loading states
    if (workbench.kind === "error") {
      await refreshDetail();
      return;
    }
    if (workbench.kind !== "supported") {
      return;
    }

    try {
      const nextAction = resolvePrimaryActionKind(workbench.detail);

      if (nextAction === "install") {
        setActionNotice({ tone: "info", message: "正在安装 OpenClaw..." });
        await installMutation.mutateAsync();
        await refreshDetail();
        setActionNotice(null);
        return;
      }

      if (nextAction === "start") {
        await stopThenStart({
          tone: "info",
          message: "正在清理旧的 OpenClaw 进程并启动服务...",
        });
        return;
      }

      // Dashboard
      setActionNotice({
        tone: "info",
        message: "正在通过外部浏览器打开 OpenClaw 对话页...",
      });
      await openclawActions.openDashboard(workbench.detail);
      setActionNotice(null);
    } catch (error) {
      setActionNotice({ tone: "error", message: asErrorMessage(error) });
    }
  };

  const restartService = async () => {
    if (workbench.kind !== "supported") return;
    try {
      await stopThenStart({
        tone: "info",
        message: "正在重启 OpenClaw 服务...",
      });
    } catch (error) {
      setActionNotice({ tone: "error", message: asErrorMessage(error) });
    }
  };

  const uninstall = async () => {
    if (workbench.kind !== "supported") return;
    try {
      await uninstallMutation.mutateAsync();
      await refreshDetail();
      setActionNotice(null);
    } catch (error) {
      setActionNotice({ tone: "error", message: asErrorMessage(error) });
    }
  };

  return {
    installPending: installMutation.isPending,
    startPending: startMutation.isPending || stopMutation.isPending,
    uninstallPending: uninstallMutation.isPending,
    actionNotice,
    primaryAction,
    restartService,
    refreshDetail,
    uninstall,
  };
}

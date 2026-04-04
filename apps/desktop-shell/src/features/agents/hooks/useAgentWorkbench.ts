/**
 * Hook that composes the AgentWorkbenchState from React Query data.
 *
 * Port from clawhub123/src/v2/features/agents/hooks/useAgentWorkbench.ts
 */

import { useCallback, useEffect, useMemo, useState } from "react";
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
  dashboardUrl,
} from "../services/openclawAgentController";
import { agentKeys, useAgentDetailQuery } from "../services/agentQueries";
import {
  useInstallAgentMutation,
  useStartAgentMutation,
  useStopAgentMutation,
  useUninstallAgentMutation,
} from "../services/agentMutations";
import { useMinappPopup } from "@/hooks/useMinappPopup";
import { createOpenClawDashboardApp } from "@/features/workbench/openclawDashboard";

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
  const { openSmartMinapp } = useMinappPopup();
  const [actionNotice, setActionNotice] = useState<AgentStatusNotice | null>(
    null
  );
  const [openDashboardOnRunning, setOpenDashboardOnRunning] = useState(false);
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

  const openDashboardTab = useCallback(
    (url: string) => {
      openSmartMinapp(createOpenClawDashboardApp(url));
    },
    [openSmartMinapp]
  );

  useEffect(() => {
    if (!openDashboardOnRunning || workbench.kind !== "supported") {
      return;
    }

    if (workbench.detail.product.service_running) {
      openDashboardTab(dashboardUrl(workbench.detail));
      setOpenDashboardOnRunning(false);
      setActionNotice(null);
      return;
    }

    if (
      workbench.detail.serviceStatus.finished &&
      !workbench.detail.serviceStatus.success
    ) {
      setOpenDashboardOnRunning(false);
      setActionNotice({
        tone: "error",
        message:
          workbench.detail.serviceStatus.hint ?? "启动 OpenClaw 服务失败",
      });
    }
  }, [openDashboardOnRunning, openDashboardTab, workbench]);

  const stopThenStart = async (
    notice: AgentStatusNotice,
    options?: { keepNotice?: boolean }
  ) => {
    setActionNotice(notice);
    try {
      await stopMutation.mutateAsync();
    } catch {
      // Stop may fail if service not running — continue to start
    }
    await startMutation.mutateAsync();
    await refreshDetail();
    if (!options?.keepNotice) {
      setActionNotice(null);
    }
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
        setOpenDashboardOnRunning(true);
        await stopThenStart(
          {
            tone: "info",
            message: "正在清理旧的 OpenClaw 进程并启动服务...",
          },
          { keepNotice: true }
        );
        return;
      }

      // Dashboard
      setActionNotice({
        tone: "info",
        message: "正在打开 OpenClaw 对话页...",
      });
      openDashboardTab(dashboardUrl(workbench.detail));
      setActionNotice(null);
    } catch (error) {
      setOpenDashboardOnRunning(false);
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
      setOpenDashboardOnRunning(false);
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

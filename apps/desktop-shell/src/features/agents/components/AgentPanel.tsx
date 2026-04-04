/**
 * AgentPanel — Root component for the OpenClaw agent management page.
 *
 * Port from clawhub123/src/v2/features/agents/components/AgentPanel.tsx
 * Orchestrates:
 * - AgentDetailHero (brand, status, primary action, status strip)
 * - AgentLifecycleSection (install/start/uninstall log tabs)
 */

import { useState } from "react";
import { Loader2 } from "lucide-react";
import type { AgentId } from "@/types/agent";
import {
  useAgentWorkbench,
  useAgentPanelActions,
} from "../hooks/useAgentWorkbench";
import { AgentDetailHero } from "./AgentDetailHero";
import { AgentLifecycleSection } from "./AgentLifecycleSection";

interface AgentPanelProps {
  agentId: AgentId;
}

export function AgentPanel({ agentId }: AgentPanelProps) {
  const workbench = useAgentWorkbench(agentId);
  const [preferredLogTab, setPreferredLogTab] = useState<
    "install" | "start" | "uninstall" | undefined
  >(undefined);
  const [preferredLogTabNonce, setPreferredLogTabNonce] = useState(0);

  const {
    actionNotice,
    installPending,
    primaryAction,
    refreshDetail,
    restartService,
    startPending,
    uninstall,
    uninstallPending,
  } = useAgentPanelActions(agentId);

  const requestLogTab = (tab: "install" | "start" | "uninstall") => {
    setPreferredLogTab(tab);
    setPreferredLogTabNonce((prev) => prev + 1);
  };

  const handlePrimaryAction = async () => {
    if (workbench.kind === "supported") {
      if (!workbench.detail.product.installed) {
        requestLogTab("install");
      } else if (!workbench.detail.product.service_running) {
        requestLogTab("start");
      }
    }
    await primaryAction();
  };

  const handleRestart = async () => {
    requestLogTab("start");
    await restartService();
  };

  const handleUninstall = async () => {
    requestLogTab("uninstall");
    await uninstall();
  };

  return (
    <div className="flex flex-col gap-3.5 max-w-4xl mx-auto w-full px-4 py-6">
      <AgentDetailHero
        installPending={installPending}
        onPrimaryAction={handlePrimaryAction}
        onRestart={handleRestart}
        onUninstall={handleUninstall}
        startPending={startPending}
        statusNotice={actionNotice}
        uninstallPending={uninstallPending}
        workbench={workbench}
      />

      {/* Loading indicator during pipeline operations */}
      {workbench.kind === "supported" && (installPending || startPending) && (
        <div className="flex items-center justify-center gap-2 py-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          <span>正在刷新 OpenClaw 状态...</span>
        </div>
      )}

      <AgentLifecycleSection
        onRefresh={refreshDetail}
        preferredTab={preferredLogTab}
        preferredTabNonce={preferredLogTabNonce}
        workbench={workbench}
      />
    </div>
  );
}

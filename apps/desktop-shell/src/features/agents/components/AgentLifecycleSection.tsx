/**
 * AgentLifecycleSection — Log tabs (install / start / uninstall) with real-time log cards
 *
 * Port from clawhub123/src/v2/features/agents/components/AgentLifecycleSection.tsx
 * Converted from Ant Design + BEM CSS to Tailwind + shadcn/ui
 */

import { useEffect, useMemo, useState } from "react";
import type { AgentWorkbenchState } from "@/types/agent";
import { cn } from "@/lib/utils";
import { RealtimeLogCard } from "./RealtimeLogCard";

type LogTabKey = "install" | "start" | "uninstall";

interface LogTab {
  key: LogTabKey;
  label: string;
  hint?: string | null;
  logs: string[];
  emptyText: string;
  running: boolean;
  finished: boolean;
  success: boolean;
  updatedAt: number;
}

function pickPreferredTab(tabs: LogTab[], installed: boolean): LogTabKey {
  // Running tab takes priority
  const runningTab = tabs.find((tab) => tab.running);
  if (runningTab) return runningTab.key;

  // Most recent tab with logs
  const recentTabWithLogs = [...tabs]
    .filter((tab) => tab.logs.length > 0)
    .sort((a, b) => b.updatedAt - a.updatedAt)[0];
  if (recentTabWithLogs) return recentTabWithLogs.key;

  // Most recent failure
  const recentFailure = [...tabs]
    .filter((tab) => tab.finished && !tab.success)
    .sort((a, b) => b.updatedAt - a.updatedAt)[0];
  if (recentFailure) return recentFailure.key;

  // Default to start if installed
  if (installed && tabs.some((tab) => tab.key === "start")) return "start";

  return tabs[0]?.key ?? "install";
}

interface AgentLifecycleSectionProps {
  workbench: AgentWorkbenchState;
  onRefresh: () => void;
  preferredTab?: LogTabKey;
  preferredTabNonce?: number;
}

export function AgentLifecycleSection({
  workbench,
  onRefresh,
  preferredTab,
  preferredTabNonce,
}: AgentLifecycleSectionProps) {
  const isSupported = workbench.kind === "supported";
  const loadingWorkbench = workbench.kind === "loading";

  // Build tabs from lifecycle steps
  const tabs = useMemo<LogTab[]>(() => {
    if (!isSupported) return [];
    const detail = workbench.detail;
    const next: LogTab[] = [];

    // Install tab
    const installStep = workbench.lifecycleSteps.find((s) => s.id === "install");
    if (installStep) {
      next.push({
        key: "install",
        label: "安装日志",
        hint: installStep.hint,
        logs: installStep.logs,
        emptyText: installStep.emptyText,
        running: detail.installStatus.running,
        finished: detail.installStatus.finished,
        success: detail.installStatus.success,
        updatedAt: detail.installStatus.updated_at_epoch,
      });
    }

    // Start tab
    const startStep = workbench.lifecycleSteps.find((s) => s.id === "start");
    if (startStep) {
      next.push({
        key: "start",
        label: "启动日志",
        hint: startStep.hint,
        logs: startStep.logs,
        emptyText: startStep.emptyText,
        running: detail.serviceStatus.running,
        finished: detail.serviceStatus.finished,
        success: detail.serviceStatus.success,
        updatedAt: detail.serviceStatus.updated_at_epoch,
      });
    }

    // Uninstall tab (only shown if there's meaningful data)
    if (
      detail.uninstallStatus.running ||
      detail.uninstallStatus.finished ||
      detail.uninstallStatus.logs.length > 0 ||
      detail.uninstallStatus.hint
    ) {
      next.push({
        key: "uninstall",
        label: "卸载日志",
        hint: detail.uninstallStatus.hint,
        logs: detail.uninstallStatus.logs,
        emptyText: "暂无卸载日志",
        running: detail.uninstallStatus.running,
        finished: detail.uninstallStatus.finished,
        success: detail.uninstallStatus.success,
        updatedAt: detail.uninstallStatus.updated_at_epoch,
      });
    }

    return next;
  }, [isSupported, workbench]);

  const installed = isSupported ? workbench.detail.product.installed : false;

  const [activeTab, setActiveTab] = useState<LogTabKey>(
    isSupported ? pickPreferredTab(tabs, installed) : "install"
  );
  const [clearedCounts, setClearedCounts] = useState<
    Partial<Record<LogTabKey, number>>
  >({});
  const [cachedLogs, setCachedLogs] = useState<Record<LogTabKey, string[]>>({
    install: [],
    start: [],
    uninstall: [],
  });

  // Cache logs to persist them across tab switches
  useEffect(() => {
    if (!isSupported || tabs.length === 0) return;
    setCachedLogs((prev) => {
      let changed = false;
      const next = { ...prev };
      tabs.forEach((tab) => {
        if (tab.logs.length === 0) return;
        const prior = Array.isArray(prev[tab.key]) ? prev[tab.key] : [];
        const same =
          prior.length === tab.logs.length &&
          prior.every((line, i) => line === tab.logs[i]);
        if (same) return;
        next[tab.key] = tab.logs;
        changed = true;
      });
      return changed ? next : prev;
    });
  }, [isSupported, tabs]);

  // Handle preferred tab from parent
  useEffect(() => {
    if (!preferredTab || !tabs.some((t) => t.key === preferredTab)) return;
    setActiveTab(preferredTab);
  }, [preferredTab, preferredTabNonce, tabs]);

  // Auto-switch to running tab
  useEffect(() => {
    if (!isSupported || tabs.length === 0) return;
    setActiveTab((prev) => {
      if (!tabs.some((t) => t.key === prev)) {
        return pickPreferredTab(tabs, installed);
      }
      const runningTab = tabs.find((t) => t.running);
      if (runningTab && runningTab.key !== prev) return runningTab.key;
      return prev;
    });
  }, [installed, isSupported, tabs]);

  // ── Loading / unsupported state ──────────────────────────────────────
  if (!isSupported) {
    return (
      <section className="rounded-2xl border border-border bg-card p-4">
        <div className="mb-3">
          <div className="text-xs font-medium text-muted-foreground tracking-wide uppercase">
            实时日志
          </div>
          <h4 className="text-sm font-semibold text-foreground mt-1">
            安装与启动流水线
          </h4>
        </div>
        <p className="text-sm text-muted-foreground leading-relaxed">
          {loadingWorkbench
            ? "正在读取 OpenClaw 安装与启动状态，完成后会展示实时日志与诊断信息。"
            : "该 Agent 仍处于规划阶段。"}
        </p>
      </section>
    );
  }

  // ── Supported state: log tabs ────────────────────────────────────────
  const currentTab = tabs.find((t) => t.key === activeTab) ?? tabs[0];
  const currentTabLogs = currentTab
    ? currentTab.logs.length > 0 || currentTab.running
      ? currentTab.logs
      : Array.isArray(cachedLogs[currentTab.key])
        ? cachedLogs[currentTab.key]
        : []
    : [];
  const visibleLogs = currentTab
    ? currentTabLogs.slice(
        Math.min(clearedCounts[currentTab.key] ?? 0, currentTabLogs.length)
      )
    : [];

  return (
    <section className="rounded-2xl border border-border bg-card overflow-hidden">
      {currentTab && (
        <>
          {/* Tab buttons */}
          <div className="flex items-center gap-0 border-b border-border">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                type="button"
                className={cn(
                  "px-4 py-2.5 text-sm font-medium border-b-2 transition-colors",
                  tab.key === currentTab.key
                    ? "border-primary text-primary"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                )}
                onClick={() => setActiveTab(tab.key)}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {/* Log card */}
          <div>
            <RealtimeLogCard
              lines={visibleLogs}
              title={currentTab.label}
              emptyText={currentTab.emptyText}
              height={260}
              lineColor={(line) =>
                line.includes("[stderr]") ? "#ff7875" : "#52c41a"
              }
              onRefresh={() => void onRefresh()}
              onClear={() =>
                setClearedCounts((prev) => ({
                  ...prev,
                  [currentTab.key]: currentTabLogs.length,
                }))
              }
            />
          </div>
        </>
      )}
    </section>
  );
}

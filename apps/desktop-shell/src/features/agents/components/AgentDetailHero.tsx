/**
 * AgentDetailHero — Hero section with brand, status, and primary action
 *
 * Port from clawhub123/src/v2/features/agents/components/AgentDetailHero.tsx
 * Converted from Ant Design + BEM CSS to Tailwind + shadcn/ui
 *
 * Three visual states:
 * 1. Not installed — centered icon + install button
 * 2. Installed — full layout with status strip, restart/uninstall
 * 3. Loading — placeholder with disabled button
 */

import { Loader2, Download, Power, RotateCcw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { AgentStatusNotice, AgentWorkbenchState } from "@/types/agent";
import { AgentBrandIcon } from "./AgentBrandIcon";

interface AgentDetailHeroProps {
  workbench: AgentWorkbenchState;
  installPending: boolean;
  startPending: boolean;
  uninstallPending: boolean;
  statusNotice: AgentStatusNotice | null;
  onPrimaryAction: () => void;
  onRestart: () => void;
  onUninstall: () => void;
}

export function AgentDetailHero({
  workbench,
  installPending,
  startPending,
  uninstallPending,
  statusNotice,
  onPrimaryAction,
  onRestart,
  onUninstall,
}: AgentDetailHeroProps) {
  const loading = installPending || startPending || uninstallPending;
  const loadingWorkbench = workbench.kind === "loading";

  // ── Supported state (detail loaded) ──────────────────────────────────
  if (workbench.kind === "supported") {
    const installed = workbench.detail.product.installed;
    const running = workbench.detail.product.service_running;
    const port =
      workbench.detail.serviceStatus.dashboard_url?.match(/:(\d+)/)?.[1] ??
      "18790";
    const heroNotice = statusNotice ?? workbench.statusNotice;
    const serviceStateLabel = running
      ? "运行中"
      : workbench.detail.serviceStatus.running
        ? "启动中"
        : "未运行";

    // Compact detail pills
    const compactDetails = [
      ...workbench.heroSummary,
      ...workbench.runtimeMetrics
        .filter((m) => m.label === "运行状态" || m.label === "内存占用")
        .map((m) => `${m.label} ${m.value}`),
      ...workbench.environmentItems
        .filter(
          (m) => m.label === "OpenClaw 版本" || m.label === "Node.js"
        )
        .map((m) => `${m.label} ${m.value}`),
    ];

    // ── Not installed ──────────────────────────────────────────────────
    if (!installed) {
      return (
        <section className="flex flex-col items-center text-center rounded-3xl bg-gradient-to-b from-white/[0.98] to-slate-50/[0.98] dark:from-slate-900/95 dark:to-slate-800/95 shadow-lg p-10 border border-slate-900/[0.06] dark:border-white/[0.06]">
          {/* Header */}
          <div className="flex items-center justify-center gap-3.5">
            <AgentBrandIcon agentId="openclaw" variant="plain" />
            <h3 className="text-4xl font-bold tracking-tight leading-tight">
              OpenClaw
            </h3>
          </div>

          {/* Actions */}
          <div className="flex flex-col items-stretch w-full max-w-[840px] mt-4">
            <div
              className={cn(
                "w-full mb-1 text-sm leading-snug text-left",
                heroNotice.tone === "error" && "text-red-500",
                heroNotice.tone === "warning" && "text-amber-500",
                heroNotice.tone === "success" && "text-green-600",
                (heroNotice.tone === "default" || heroNotice.tone === "info") &&
                  "text-foreground"
              )}
            >
              {heroNotice.message}
            </div>
            <Button
              size="lg"
              className="w-full h-12 rounded-[14px] text-base font-semibold"
              disabled={loading}
              onClick={() => void onPrimaryAction()}
            >
              {loading ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Download className="size-4" />
              )}
              {workbench.primaryActionLabel}
            </Button>
          </div>
        </section>
      );
    }

    // ── Installed (with status strip) ──────────────────────────────────
    return (
      <section className="flex flex-col items-center text-center rounded-3xl bg-gradient-to-b from-white/[0.98] to-slate-50/[0.98] dark:from-slate-900/95 dark:to-slate-800/95 shadow-lg p-10 border border-slate-900/[0.06] dark:border-white/[0.06]">
        {/* Header */}
        <div className="flex items-center justify-center gap-3.5">
          <AgentBrandIcon agentId="openclaw" variant="plain" />
          <h3 className="text-4xl font-bold tracking-tight leading-tight">
            OpenClaw
          </h3>
        </div>

        {/* Actions */}
        <div className="flex flex-col items-stretch w-full max-w-[840px] mt-4">
          <div
            className={cn(
              "w-full mb-1 text-sm leading-snug text-left",
              heroNotice.tone === "error" && "text-red-500",
              heroNotice.tone === "warning" && "text-amber-500",
              heroNotice.tone === "success" && "text-green-600",
              (heroNotice.tone === "default" || heroNotice.tone === "info") &&
                "text-foreground"
            )}
          >
            {heroNotice.message}
          </div>
          <Button
            size="lg"
            className="w-full h-12 rounded-[14px] text-base font-semibold"
            disabled={loading}
            onClick={() => void onPrimaryAction()}
          >
            {loading ? (
              <Loader2 className="size-4 animate-spin" />
            ) : running ? (
              <AgentBrandIcon agentId="openclaw" variant="plain" />
            ) : (
              <Power className="size-4" />
            )}
            {workbench.primaryActionLabel}
          </Button>
        </div>

        {/* Status strip */}
        <div
          className={cn(
            "flex flex-col items-stretch gap-3.5 w-full max-w-[840px] mt-6 px-5 py-4 rounded-2xl",
            running
              ? "bg-gradient-to-b from-slate-900/[0.04] to-slate-900/[0.06] dark:from-white/[0.04] dark:to-white/[0.06]"
              : "bg-slate-900/[0.035] dark:bg-white/[0.035]"
          )}
        >
          {/* Top row: status + actions */}
          <div className="flex items-center justify-between gap-3.5">
            <div className="inline-flex items-center gap-2.5 text-sm text-foreground">
              <span
                className={cn(
                  "w-2.5 h-2.5 rounded-full",
                  running ? "bg-green-500" : "bg-slate-400"
                )}
              />
              <strong>{serviceStateLabel}</strong>
              <span className="text-muted-foreground font-semibold">
                :{port}
              </span>
            </div>
            <div className="inline-flex items-center gap-2.5">
              {running && (
                <button
                  className="inline-flex items-center gap-2 border-0 bg-transparent text-foreground font-semibold text-sm cursor-pointer disabled:opacity-45 disabled:cursor-not-allowed hover:text-primary"
                  disabled={startPending}
                  onClick={() => void onRestart()}
                  type="button"
                >
                  <RotateCcw className="size-[15px]" />
                  <span>重启</span>
                </button>
              )}
              <button
                className="inline-flex items-center gap-2 border-0 bg-transparent text-muted-foreground font-semibold text-sm cursor-pointer disabled:opacity-45 disabled:cursor-not-allowed hover:text-destructive"
                disabled={uninstallPending}
                onClick={() => void onUninstall()}
                type="button"
              >
                <Trash2 className="size-[15px]" />
                <span>{workbench.uninstallActionLabel}</span>
              </button>
            </div>
          </div>

          {/* Detail pills */}
          <div className="flex flex-wrap justify-start gap-2 w-full">
            {compactDetails.map((item) => (
              <span
                key={item}
                className="inline-flex items-center min-h-[24px] max-w-full px-2 py-0.5 rounded-full bg-slate-900/[0.04] dark:bg-white/[0.06] text-muted-foreground text-[10px] leading-tight break-words"
              >
                {item}
              </span>
            ))}
          </div>
        </div>
      </section>
    );
  }

  // ── Loading / Error state ────────────────────────────────────────────
  return (
    <section className="flex flex-col items-center text-center rounded-3xl bg-gradient-to-b from-white/[0.98] to-slate-50/[0.98] dark:from-slate-900/95 dark:to-slate-800/95 shadow-lg p-10 border border-slate-900/[0.06] dark:border-white/[0.06]">
      <div className="flex items-center justify-center gap-3.5">
        <AgentBrandIcon agentId="openclaw" variant="plain" />
        <div className="text-center">
          <h3 className="text-4xl font-bold tracking-tight leading-tight">
            OpenClaw
          </h3>
          <p className="mt-3.5 text-base text-muted-foreground max-w-[44ch] leading-relaxed">
            开源个人 AI 助手
          </p>
        </div>
      </div>

      <div className="mt-4 text-xs font-semibold text-muted-foreground leading-loose">
        {loadingWorkbench
          ? "正在读取 Agent 当前状态 · 完成后会自动切换到真实运行视图"
          : "统一 Agent 详情模板"}
      </div>

      <div className="flex flex-col items-stretch w-full max-w-[840px] mt-4">
        {workbench.statusNotice && (
          <div
            className={cn(
              "w-full mb-1 text-sm leading-snug text-left",
              workbench.statusNotice.tone === "error" && "text-red-500",
              workbench.statusNotice.tone === "warning" && "text-amber-500",
              (workbench.statusNotice.tone === "default" ||
                workbench.statusNotice.tone === "info") &&
                "text-foreground"
            )}
          >
            {workbench.statusNotice.message}
          </div>
        )}
        <Button
          size="lg"
          className="w-full h-12 rounded-[14px] text-base font-semibold"
          disabled={loadingWorkbench || loading}
          onClick={() => void onPrimaryAction()}
        >
          {(loading || loadingWorkbench) && (
            <Loader2 className="size-4 animate-spin" />
          )}
          {workbench.primaryActionLabel}
        </Button>
      </div>
    </section>
  );
}

/**
 * ConnectWeChatPipelinePage -- full-screen route page for the one-click
 * WeChat kefu (customer service) onboarding pipeline.
 *
 * Layout:
 *   Top bar: back | title | cancel
 *   Progress bar: 5 steps
 *   Left panel (instructions / spinner) + Right drawer (browser panel)
 *
 * The pipeline has 5 phases:
 *   Phase 1: CF 账号       -- manual (drawer OPEN, user does CAPTCHA)
 *   Phase 2: 部署中继      -- auto   (drawer CLOSED, centered spinner)
 *   Phase 3: 微信授权      -- manual (drawer OPEN, user scans QR)
 *   Phase 4: 回调配置      -- auto   (drawer CLOSED, centered spinner)
 *   Phase 5: 创建账号      -- auto   (drawer CLOSED, centered spinner)
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  startKefuPipeline,
  getKefuPipelineStatus,
  cancelKefuPipeline,
  getKefuContactUrl,
} from "@/api/desktop/settings";
import type {
  KefuPipelineState,
  PipelinePhaseState,
} from "@/api/desktop/settings";
import { kefuQueryKeys } from "./kefu-query-keys";
import { useSettingsStore } from "@/state/settings-store";

// ── Phase metadata ──────────────────────────────────────────────────

interface PhaseMeta {
  key: PipelinePhaseState["phase"];
  label: string;
  shortLabel: string;
  /** Whether this phase requires manual user action (drawer open). */
  manual: boolean;
  /** Icon displayed in the drawer browser panel placeholder. */
  icon: string;
  /** Brief description shown in the drawer placeholder. */
  drawerTitle: string;
}

const PHASES: PhaseMeta[] = [
  {
    key: "cf_register",
    label: "Cloudflare 账号注册",
    shortLabel: "CF 账号",
    manual: true,
    icon: "☁️",
    drawerTitle: "Cloudflare 注册 / 验证",
  },
  {
    key: "worker_deploy",
    label: "中继服务器部署",
    shortLabel: "部署中继",
    manual: false,
    icon: "🚀",
    drawerTitle: "正在部署 Worker...",
  },
  {
    key: "wecom_auth",
    label: "扫码授权企业微信",
    shortLabel: "微信授权",
    manual: true,
    icon: "📱",
    drawerTitle: "微信扫码授权",
  },
  {
    key: "callback_config",
    label: "回调 URL 配置",
    shortLabel: "回调配置",
    manual: false,
    icon: "🔗",
    drawerTitle: "正在配置回调...",
  },
  {
    key: "kefu_create",
    label: "创建客服账号",
    shortLabel: "创建账号",
    manual: false,
    icon: "👤",
    drawerTitle: "正在创建客服账号...",
  },
];

// ── Status helpers ──────────────────────────────────────────────────

type PhaseVisualStatus = "done" | "active" | "pending" | "failed";

function classifyStatus(status: PipelinePhaseState["status"]): PhaseVisualStatus {
  if (status === "done" || status === "skipped") return "done";
  if (status === "running" || status === "waiting_scan") return "active";
  if (status === "failed") return "failed";
  return "pending";
}

function statusIcon(visual: PhaseVisualStatus): string {
  switch (visual) {
    case "done":
      return "✓";
    case "active":
      return "◎";
    case "failed":
      return "✕";
    default:
      return "○";
  }
}

/** Return the first non-done phase index, or -1 if all done. */
function deriveCurrentPhaseIndex(phases: PipelinePhaseState[]): number {
  for (let i = 0; i < phases.length; i++) {
    const s = phases[i].status;
    if (s !== "done" && s !== "skipped") return i;
  }
  return -1;
}

function allPhasesDone(phases: PipelinePhaseState[]): boolean {
  return (
    phases.length > 0 &&
    phases.every((p) => p.status === "done" || p.status === "skipped")
  );
}

// ── Component ───────────────────────────────────────────────────────

export function ConnectWeChatPipelinePage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  // Browser drawer (zustand-based, Shell-level)
  const openBrowser = useSettingsStore((s) => s.openBrowser);
  const closeBrowser = useSettingsStore((s) => s.closeBrowser);
  const browserDrawerOpen = useSettingsStore((s) => s.browserDrawerOpen);

  const [logsExpanded, setLogsExpanded] = useState(true);
  const logEndRef = useRef<HTMLDivElement>(null);

  // ── Start pipeline mutation ──────────────────────────────────────
  const startMut = useMutation({
    mutationFn: () => startKefuPipeline({}),
  });

  // ── Poll pipeline status ─────────────────────────────────────────
  const pipelineQuery = useQuery({
    queryKey: kefuQueryKeys.pipeline(),
    queryFn: getKefuPipelineStatus,
    refetchInterval: 2000,
    refetchIntervalInBackground: true,
    refetchOnMount: "always",
    staleTime: 1000,
  });

  const pipeline: KefuPipelineState | undefined = pipelineQuery.data;
  const phases = pipeline?.phases ?? [];
  const logs = pipeline?.logs ?? [];
  const currentPhaseIdx = deriveCurrentPhaseIndex(phases);
  const isDone = allPhasesDone(phases) && !!pipeline?.contact_url;

  // ── Cancel mutation ──────────────────────────────────────────────
  const cancelMut = useMutation({
    mutationFn: cancelKefuPipeline,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: kefuQueryKeys.pipeline() });
    },
  });

  // ── Contact URL (for congrats view) ──────────────────────────────
  const contactUrlQuery = useQuery({
    queryKey: kefuQueryKeys.contactUrl(),
    queryFn: getKefuContactUrl,
    enabled: isDone,
  });
  const contactUrl = contactUrlQuery.data?.url ?? pipeline?.contact_url ?? null;

  // ── Kick off pipeline on mount if not already running ────────────
  const hasKickedOff = useRef(false);
  useEffect(() => {
    if (hasKickedOff.current) return;
    // If pipeline is already active or done, don't re-start
    if (pipeline === undefined) return; // still loading
    if (pipeline.active || allPhasesDone(pipeline.phases)) {
      hasKickedOff.current = true;
      return;
    }
    // If no phases have run yet, start the pipeline
    const anyNonPending = pipeline.phases.some((p) => p.status !== "pending");
    if (!anyNonPending && !pipeline.started_at) {
      hasKickedOff.current = true;
      startMut.mutate();
    } else {
      hasKickedOff.current = true;
    }
  }, [pipeline, startMut]);

  // ── Auto-open/close drawer based on phase type ───────────────────
  const currentPhaseMeta = currentPhaseIdx >= 0 ? PHASES[currentPhaseIdx] : null;
  useEffect(() => {
    if (isDone) {
      closeBrowser();
      return;
    }
    if (currentPhaseMeta) {
      if (currentPhaseMeta.manual) {
        openBrowser("", currentPhaseMeta.drawerTitle, currentPhaseMeta.icon);
      } else {
        closeBrowser();
      }
    }
  }, [currentPhaseMeta, isDone, openBrowser, closeBrowser]);

  // ── Auto-scroll logs ─────────────────────────────────────────────
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs.length]);

  // ── Handlers ─────────────────────────────────────────────────────
  const handleBack = useCallback(() => {
    navigate("/ask");
  }, [navigate]);

  const handleCancel = useCallback(() => {
    if (pipeline?.active) {
      cancelMut.mutate();
    }
    closeBrowser();
    navigate("/ask");
  }, [navigate, pipeline, cancelMut, closeBrowser]);

  const handleDone = useCallback(() => {
    closeBrowser();
    void queryClient.invalidateQueries({ queryKey: kefuQueryKeys.status() });
    void queryClient.invalidateQueries({ queryKey: kefuQueryKeys.config() });
    navigate("/ask");
  }, [navigate, queryClient, closeBrowser]);

  // Current phase state from pipeline data
  const currentPhaseState: PipelinePhaseState | null =
    currentPhaseIdx >= 0 && phases[currentPhaseIdx]
      ? phases[currentPhaseIdx]
      : null;

  // ── Render ───────────────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col bg-[var(--color-background)]">
      {/* ── Top bar ───────────────────────────────────────────── */}
      <div className="flex h-12 shrink-0 items-center justify-between border-b border-[var(--color-border)] px-4">
        <button
          type="button"
          onClick={handleBack}
          className="flex items-center gap-1.5 text-sm text-[var(--color-muted-foreground)] hover:text-[var(--color-foreground)]"
        >
          <span>◀</span> <span>返回</span>
        </button>
        <span className="text-sm font-semibold text-[var(--color-foreground)]">
          一键接入微信客服
        </span>
        <button
          type="button"
          onClick={handleCancel}
          className="text-sm text-[var(--color-muted-foreground)] hover:text-red-500"
        >
          取消
        </button>
      </div>

      {/* ── Progress bar ──────────────────────────────────────── */}
      <ProgressBar phases={phases} isDone={isDone} />

      {/* ── Main content area: left panel + right drawer ──────── */}
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* ── Left panel ────────────────────────────────── */}
        <div
          className="flex flex-col overflow-y-auto border-r border-[var(--color-border)]"
          style={{
            flex: browserDrawerOpen ? "0 0 320px" : "1 1 auto",
            transition: "flex 300ms ease",
          }}
        >
          {isDone ? (
            <CongratsView
              contactUrl={contactUrl}
              onDone={handleDone}
            />
          ) : currentPhaseMeta && !currentPhaseMeta.manual ? (
            /* Auto phase: centered spinner */
            <div className="flex flex-1 flex-col items-center justify-center px-6 py-12">
              <div className="mb-4 h-16 w-16 animate-spin rounded-full border-4 border-gray-200 border-t-indigo-500" />
              <h2 className="text-base font-semibold text-[var(--color-foreground)]">
                {currentPhaseMeta.label}
              </h2>
              <p className="mt-2 text-sm text-[var(--color-muted-foreground)]">
                自动执行中，无需操作
              </p>
              {currentPhaseState?.message && (
                <p className="mt-2 text-xs text-[var(--color-muted-foreground)]">
                  {currentPhaseState.message}
                </p>
              )}
            </div>
          ) : currentPhaseMeta && currentPhaseMeta.manual ? (
            /* Manual phase: instruction callout */
            <div className="flex flex-1 flex-col px-6 py-8">
              <div className="mb-6 rounded-lg border border-amber-300 bg-amber-50 p-4">
                <div className="mb-1 flex items-center gap-2 text-sm font-semibold text-amber-800">
                  <span>⚠️</span>
                  <span>需要手动操作</span>
                </div>
                <p className="text-sm leading-relaxed text-amber-700">
                  {currentPhaseMeta.key === "cf_register"
                    ? "请在右侧面板完成 Cloudflare 注册或验证。如果已有账号，系统会自动跳过此步骤。"
                    : "请使用微信扫描右侧面板中的二维码，完成企业微信授权。"}
                </p>
              </div>
              {currentPhaseState?.message && (
                <p className="mb-4 text-sm text-[var(--color-muted-foreground)]">
                  {currentPhaseState.message}
                </p>
              )}
              {currentPhaseState?.error && (
                <p className="mb-4 text-sm text-red-600">
                  {currentPhaseState.error}
                </p>
              )}
            </div>
          ) : (
            /* Loading / waiting state */
            <div className="flex flex-1 flex-col items-center justify-center px-6 py-12">
              <div className="mb-4 h-16 w-16 animate-spin rounded-full border-4 border-gray-200 border-t-indigo-500" />
              <p className="text-sm text-[var(--color-muted-foreground)]">
                {startMut.isPending ? "正在启动流程..." : "正在加载..."}
              </p>
              {startMut.error && (
                <p className="mt-2 text-sm text-red-600">
                  启动失败: {String((startMut.error as Error)?.message ?? startMut.error)}
                </p>
              )}
            </div>
          )}

          {/* ── Step list (always visible at bottom) ──── */}
          {phases.length > 0 && !isDone && (
            <StepList phases={phases} currentPhaseIdx={currentPhaseIdx} />
          )}

          {/* ── Log viewer ───────────────────────────── */}
          {logs.length > 0 && !isDone && (
            <LogViewer
              logs={logs}
              expanded={logsExpanded}
              onToggle={() => setLogsExpanded((v) => !v)}
              logEndRef={logEndRef}
            />
          )}
        </div>

        {/* Right drawer is now handled by the Shell-level BrowserDrawer component */}
      </div>
    </div>
  );
}

// ── Progress Bar ────────────────────────────────────────────────────

function ProgressBar({
  phases,
  isDone,
}: {
  phases: PipelinePhaseState[];
  isDone: boolean;
}) {
  if (phases.length === 0) return null;

  return (
    <div className="flex shrink-0 items-center gap-1 border-b border-[var(--color-border)] px-4 py-2">
      {PHASES.map((meta, idx) => {
        const phaseState = phases[idx];
        const visual: PhaseVisualStatus = phaseState
          ? classifyStatus(phaseState.status)
          : "pending";
        const isManualActive =
          visual === "active" && meta.manual;

        let bgClass = "bg-gray-200 text-gray-500";
        let dotClass = "bg-gray-400";
        if (isDone || visual === "done") {
          bgClass = "bg-green-100 text-green-700";
          dotClass = "bg-green-500";
        } else if (visual === "active") {
          bgClass = isManualActive
            ? "bg-orange-100 text-orange-700"
            : "bg-indigo-100 text-indigo-700";
          dotClass = isManualActive ? "bg-orange-500" : "bg-indigo-500";
        } else if (visual === "failed") {
          bgClass = "bg-red-100 text-red-700";
          dotClass = "bg-red-500";
        }

        return (
          <div key={meta.key} className="flex items-center gap-1">
            <div
              className={`flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium ${bgClass}`}
            >
              <span
                className={`inline-block h-2 w-2 rounded-full ${dotClass} ${visual === "active" ? "animate-pulse" : ""}`}
              />
              <span>{statusIcon(visual)}</span>
              <span>{meta.shortLabel}</span>
            </div>
            {/* Connector line between steps */}
            {idx < PHASES.length - 1 && (
              <div
                className={`h-0.5 w-4 ${
                  (phaseState && classifyStatus(phaseState.status) === "done") || isDone
                    ? "bg-green-400"
                    : "bg-gray-200"
                }`}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

// ── Step List ───────────────────────────────────────────────────────

function StepList({
  phases,
  currentPhaseIdx,
}: {
  phases: PipelinePhaseState[];
  currentPhaseIdx: number;
}) {
  return (
    <div className="border-t border-[var(--color-border)] px-6 py-4">
      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-[var(--color-muted-foreground)]">
        接入步骤
      </h3>
      <ul className="space-y-1.5">
        {PHASES.map((meta, idx) => {
          const phaseState = phases[idx];
          const visual: PhaseVisualStatus = phaseState
            ? classifyStatus(phaseState.status)
            : "pending";
          const isCurrent = idx === currentPhaseIdx;

          let textClass = "text-[var(--color-muted-foreground)]";
          if (visual === "done") textClass = "text-green-600";
          else if (visual === "active")
            textClass = "text-[var(--color-foreground)] font-medium";
          else if (visual === "failed") textClass = "text-red-600";

          return (
            <li
              key={meta.key}
              className={`flex items-center gap-2 rounded-md px-2 py-1.5 text-sm ${
                isCurrent ? "bg-[var(--color-accent)]" : ""
              } ${textClass}`}
            >
              <PhaseStatusDot visual={visual} manual={meta.manual} />
              <span>{meta.icon}</span>
              <span>{meta.label}</span>
              {phaseState?.status === "skipped" && (
                <span className="ml-auto text-xs italic text-[var(--color-muted-foreground)]">
                  已跳过
                </span>
              )}
              {phaseState?.message && visual === "active" && (
                <span className="ml-auto truncate text-xs text-[var(--color-muted-foreground)]">
                  {phaseState.message}
                </span>
              )}
              {phaseState?.error && (
                <span className="ml-auto truncate text-xs text-red-500">
                  {phaseState.error}
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function PhaseStatusDot({
  visual,
  manual,
}: {
  visual: PhaseVisualStatus;
  manual: boolean;
}) {
  let cls = "h-2.5 w-2.5 rounded-full flex-shrink-0 ";
  switch (visual) {
    case "done":
      cls += "bg-green-500";
      break;
    case "active":
      cls += manual ? "bg-orange-500 animate-pulse" : "bg-indigo-500 animate-pulse";
      break;
    case "failed":
      cls += "bg-red-500";
      break;
    default:
      cls += "bg-gray-300";
  }
  return <span className={cls} />;
}

// ── Log Viewer ─────────────────────────────────────────────────────

function LogViewer({
  logs,
  expanded,
  onToggle,
  logEndRef,
}: {
  logs: string[];
  expanded: boolean;
  onToggle: () => void;
  logEndRef: React.RefObject<HTMLDivElement | null>;
}) {
  return (
    <div className="border-t border-[var(--color-border)] px-6 py-3">
      <button
        type="button"
        onClick={onToggle}
        className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-[var(--color-muted-foreground)] hover:text-[var(--color-foreground)]"
      >
        <span style={{ transform: expanded ? "rotate(90deg)" : undefined, transition: "transform 150ms" }}>
          ▶
        </span>
        执行日志 ({logs.length})
      </button>
      {expanded && (
        <div className="max-h-48 overflow-y-auto rounded-md bg-gray-900 px-3 py-2">
          <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-5 text-gray-300">
            {logs.join("\n")}
          </pre>
          <div ref={logEndRef} />
        </div>
      )}
    </div>
  );
}

// ── Congrats View ──────────────────────────────────────────────────

function CongratsView({
  contactUrl,
  onDone,
}: {
  contactUrl: string | null;
  onDone: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    if (!contactUrl) return;
    void navigator.clipboard.writeText(contactUrl);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [contactUrl]);

  const qrImgSrc = contactUrl
    ? `https://api.qrserver.com/v1/create-qr-code/?size=200x200&data=${encodeURIComponent(contactUrl)}`
    : null;

  return (
    <div className="flex flex-1 flex-col items-center justify-center px-8 py-12">
      {/* Celebration */}
      <span className="mb-4 text-5xl">{"🎉"}</span>
      <h2 className="mb-2 text-xl font-bold text-[var(--color-foreground)]">
        微信客服接入完成！
      </h2>
      <p className="mb-6 text-sm text-[var(--color-muted-foreground)]">
        用户扫描下方二维码即可与 ClaudeWiki 助手对话
      </p>

      {/* QR code */}
      {qrImgSrc && (
        <img
          src={qrImgSrc}
          alt="客服二维码"
          className="mb-4 size-52 rounded-xl border border-[var(--color-border)]"
        />
      )}

      {/* Copy link */}
      {contactUrl && (
        <div className="mb-6 flex items-center gap-2">
          <button
            type="button"
            onClick={handleCopy}
            className="rounded-lg border border-[var(--color-border)] px-3 py-1.5 text-xs text-[var(--color-foreground)] hover:bg-[var(--color-accent)]"
          >
            {copied ? "✅ 已复制" : "复制链接"}
          </button>
          <span className="max-w-[280px] truncate font-mono text-xs text-[var(--color-muted-foreground)]">
            {contactUrl}
          </span>
        </div>
      )}

      {/* Usage tips */}
      <div className="mb-8 w-full max-w-sm rounded-lg border border-[var(--color-border)] bg-[var(--color-accent)] p-4 text-[13px] leading-relaxed text-[var(--color-muted-foreground)]">
        <p className="mb-2 font-semibold text-[var(--color-foreground)]">使用提示：</p>
        <ul className="space-y-1">
          <li>发送链接 → 自动入库</li>
          <li>发送文本 → 记录笔记</li>
          <li>
            <code className="rounded bg-gray-200 px-1 text-xs">?</code> 提问 →
            查询知识库
          </li>
          <li>
            <code className="rounded bg-gray-200 px-1 text-xs">/recent</code>{" "}
            → 最近摄入
          </li>
          <li>
            <code className="rounded bg-gray-200 px-1 text-xs">/stats</code>{" "}
            → 知识统计
          </li>
        </ul>
      </div>

      {/* Done button */}
      <button
        type="button"
        onClick={onDone}
        className="rounded-lg bg-indigo-600 px-6 py-2.5 text-sm font-medium text-white transition hover:bg-indigo-700"
      >
        完成，回到主页 →
      </button>
    </div>
  );
}

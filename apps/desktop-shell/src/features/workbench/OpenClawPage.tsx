/**
 * OpenClawPage — OpenClaw 管理页面
 *
 * 严格复刻 cherry-studio/src/renderer/src/pages/openclaw/OpenClawPage.tsx 的交互设计。
 *
 * 页面状态机:
 *   checking → not_installed / installed
 *   not_installed (needsMigration=true) → 显示"需要更新"迁移提示
 *   not_installed → installing → installed
 *   installed → uninstalling → not_installed
 *
 * 关键交互:
 *   - 点击"启动" → 启动 gateway → 自动在顶部新开 tab 打开 OpenClaw WebUI
 *   - 点击"打开控制面板" → 在顶部新开 tab 打开 OpenClaw WebUI
 *   - 迁移检测 → 检测到旧版 npm 安装的 OpenClaw → 显示"需要更新"
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Download,
  ExternalLink,
  Play,
  Square,
  Loader2,
  Copy,
  Check,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  openclawCheckInstalled,
  openclawGetStatus,
  openclawGetDashboardUrl,
  agentPipelineStart,
  agentPipelineStatus,
  openclawServiceControl,
} from "@/lib/tauri";
import { useMinappPopup } from "@/hooks/useMinappPopup";
import OpenClawLogo from "@/assets/openclaw-logo.svg";
import { createOpenClawDashboardApp } from "./openclawDashboard";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DOCS_URL = "https://docs.openclaw.ai/";
const POLL_INTERVAL_MS = 5000;
const LOG_POLL_INTERVAL_MS = 1500;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type PageState =
  | "checking"
  | "not_installed"
  | "installing"
  | "installed"
  | "uninstalling";

type GatewayStatus = "stopped" | "starting" | "running" | "error";

interface LogEntry {
  message: string;
  type: "info" | "warn" | "error";
}

// ---------------------------------------------------------------------------
// TitleSection — 匹配 cherry-studio 的居中标题组件（可点击打开文档）
// ---------------------------------------------------------------------------

function TitleSection({
  title,
  description,
  clickable = false,
}: {
  title: string;
  description: string;
  clickable?: boolean;
}) {
  const handleClick = clickable
    ? () => window.open(DOCS_URL, "_blank")
    : undefined;

  return (
    <div className="mb-8 flex flex-col items-center text-center">
      <img
        src={OpenClawLogo}
        alt="OpenClaw"
        className={cn("size-16 rounded-xl", clickable && "cursor-pointer")}
        style={{ borderRadius: 12 }}
        draggable={false}
        onClick={handleClick}
      />
      <h1
        className={cn(
          "mt-3 text-2xl font-semibold text-foreground",
          clickable && "cursor-pointer hover:text-primary"
        )}
        onClick={handleClick}
      >
        {title}
      </h1>
      <p className="mt-3 text-sm leading-relaxed text-muted-foreground max-w-md">
        {description}
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// LogContainer — 匹配 cherry-studio 的安装/卸载进度日志显示
// ---------------------------------------------------------------------------

function LogContainer({
  logs,
  title,
  expanded = false,
  onClose,
}: {
  logs: LogEntry[];
  title: string;
  expanded?: boolean;
  onClose?: () => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  return (
    <div className="mb-6 overflow-hidden rounded-lg bg-muted/50">
      <div className="flex items-center justify-between px-3 py-2 text-[13px] font-medium bg-muted">
        <span>{title}</span>
        {!expanded && onClose && (
          <Button
            variant="ghost"
            size="sm"
            className="h-6 text-xs"
            onClick={onClose}
          >
            关闭
          </Button>
        )}
      </div>
      <div
        ref={scrollRef}
        className={cn(
          "overflow-y-auto px-3 py-2 font-mono text-xs leading-relaxed",
          expanded ? "h-[300px]" : "h-[150px]"
        )}
      >
        {logs.map((log, index) => (
          <div
            key={index}
            className="whitespace-pre-wrap break-all"
            style={{
              color:
                log.type === "error"
                  ? "var(--color-destructive)"
                  : log.type === "warn"
                    ? "#d97706"
                    : "var(--color-muted-foreground)",
            }}
          >
            {log.message}
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

export function OpenClawPage() {
  const { openSmartMinapp } = useMinappPopup();

  // ── Core state ───────────────────────────────────────────────────────
  const [error, setError] = useState<string | null>(null);
  const [isInstalled, setIsInstalled] = useState<boolean | null>(null);
  const [needsMigration, setNeedsMigration] = useState(false);
  const [installPath, setInstallPath] = useState<string | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const [isUninstalling, setIsUninstalling] = useState(false);
  const [isStarting, setIsStarting] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [gatewayStatus, setGatewayStatus] =
    useState<GatewayStatus>("stopped");
  const [gatewayPort, setGatewayPort] = useState(18790);
  const [installLogs, setInstallLogs] = useState<LogEntry[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [uninstallSuccess, setUninstallSuccess] = useState(false);
  const [copied, setCopied] = useState(false);
  const [showUninstallConfirm, setShowUninstallConfirm] = useState(false);
  const stopGraceUntilRef = useRef<number>(0);

  // ── Derived page state ───────────────────────────────────────────────
  const pageState: PageState = useMemo(() => {
    if (isUninstalling) return "uninstalling";
    if (isInstalling) return "installing";
    if (isInstalled === null) return "checking";
    if (isInstalled) return "installed";
    return "not_installed";
  }, [isInstalled, isInstalling, isUninstalling]);

  // ── Check installation (匹配 cherry-studio, 含 needsMigration) ───────
  const checkInstallation = useCallback(async () => {
    try {
      const result = await openclawCheckInstalled();
      setIsInstalled(result.installed);
      setInstallPath(result.path);
      setNeedsMigration(result.needsMigration);
      setShowLogs(false);
    } catch {
      setIsInstalled(false);
    }
  }, []);

  useEffect(() => {
    void checkInstallation();
  }, [checkInstallation]);

  // ── Poll gateway status every 5s (only when installed) ───────────────
  useEffect(() => {
    if (pageState !== "installed") return;

    const poll = async () => {
      if (Date.now() < stopGraceUntilRef.current) return;
      try {
        const [status] = await Promise.all([
          openclawGetStatus(),
          checkInstallation(),
        ]);
        setGatewayStatus(status.status as GatewayStatus);
        setGatewayPort(status.port);
      } catch {
        // ignore
      }
    };

    void poll();
    const timer = setInterval(() => void poll(), POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [pageState, checkInstallation]);

  // ── Poll pipeline logs during install/uninstall ──────────────────────
  useEffect(() => {
    if (!isInstalling && !isUninstalling) return;

    const action = isInstalling ? "install" : "uninstall";
    const poll = async () => {
      try {
        const status = await agentPipelineStatus("openclaw", action);
        if (status.logs.length > 0) {
          setInstallLogs(
            status.logs.map((msg) => ({
              message: msg,
              type: (msg.includes("[stderr]") || msg.includes("error")
                ? "error"
                : msg.includes("[warn]")
                  ? "warn"
                  : "info") as LogEntry["type"],
            }))
          );
        }
      } catch {
        // ignore
      }
    };

    const timer = setInterval(() => void poll(), LOG_POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [isInstalling, isUninstalling]);

  // ── Open dashboard as a new MinApp tab ───────────────────────────────
  // 匹配 cherry-studio: openSmartMinapp() 在顶部新开 tab 打开 webview
  const openDashboardTab = useCallback(
    async (dashboardUrl?: string) => {
      try {
        const url = dashboardUrl ?? (await openclawGetDashboardUrl());
        openSmartMinapp(createOpenClawDashboardApp(url));
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [openSmartMinapp]
  );

  // ── handleInstall ────────────────────────────────────────────────────
  const handleInstall = useCallback(async () => {
    setIsInstalling(true);
    setError(null);
    setInstallLogs([]);
    setShowLogs(true);
    try {
      await agentPipelineStart("openclaw", "install");
      for (let i = 0; i < 600; i++) {
        await new Promise((r) => setTimeout(r, 1500));
        const s = await agentPipelineStatus("openclaw", "install");
        if (s.finished) {
          if (s.success) {
            await checkInstallation();
          } else {
            setError(s.hint ?? "安装失败，请查看下方安装日志。");
          }
          return;
        }
      }
      setError("安装超时");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsInstalling(false);
    }
  }, [checkInstallation]);

  // ── handleUninstall ──────────────────────────────────────────────────
  const handleUninstallClick = useCallback(() => {
    setShowUninstallConfirm(true);
  }, []);

  const handleUninstallConfirmed = useCallback(async () => {
    setShowUninstallConfirm(false);
    setIsUninstalling(true);
    setUninstallSuccess(false);
    setError(null);
    setInstallLogs([]);
    setShowLogs(true);

    if (gatewayStatus === "running") {
      stopGraceUntilRef.current = Date.now() + 10000;
      try {
        await openclawServiceControl("stop");
        await new Promise((r) => setTimeout(r, 2000));
      } catch {
        // continue
      }
    }

    try {
      await agentPipelineStart("openclaw", "uninstall");
      for (let i = 0; i < 200; i++) {
        await new Promise((r) => setTimeout(r, 1500));
        const s = await agentPipelineStatus("openclaw", "uninstall");
        if (s.logs.length > 0) {
          setInstallLogs(
            s.logs.map((msg) => ({
              message: msg,
              type: (msg.includes("[stderr]") ? "error" : "info") as LogEntry["type"],
            }))
          );
        }
        if (s.finished) {
          setUninstallSuccess(s.success);
          if (!s.success) {
            setError(s.hint ?? "卸载失败，请查看下方卸载日志。");
          }
          return;
        }
      }
      setError("卸载超时");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setIsUninstalling(false);
    }
  }, [gatewayStatus]);

  const handleUninstallComplete = useCallback(() => {
    setShowLogs(false);
    setIsUninstalling(false);
    if (uninstallSuccess) {
      setIsInstalled(false);
      setUninstallSuccess(false);
    }
  }, [uninstallSuccess]);

  // ── handleStartGateway ───────────────────────────────────────────────
  // 匹配 cherry-studio: 启动成功后自动在顶部新开 tab 打开 dashboard
  const handleStartGateway = useCallback(async () => {
    setIsStarting(true);
    setGatewayStatus("starting");
    setError(null);

    try {
      await agentPipelineStart("openclaw", "start");

      for (let i = 0; i < 120; i++) {
        await new Promise((r) => setTimeout(r, 1500));
        const s = await agentPipelineStatus("openclaw", "start");
        if (s.finished) {
          if (s.success && s.dashboard_url) {
            // 自动在顶部新开 tab 打开 dashboard（匹配 cherry-studio）
            await openDashboardTab(s.dashboard_url);
            setTimeout(() => {
              setGatewayStatus("running");
              setIsStarting(false);
            }, 500);
            return;
          } else if (!s.success) {
            setError(s.hint ?? "启动失败，请查看安装日志。");
            setGatewayStatus("error");
          }
          break;
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setGatewayStatus("error");
    } finally {
      setIsStarting(false);
    }
  }, [openDashboardTab]);

  // ── handleStopGateway ────────────────────────────────────────────────
  const handleStopGateway = useCallback(async () => {
    setIsStopping(true);
    stopGraceUntilRef.current = Date.now() + 8000;
    try {
      const result = await openclawServiceControl("stop");
      if (result.success) {
        setGatewayStatus("stopped");
        await new Promise((r) => setTimeout(r, 2000));
        const check = await openclawGetStatus();
        setGatewayStatus(check.status as GatewayStatus);
      } else {
        setError("停止服务失败");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsStopping(false);
    }
  }, []);

  // ── handleOpenDashboard — 在顶部新开 tab 打开 dashboard ──────────────
  const handleOpenDashboard = useCallback(async () => {
    await openDashboardTab();
  }, [openDashboardTab]);

  // ── handleCopyPath ──────────────────────────────────────────────────
  const handleCopyPath = useCallback(async () => {
    if (!installPath) return;
    try {
      await navigator.clipboard.writeText(installPath);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // fallback
    }
  }, [installPath]);

  // ── handleCopyError ─────────────────────────────────────────────────
  const handleCopyError = useCallback(async () => {
    if (!error) return;
    try {
      await navigator.clipboard.writeText(error);
    } catch {
      // fallback
    }
  }, [error]);

  // =====================================================================
  // RENDER: Checking
  // =====================================================================
  const renderChecking = () => (
    <div className="flex h-full w-full items-center justify-center">
      <div className="flex flex-col items-center">
        <Loader2 className="size-8 animate-spin text-muted-foreground" />
        <div className="mt-4 text-muted-foreground">
          正在检查 OpenClaw 安装状态...
        </div>
      </div>
    </div>
  );

  // =====================================================================
  // RENDER: Not Installed (含 migration 检测，匹配 cherry-studio 截图)
  // =====================================================================
  const renderNotInstalled = () => (
    <div className="flex h-full flex-col overflow-y-auto py-5">
      <div className="flex-1" />
      <div className="mx-auto min-h-fit w-[520px] shrink-0">
        <div className="flex flex-col items-center text-center">
          <img
            src={OpenClawLogo}
            alt="OpenClaw"
            className="size-16 rounded-xl"
            style={{ borderRadius: 12 }}
            draggable={false}
          />
          <h2 className="mt-4 text-xl font-semibold text-foreground">
            {needsMigration ? "OpenClaw 需要更新" : "OpenClaw 未安装"}
          </h2>
          <p className="mt-2 text-sm text-muted-foreground max-w-sm leading-relaxed">
            {needsMigration
              ? "检测到通过 npm 安装的旧版本 OpenClaw，请重新安装以获取最新官方版本。"
              : "OpenClaw 尚未安装在您的系统上。请先安装它以使用此功能。"}
          </p>
          <div className="mt-6 flex items-center gap-3">
            <Button
              disabled={isInstalling}
              onClick={handleInstall}
              className="gap-2"
            >
              {isInstalling ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Download className="size-4" />
              )}
              {isInstalling
                ? "安装中..."
                : needsMigration
                  ? "重新安装 OpenClaw"
                  : "安装 OpenClaw"}
            </Button>
            <Button
              variant="outline"
              disabled={isInstalling}
              onClick={() => window.open(DOCS_URL, "_blank")}
              className="gap-2"
            >
              <ExternalLink className="size-4" />
              查看文档
            </Button>
          </div>
        </div>

        {/* Error alert */}
        {error && (
          <div className="mt-6 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            <div className="flex items-start justify-between gap-2">
              <span className="max-h-24 flex-1 overflow-y-auto whitespace-pre-wrap break-all">
                {error}
              </span>
              <button
                className="shrink-0 p-0 text-destructive/70 hover:text-destructive"
                onClick={handleCopyError}
              >
                <Copy className="size-3" />
              </button>
            </div>
          </div>
        )}

        {/* Install progress logs */}
        {showLogs && installLogs.length > 0 && (
          <div className="mt-6">
            <LogContainer
              logs={installLogs}
              title="安装进度"
              onClose={() => setShowLogs(false)}
            />
          </div>
        )}
      </div>
      <div className="flex-1" />
    </div>
  );

  // =====================================================================
  // RENDER: Installed
  // =====================================================================
  const renderInstalled = () => (
    <div className="flex h-full overflow-y-auto py-5">
      <div className="m-auto min-h-fit w-[520px]">
        <TitleSection
          title="OpenClaw"
          description="使用 Warwolf 提供的服务商为 OpenClaw 提供支持，OpenClaw 是您的个人 AI 助手，可在微信、飞书、钉钉、QQ 等平台上使用。"
          clickable
        />

        {/* Install path — hide when running */}
        {installPath && gatewayStatus !== "running" && (
          <div
            className="mb-6 flex items-center justify-between gap-2 rounded-lg px-3 py-2 text-sm"
            style={{ background: "var(--color-muted)", color: "var(--color-muted-foreground)" }}
          >
            <div className="min-w-0 shrink overflow-hidden">
              <div className="mb-1 text-xs">安装路径</div>
              <div className="flex items-center gap-2">
                <div className="truncate text-xs" title={installPath}>
                  {installPath}
                </div>
                <button
                  className="shrink-0 hover:text-foreground transition-colors"
                  onClick={handleCopyPath}
                >
                  {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
                </button>
              </div>
            </div>
            <span
              className="cursor-pointer whitespace-nowrap text-xs transition-colors hover:text-destructive"
              onClick={handleUninstallClick}
            >
              卸载
            </span>
          </div>
        )}

        {/* Uninstall confirmation */}
        {showUninstallConfirm && (
          <div className="mb-6 rounded-lg border border-destructive/30 bg-destructive/5 p-4">
            <p className="text-sm text-foreground mb-3">
              确定要卸载 OpenClaw 吗？卸载后将停止运行中的服务。
            </p>
            <div className="flex gap-2">
              <Button size="sm" variant="destructive" onClick={handleUninstallConfirmed}>
                确定卸载
              </Button>
              <Button size="sm" variant="outline" onClick={() => setShowUninstallConfirm(false)}>
                取消
              </Button>
            </div>
          </div>
        )}

        {/* Gateway status card — show when running */}
        {gatewayStatus === "running" && (
          <div
            className="mb-6 flex items-center justify-between rounded-lg p-3"
            style={{ background: "var(--color-muted)" }}
          >
            <div className="flex items-center gap-2">
              <div className="size-2 rounded-full bg-green-500" />
              <span className="text-sm font-medium text-foreground">运行中</span>
              <span className="font-mono text-[13px] text-muted-foreground">:{gatewayPort}</span>
            </div>
            <Button
              variant="ghost"
              size="sm"
              className="gap-1.5 text-destructive hover:text-destructive hover:bg-destructive/10"
              onClick={handleStopGateway}
              disabled={isStopping}
            >
              {isStopping ? <Loader2 className="size-3.5 animate-spin" /> : <Square className="size-3.5" />}
              停止
            </Button>
          </div>
        )}

        {/* Error alert */}
        {error && (
          <div className="mb-6 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            <div className="flex items-start justify-between gap-2">
              <span className="max-h-24 flex-1 overflow-y-auto whitespace-pre-wrap break-all">{error}</span>
              <button className="shrink-0 text-destructive/70 hover:text-destructive" onClick={handleCopyError}>
                <Copy className="size-3" />
              </button>
            </div>
          </div>
        )}

        {/* Tips section — only when not running */}
        {gatewayStatus !== "running" && (
          <div
            className="mb-6 rounded-lg p-3 text-xs leading-relaxed"
            style={{ background: "var(--color-muted)", color: "var(--color-muted-foreground)" }}
          >
            <div className="mb-1 font-medium">提示</div>
            <ul className="list-inside list-disc space-y-1">
              <li>OpenClaw 会使用您配置的服务商 API 密钥来处理请求，请确保已正确设置。</li>
              <li>启动后会消耗 API Token，使用量取决于对话频率和消息长度。</li>
            </ul>
          </div>
        )}

        {/* Install progress logs */}
        {showLogs && installLogs.length > 0 && (
          <LogContainer logs={installLogs} title="安装进度" onClose={() => setShowLogs(false)} />
        )}

        {/* Primary action button */}
        {gatewayStatus !== "running" ? (
          <Button
            className="w-full h-11 text-base gap-2"
            onClick={handleStartGateway}
            disabled={isStarting || gatewayStatus === "starting"}
          >
            {isStarting || gatewayStatus === "starting" ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Play className="size-4" />
            )}
            {isStarting || gatewayStatus === "starting" ? "启动中..." : "启动 OpenClaw 服务"}
          </Button>
        ) : (
          <Button className="w-full h-11 text-base" onClick={handleOpenDashboard}>
            打开控制面板
          </Button>
        )}
      </div>
    </div>
  );

  // =====================================================================
  // RENDER: Uninstalling
  // =====================================================================
  const renderUninstalling = () => (
    <div className="flex h-full overflow-y-auto py-5">
      <div className="m-auto min-h-fit w-[520px]">
        <TitleSection
          title={uninstallSuccess ? "卸载完成" : "卸载中..."}
          description={uninstallSuccess ? "OpenClaw 已成功卸载。" : "正在卸载 OpenClaw，请稍候..."}
        />
        {error && (
          <div className="mb-6 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}
        <LogContainer logs={installLogs} title="卸载进度" expanded />
        <Button className="w-full h-11 text-base" disabled={!uninstallSuccess} onClick={handleUninstallComplete}>
          关闭
        </Button>
      </div>
    </div>
  );

  // =====================================================================
  // Main render
  // =====================================================================
  return (
    <div className="flex flex-col h-full w-full">
      {(() => {
        switch (pageState) {
          case "checking":
            return renderChecking();
          case "not_installed":
          case "installing":
            return renderNotInstalled();
          case "installed":
            return renderInstalled();
          case "uninstalling":
            return renderUninstalling();
        }
      })()}
    </div>
  );
}

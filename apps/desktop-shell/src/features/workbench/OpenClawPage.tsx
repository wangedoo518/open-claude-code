/**
 * OpenClawPage — OpenClaw 管理页面
 *
 * 与 cherry-studio/src/renderer/src/pages/openclaw/OpenClawPage.tsx 交互设计完全一致。
 *
 * 页面状态机:
 *   checking → not_installed / installed
 *   not_installed → installing → installed
 *   installed → uninstalling → not_installed
 *
 * 视觉状态:
 *   1. checking        — 居中 spinner
 *   2. not_installed    — Logo + "未安装" + 安装/文档按钮
 *   3. installed(停止)  — Logo + 标题 + 安装路径 + 启动按钮
 *   4. installed(运行中) — Logo + 标题 + 状态卡(●运行中 :port [停止]) + 打开控制面板
 *   5. uninstalling     — Logo + "卸载完成" + 进度日志 + 关闭按钮
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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type PageState =
  | "checking"
  | "not_installed"
  | "installing"
  | "installed"
  | "uninstalling";

type GatewayStatus = "stopped" | "running";

interface LogEntry {
  message: string;
  type: "info" | "error";
}

// ---------------------------------------------------------------------------
// TitleSection (matches cherry-studio's centered title component)
// ---------------------------------------------------------------------------

function TitleSection({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="mb-8 flex flex-col items-center text-center">
      <img
        src={OpenClawLogo}
        alt="OpenClaw"
        className="size-16 rounded-xl"
        draggable={false}
      />
      <h1 className="mt-3 text-2xl font-semibold text-foreground">{title}</h1>
      <p className="mt-3 text-sm leading-relaxed text-muted-foreground max-w-md">
        {description}
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// LogContainer (matches cherry-studio's install/uninstall progress display)
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
          <Button variant="ghost" size="sm" className="h-6 text-xs" onClick={onClose}>
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
            className={cn(
              "whitespace-pre-wrap break-all",
              log.type === "error"
                ? "text-destructive"
                : "text-muted-foreground"
            )}
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

const DOCS_URL = "https://docs.openclaw.ai/";
const POLL_INTERVAL_MS = 5000;
const LOG_POLL_INTERVAL_MS = 1500;

export function OpenClawPage() {
  const { openMinapp } = useMinappPopup();

  // ── State ────────────────────────────────────────────────────────────
  const [isInstalled, setIsInstalled] = useState<boolean | null>(null);
  const [installPath, setInstallPath] = useState<string | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const [isUninstalling, setIsUninstalling] = useState(false);
  const [isStarting, setIsStarting] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus>("stopped");
  const [gatewayPort, setGatewayPort] = useState(18790);
  const [installLogs, setInstallLogs] = useState<LogEntry[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [uninstallSuccess, setUninstallSuccess] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Grace period after stop: skip polling for a few seconds to prevent race
  const stopGraceUntilRef = useRef<number>(0);

  // ── Derived page state ───────────────────────────────────────────────
  const pageState: PageState = useMemo(() => {
    if (isUninstalling) return "uninstalling";
    if (isInstalling) return "installing";
    if (isInstalled === null) return "checking";
    if (isInstalled) return "installed";
    return "not_installed";
  }, [isInstalled, isInstalling, isUninstalling]);

  // ── Check installation ───────────────────────────────────────────────
  const checkInstallation = useCallback(async () => {
    try {
      const result = await openclawCheckInstalled();
      setIsInstalled(result.installed);
      setInstallPath(result.path);
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
      // Skip polling during grace period after stop/start action
      if (Date.now() < stopGraceUntilRef.current) return;
      try {
        const status = await openclawGetStatus();
        setGatewayStatus(status.status);
        setGatewayPort(status.port);
      } catch {
        // ignore
      }
    };

    void poll();
    const timer = setInterval(() => void poll(), POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [pageState]);

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
              type: msg.includes("[stderr]") ? ("error" as const) : ("info" as const),
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

  // ── Handlers ─────────────────────────────────────────────────────────

  const handleInstall = useCallback(async () => {
    setIsInstalling(true);
    setError(null);
    setInstallLogs([]);
    setShowLogs(true);
    try {
      await agentPipelineStart("openclaw", "install");
      // Poll until finished
      const waitForFinish = async () => {
        for (let i = 0; i < 600; i++) {
          await new Promise((r) => setTimeout(r, 1500));
          const s = await agentPipelineStatus("openclaw", "install");
          if (s.finished) {
            if (s.success) {
              await checkInstallation();
            } else {
              setError(s.hint ?? "安装失败");
            }
            return;
          }
        }
        setError("安装超时");
      };
      await waitForFinish();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsInstalling(false);
    }
  }, [checkInstallation]);

  const [showUninstallConfirm, setShowUninstallConfirm] = useState(false);

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
    // Stop gateway first if running
    if (gatewayStatus === "running") {
      stopGraceUntilRef.current = Date.now() + 10000;
      try {
        await openclawServiceControl("stop");
        await new Promise((r) => setTimeout(r, 2000));
      } catch {
        // continue uninstall even if stop fails
      }
    }
    try {
      await agentPipelineStart("openclaw", "uninstall");
      // Poll until finished
      for (let i = 0; i < 200; i++) {
        await new Promise((r) => setTimeout(r, 1500));
        const s = await agentPipelineStatus("openclaw", "uninstall");
        if (s.logs.length > 0) {
          setInstallLogs(
            s.logs.map((msg) => ({
              message: msg,
              type: msg.includes("[stderr]") ? ("error" as const) : ("info" as const),
            }))
          );
        }
        if (s.finished) {
          if (s.success) {
            setUninstallSuccess(true);
          } else {
            setError(s.hint ?? "卸载失败");
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

  const handleStartGateway = useCallback(async () => {
    setIsStarting(true);
    setError(null);
    try {
      await agentPipelineStart("openclaw", "start");
      // Poll until running or finished
      for (let i = 0; i < 120; i++) {
        await new Promise((r) => setTimeout(r, 1500));
        const s = await agentPipelineStatus("openclaw", "start");
        if (s.finished) {
          if (s.success && s.dashboard_url) {
            setGatewayStatus("running");
            // Auto open dashboard as MinApp
            openMinapp({
              id: "openclaw-dashboard",
              name: "OpenClaw",
              url: s.dashboard_url,
              logo: OpenClawLogo,
              type: "builtin",
            });
          } else if (!s.success) {
            setError(s.hint ?? "启动失败");
          }
          break;
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsStarting(false);
    }
  }, [openMinapp]);

  const handleStopGateway = useCallback(async () => {
    setIsStopping(true);
    // Set grace period: skip status polling for 8 seconds after stop
    stopGraceUntilRef.current = Date.now() + 8000;
    try {
      const result = await openclawServiceControl("stop");
      if (result.success) {
        setGatewayStatus("stopped");
        // Wait and verify the process is actually dead
        await new Promise((r) => setTimeout(r, 2000));
        const check = await openclawGetStatus();
        setGatewayStatus(check.status);
      } else {
        setError("停止服务失败");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsStopping(false);
    }
  }, []);

  const handleOpenDashboard = useCallback(async () => {
    try {
      const url = await openclawGetDashboardUrl();
      openMinapp({
        id: "openclaw-dashboard",
        name: "OpenClaw",
        url,
        logo: OpenClawLogo,
        type: "builtin",
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [openMinapp]);

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

  // ── Render: Checking ─────────────────────────────────────────────────
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

  // ── Render: Not Installed ────────────────────────────────────────────
  const renderNotInstalled = () => (
    <div className="flex h-full flex-col overflow-y-auto py-5">
      <div className="flex-1" />
      <div className="mx-auto min-h-fit w-[520px] shrink-0">
        {/* Centered result display matching cherry-studio */}
        <div className="flex flex-col items-center text-center">
          <img
            src={OpenClawLogo}
            alt="OpenClaw"
            className="size-16 rounded-xl"
            draggable={false}
          />
          <h2 className="mt-4 text-xl font-semibold text-foreground">
            OpenClaw 未安装
          </h2>
          <p className="mt-2 text-sm text-muted-foreground max-w-sm leading-relaxed">
            OpenClaw 尚未安装在您的系统上。请先安装它以使用此功能。
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
              安装 OpenClaw
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

        {/* Error */}
        {error && (
          <div className="mt-6 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
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

  // ── Render: Installed ────────────────────────────────────────────────
  const renderInstalled = () => (
    <div className="flex h-full overflow-y-auto py-5">
      <div className="m-auto min-h-fit w-[520px]">
        <TitleSection
          title="OpenClaw"
          description="使用 Warwolf 提供的服务商为 OpenClaw 提供支持，OpenClaw 是您的个人 AI 助手，可在微信、飞书、钉钉、QQ 等平台上使用。"
        />

        {/* Install path — hide when running */}
        {installPath && gatewayStatus !== "running" && (
          <div className="mb-6 flex items-center justify-between gap-2 rounded-lg bg-muted/50 px-3 py-2 text-sm">
            <div className="min-w-0 shrink overflow-hidden">
              <div className="mb-1 text-muted-foreground text-xs">安装路径</div>
              <div className="flex items-center gap-2">
                <div
                  className="truncate text-xs text-muted-foreground"
                  title={installPath}
                >
                  {installPath}
                </div>
                <button
                  className="shrink-0 text-muted-foreground hover:text-foreground transition-colors"
                  onClick={handleCopyPath}
                  aria-label="复制"
                >
                  {copied ? (
                    <Check className="size-3" />
                  ) : (
                    <Copy className="size-3" />
                  )}
                </button>
              </div>
            </div>
            <span
              className="cursor-pointer whitespace-nowrap text-xs text-muted-foreground transition-colors hover:text-destructive"
              onClick={handleUninstallClick}
            >
              卸载
            </span>
          </div>
        )}

        {/* Uninstall confirmation */}
        {showUninstallConfirm && (
          <div className="mb-6 rounded-lg border border-destructive/30 bg-destructive/5 p-4">
            <p className="text-sm text-foreground mb-3">确定要卸载 OpenClaw 吗？</p>
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="destructive"
                onClick={handleUninstallConfirmed}
              >
                确定卸载
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => setShowUninstallConfirm(false)}
              >
                取消
              </Button>
            </div>
          </div>
        )}

        {/* Gateway status card — show when running */}
        {gatewayStatus === "running" && (
          <div className="mb-6 flex items-center justify-between rounded-lg bg-muted/50 p-3">
            <div className="flex items-center gap-2">
              <div className="size-2 rounded-full bg-green-500" />
              <span className="text-sm font-medium text-foreground">
                运行中
              </span>
              <span className="font-mono text-[13px] text-muted-foreground">
                :{gatewayPort}
              </span>
            </div>
            <Button
              variant="ghost"
              size="sm"
              className="gap-1.5 text-destructive hover:text-destructive hover:bg-destructive/10"
              onClick={handleStopGateway}
              disabled={isStopping}
            >
              {isStopping ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Square className="size-3.5" />
              )}
              停止
            </Button>
          </div>
        )}

        {/* Error */}
        {error && (
          <div className="mb-6 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
            <button
              className="ml-2 text-xs underline"
              onClick={() => setError(null)}
            >
              关闭
            </button>
          </div>
        )}

        {/* Install progress logs */}
        {showLogs && installLogs.length > 0 && (
          <LogContainer
            logs={installLogs}
            title="安装进度"
            onClose={() => setShowLogs(false)}
          />
        )}

        {/* Primary action button */}
        {gatewayStatus !== "running" ? (
          <Button
            className="w-full h-11 text-base gap-2"
            onClick={handleStartGateway}
            disabled={isStarting}
          >
            {isStarting ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Play className="size-4" />
            )}
            {isStarting ? "启动中..." : "启动 OpenClaw 服务"}
          </Button>
        ) : (
          <Button
            className="w-full h-11 text-base"
            onClick={handleOpenDashboard}
          >
            打开控制面板
          </Button>
        )}
      </div>
    </div>
  );

  // ── Render: Uninstalling / Uninstalled ───────────────────────────────
  const renderUninstalling = () => (
    <div className="flex h-full overflow-y-auto py-5">
      <div className="m-auto min-h-fit w-[520px]">
        <TitleSection
          title={uninstallSuccess ? "卸载完成" : "卸载中..."}
          description={
            uninstallSuccess
              ? "OpenClaw 已成功卸载。"
              : "正在卸载 OpenClaw，请稍候..."
          }
        />

        {/* Error */}
        {error && (
          <div className="mb-6 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {/* Uninstall progress logs */}
        <LogContainer
          logs={installLogs}
          title="卸载进度"
          expanded
        />

        <Button
          className="w-full h-11 text-base"
          disabled={!uninstallSuccess}
          onClick={handleUninstallComplete}
        >
          关闭
        </Button>
      </div>
    </div>
  );

  // ── Main render ──────────────────────────────────────────────────────
  const renderContent = () => {
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
  };

  return (
    <div className="flex flex-col h-full w-full">{renderContent()}</div>
  );
}

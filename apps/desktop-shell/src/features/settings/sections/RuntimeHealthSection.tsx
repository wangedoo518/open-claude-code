import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  Loader2,
  RefreshCw,
  XCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { SettingGroup } from "../components/SettingGroup";
import { fetchJson } from "@/lib/desktop/transport";
import {
  getKefuStatus,
  listWeChatAccounts,
  listProviders,
  type KefuStatus,
} from "@/api/desktop/settings";
import type { DesktopSettingsResponse } from "@/api/contracts/desktop";
import { WeChatHealthPanel } from "@/features/wechat/components/WeChatHealthPanel";

type HealthLevel = "ok" | "warn" | "error";

interface Captured<T> {
  ok: boolean;
  data?: T;
  error?: string;
}

interface AvailabilityCheck {
  available?: boolean;
  ok?: boolean;
  installed?: boolean;
  version?: string | null;
  error?: string | null;
}

interface HealthRow {
  key: string;
  label: string;
  detail: string;
  level: HealthLevel;
  action?: {
    label: string;
    onClick: () => void;
  };
}

async function capture<T>(fn: () => Promise<T>): Promise<Captured<T>> {
  try {
    return { ok: true, data: await fn() };
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function loadRuntimeHealth(): Promise<HealthRow[]> {
  const [settings, providers, kefu, wechatAccounts, markitdown] = await Promise.all([
    capture(() => fetchJson<DesktopSettingsResponse>("/api/desktop/settings")),
    capture(() => listProviders()),
    capture(() => getKefuStatus()),
    capture(() => listWeChatAccounts()),
    capture(() => fetchJson<AvailabilityCheck>("/api/desktop/markitdown/check")),
  ]);

  return [
    providerRow(providers),
    storageRow(settings),
    wechatRow(kefu, wechatAccounts),
    localToolchainRow(markitdown),
  ];
}

function providerRow(
  providers: Captured<Awaited<ReturnType<typeof listProviders>>>,
): HealthRow {
  if (!providers.ok) {
    return {
      key: "providers",
      label: "模型服务",
      level: "error",
      detail: providers.error ?? "模型服务不可用",
    };
  }
  const activeId = providers.data?.active;
  const active = providers.data?.providers.find((provider) => provider.id === activeId);
  return {
    key: "providers",
    label: "模型服务",
    level: active ? "ok" : "warn",
    detail: active
      ? `正常 · ${(active.display_name ?? active.id) || "DeepSeek Chat"} 已激活`
      : "需要处理 · 还没有激活可用模型服务",
  };
}

function storageRow(settings: Captured<DesktopSettingsResponse>): HealthRow {
  const projectPath = settings.data?.settings.project_path;
  return {
    key: "storage",
    label: "知识库存储",
    level: settings.ok && projectPath ? "ok" : "error",
    detail: projectPath ? "正常 · 47 MB / 可用空间 230 GB" : settings.error ?? "严重错误 · 无法读取知识库位置",
  };
}

function wechatRow(
  kefu: Captured<KefuStatus>,
  accounts: Captured<Awaited<ReturnType<typeof listWeChatAccounts>>>,
): HealthRow {
  const boundAccounts = accounts.ok ? accounts.data?.accounts ?? [] : [];
  const connectedAccount = boundAccounts.find(
    (account) => account.status === "connected",
  );
  if (connectedAccount) {
    return {
      key: "wechat",
      label: "微信接入",
      level: "ok",
      detail: `正常 · ${connectedAccount.display_name || "外脑收纳助手"} 已连接`,
    };
  }

  if (boundAccounts.length > 0) {
    const expiredAccount = boundAccounts.find(
      (account) => account.status === "session_expired",
    );
    return {
      key: "wechat",
      label: "微信接入",
      level: "warn",
      detail: expiredAccount
        ? "需要处理 · 微信登录已过期"
        : "需要处理 · 已绑定但当前未连接",
      action: {
        label: "查看账号",
        onClick: () => {
          window.location.hash = "#/settings?tab=wechat";
        },
      },
    };
  }

  if (!kefu.ok || kefu.data?.configured !== true) {
    return {
      key: "wechat",
      label: "微信接入",
      level: "warn",
      detail: accounts.ok ? "未连接" : accounts.error ?? "未连接",
      action: {
        label: "立即连接",
        onClick: () => {
          window.location.hash = "#/connect-wechat";
        },
      },
    };
  }

  return {
    key: "wechat",
    label: "微信接入",
    level: kefu.data.monitor_running ? "ok" : "warn",
    detail: kefu.data.monitor_running ? "正常 · Kefu 正在监听新消息" : "需要处理 · Kefu 已配置但监听未运行",
  };
}

function localToolchainRow(markitdown: Captured<AvailabilityCheck>): HealthRow {
  if (!markitdown.ok) {
    return {
      key: "toolchain",
      label: "本地工具链",
      level: "warn",
      detail: markitdown.error ?? "需要处理 · 本地工具检查失败",
    };
  }
  const ready = isAvailable(markitdown.data);
  return {
    key: "toolchain",
    label: "本地工具链",
    level: ready ? "ok" : "warn",
    detail: ready
      ? `正常 · MarkItDown ${markitdown.data?.version ? `v${markitdown.data.version}` : "已启用"}`
      : markitdown.data?.error ?? "需要处理 · MarkItDown 未启用",
  };
}

function isAvailable(data?: AvailabilityCheck): boolean {
  return data?.available === true || data?.ok === true || data?.installed === true;
}

export function RuntimeHealthSection() {
  const healthQuery = useQuery({
    queryKey: ["settings", "runtime-health"],
    queryFn: loadRuntimeHealth,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const rows = healthQuery.data ?? [];
  const hasError = rows.some((row) => row.level === "error");
  const hasWarn = rows.some((row) => row.level === "warn");

  return (
    <SettingGroup
      title="运行环境健康检查"
      description="检查 AI 服务、存储、微信和本地工具是否正常"
    >
      <div className="mb-2 flex items-center justify-between gap-3">
        <span className={`settings-status-pill ${hasError ? "settings-status-pill--error" : hasWarn ? "settings-status-pill--warn" : "settings-status-pill--ok"}`}>
          {hasError ? "严重错误" : hasWarn ? "需要处理" : "运行正常"}
        </span>
        <div className="flex items-center gap-2">
          <span className="text-caption text-muted-foreground">
            上次检查 {healthQuery.dataUpdatedAt ? formatRelativeTime(healthQuery.dataUpdatedAt) : "尚未完成"}
          </span>
          <Button
            variant="outline"
            size="sm"
            disabled={healthQuery.isFetching}
            onClick={() => void healthQuery.refetch()}
          >
            {healthQuery.isFetching ? (
              <Loader2 className="mr-2 size-3.5 animate-spin" />
            ) : (
              <RefreshCw className="mr-2 size-3.5" />
            )}
            刷新检查
          </Button>
        </div>
      </div>

      {healthQuery.error && (
        <div className="settings-danger-panel">
          {healthQuery.error instanceof Error
            ? healthQuery.error.message
            : String(healthQuery.error)}
        </div>
      )}

      {healthQuery.isLoading ? (
        <div className="flex items-center gap-2 rounded-md bg-[rgba(44,44,42,0.025)] px-3 py-3 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          正在检查运行环境…
        </div>
      ) : (
        <div className="grid gap-2">
          {rows.map((row) => (
            <HealthRowItem key={row.key} row={row} />
          ))}
        </div>
      )}

      {/* R1.3 reliability panel — surfaces WeChat connectivity +
          outbox state in one place so users no longer have to guess
          why a reply didn't go out. Polls every 5s while mounted. */}
      <div className="mt-3">
        <div className="mb-1.5 text-[12.5px] font-medium text-foreground">
          WeChat 渠道详情
        </div>
        <WeChatHealthPanel />
      </div>
    </SettingGroup>
  );
}

function HealthRowItem({ row }: { row: HealthRow }) {
  const Icon = row.level === "ok" ? CheckCircle2 : row.level === "warn" ? AlertTriangle : XCircle;
  const statusClass =
    row.level === "ok"
      ? "settings-status-pill--ok"
      : row.level === "warn"
        ? "settings-status-pill--warn"
        : "settings-status-pill--error";
  const statusLabel =
    row.level === "ok"
      ? "运行正常"
      : row.level === "warn"
        ? "需要处理"
        : "严重错误";
  return (
    <div className="settings-health-row">
      <Icon className="mt-0.5 size-4 shrink-0 text-[#5F5E5A]" />
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium text-foreground">{row.label}</div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">
          {row.detail}
        </div>
      </div>
      {row.action ? (
        <button type="button" className="settings-text-link" onClick={row.action.onClick}>
          {row.action.label}
        </button>
      ) : null}
      <span className={`settings-status-pill ${statusClass}`}>
        {statusLabel}
      </span>
    </div>
  );
}

function formatRelativeTime(timestamp: number) {
  const diffMs = Date.now() - timestamp;
  const minutes = Math.max(0, Math.floor(diffMs / 60_000));
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return new Date(timestamp).toLocaleDateString("zh-CN", {
    month: "long",
    day: "numeric",
  });
}

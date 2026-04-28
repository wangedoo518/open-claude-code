/**
 * R1.3 reliability sprint — WeChat 健康面板.
 *
 * Compact panel that surfaces the durability + connectivity state of
 * every WeChat channel in one glance. Replaces the previous "guess
 * what's broken from logs" workflow with explicit status:
 *
 *   - Overall verdict pill (Connected / Degraded / Disconnected /
 *     Not configured) — backend-derived in
 *     `wechat_health_handler::derive`, so the UI doesn't re-implement
 *     the worst-wins rules.
 *   - Kefu monitor row: running flag, last poll, consecutive failures,
 *     last error.
 *   - iLink accounts roster: per-account status (connected /
 *     disconnected / expired) plus last-active time.
 *   - Outbox histogram: pending / sending / sent / failed / cancelled
 *     counts pulled from `_wechat_outbox.json`.
 *
 * Polling: every 5 s while mounted. WeChat reliability changes slowly
 * (network flap = 30 s monitor backoff, send retries take minutes), so
 * a short interval is sufficient. SSE-based push is a follow-up for
 * status events richer than this snapshot.
 *
 * Surfaced via the SettingsPage `RuntimeHealthSection` so it lives next
 * to the other system-wide diagnostic surfaces. Lightweight enough to
 * also embed in a future Dashboard widget.
 */

import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  Loader2,
  RefreshCw,
  Send,
  Wifi,
  WifiOff,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { fetchWeChatHealth, type WeChatHealthVerdict } from "@/api/wechat/bridge";
import { formatRelativeTime } from "@/features/wechat/health-state";
import { cn } from "@/lib/utils";

interface VerdictMeta {
  label: string;
  tone: string;
  Icon: React.ComponentType<{ className?: string }>;
  description: string;
}

const VERDICT_META: Record<WeChatHealthVerdict, VerdictMeta> = {
  connected: {
    label: "已连接",
    tone: "border-emerald-500/40 bg-emerald-50 text-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-200",
    Icon: Wifi,
    description: "所有渠道运行正常，发送队列清空。",
  },
  degraded: {
    label: "降级运行",
    tone: "border-amber-500/40 bg-amber-50 text-amber-800 dark:bg-amber-950/40 dark:text-amber-200",
    Icon: AlertTriangle,
    description: "有连续失败或卡住的回复 — 系统仍在运行，但需要关注。",
  },
  disconnected: {
    label: "已断开",
    tone: "border-red-500/40 bg-red-50 text-red-800 dark:bg-red-950/40 dark:text-red-200",
    Icon: WifiOff,
    description: "至少一个已配置渠道未连接 — 可能需要重新登录或重启监听。",
  },
  not_configured: {
    label: "未配置",
    tone: "border-muted-foreground/30 bg-muted/30 text-muted-foreground",
    Icon: AlertTriangle,
    description: "尚未配置任何 WeChat 渠道（kefu / iLink）。",
  },
};

export function WeChatHealthPanel() {
  const query = useQuery({
    queryKey: ["wechat-health"],
    queryFn: fetchWeChatHealth,
    // R1.3: 5s polling while panel is mounted. Stops automatically
    // on unmount (default react-query behaviour).
    refetchInterval: 5_000,
    // No need for stale data on remount — health changes are slow.
    staleTime: 0,
  });

  if (query.isLoading) {
    return (
      <div className="flex items-center gap-2 rounded-md border border-border/50 bg-muted/20 px-3 py-2 text-[12px] text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        <span>正在加载 WeChat 健康状态…</span>
      </div>
    );
  }

  if (query.error || !query.data) {
    const message = query.error instanceof Error ? query.error.message : String(query.error);
    return (
      <div className="flex items-start gap-2 rounded-md border border-red-500/30 bg-red-50 px-3 py-2 text-[12px] text-red-800 dark:bg-red-950/30 dark:text-red-200">
        <AlertTriangle className="size-3.5 shrink-0" />
        <div className="flex-1 space-y-1">
          <div className="font-medium">无法读取 WeChat 健康状态</div>
          <div className="font-mono text-[10.5px] opacity-80">{message || "未知错误"}</div>
        </div>
        <Button
          size="sm"
          variant="outline"
          className="h-6 px-2 text-[11px]"
          onClick={() => void query.refetch()}
        >
          <RefreshCw className="mr-1 size-3" />
          重试
        </Button>
      </div>
    );
  }

  const { health, kefu, ilink_accounts, outbox } = query.data;
  const meta = VERDICT_META[health];
  const Icon = meta.Icon;

  return (
    <div className="space-y-3 rounded-md border border-border/50 p-3">
      {/* ── Overall verdict ─────────────────────────────────────── */}
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-2">
          <Badge variant="outline" className={cn("gap-1.5 px-2 py-0.5 text-[11px]", meta.tone)}>
            <Icon className="size-3 shrink-0" />
            {meta.label}
          </Badge>
          <span className="text-[11.5px] leading-snug text-muted-foreground">
            {meta.description}
          </span>
        </div>
        <Button
          size="sm"
          variant="ghost"
          className="h-6 shrink-0 px-2 text-[11px]"
          onClick={() => void query.refetch()}
          title="刷新"
        >
          <RefreshCw className={cn("size-3", query.isFetching && "animate-spin")} />
        </Button>
      </div>

      {/* ── Kefu row ────────────────────────────────────────────── */}
      <div className="space-y-1 border-t border-border/30 pt-2 text-[11.5px]">
        <div className="flex items-center justify-between gap-2 font-medium text-foreground">
          <span>微信客服 (kefu)</span>
          {kefu.configured ? (
            kefu.monitor_running ? (
              <Badge variant="outline" className="gap-1 border-emerald-500/40 text-emerald-700 dark:text-emerald-300">
                <CheckCircle2 className="size-2.5" />
                运行中
              </Badge>
            ) : (
              <Badge variant="outline" className="gap-1 border-red-500/40 text-red-700 dark:text-red-300">
                <WifiOff className="size-2.5" />
                未运行
              </Badge>
            )
          ) : (
            <Badge variant="outline" className="text-muted-foreground">
              未配置
            </Badge>
          )}
        </div>
        {kefu.configured && (
          <dl className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-muted-foreground">
            <div className="flex justify-between">
              <dt>最后轮询</dt>
              <dd>{formatRelativeTime(kefu.last_poll_unix_ms)}</dd>
            </div>
            <div className="flex justify-between">
              <dt>最后入站</dt>
              <dd>{formatRelativeTime(kefu.last_inbound_unix_ms)}</dd>
            </div>
            <div className="flex justify-between">
              <dt>连续失败</dt>
              <dd
                className={cn(
                  kefu.consecutive_failures > 0 && "text-amber-700 dark:text-amber-300",
                )}
              >
                {kefu.consecutive_failures}
              </dd>
            </div>
            {kefu.last_error && (
              <div className="col-span-2 mt-0.5 truncate text-red-700 dark:text-red-300" title={kefu.last_error}>
                ⚠ {kefu.last_error}
              </div>
            )}
          </dl>
        )}
      </div>

      {/* ── iLink accounts ──────────────────────────────────────── */}
      {ilink_accounts.length > 0 && (
        <div className="space-y-1 border-t border-border/30 pt-2 text-[11.5px]">
          <div className="font-medium text-foreground">个微 iLink ({ilink_accounts.length})</div>
          <ul className="space-y-0.5">
            {ilink_accounts.map((acc) => (
              <li key={acc.id} className="flex items-center justify-between gap-2 text-muted-foreground">
                <span className="truncate" title={acc.id}>
                  {acc.display_name || acc.id}
                </span>
                <Badge
                  variant="outline"
                  className={cn(
                    "shrink-0 gap-1 px-1.5 py-0 text-[10px]",
                    acc.status === "connected"
                      ? "border-emerald-500/40 text-emerald-700 dark:text-emerald-300"
                      : acc.status === "expired"
                        ? "border-amber-500/40 text-amber-700 dark:text-amber-300"
                        : "border-red-500/40 text-red-700 dark:text-red-300",
                  )}
                >
                  {labelForIlinkStatus(acc.status)}
                </Badge>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* ── Outbox histogram ────────────────────────────────────── */}
      <div className="space-y-1 border-t border-border/30 pt-2 text-[11.5px]">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 font-medium text-foreground">
            <Send className="size-3" />
            <span>发送队列</span>
          </div>
          {outbox.failed > 0 && (
            <Badge
              variant="outline"
              className="gap-1 border-red-500/40 px-1.5 py-0 text-[10px] text-red-700 dark:text-red-300"
            >
              <AlertTriangle className="size-2.5" />
              {outbox.failed} 失败
            </Badge>
          )}
        </div>
        <dl className="grid grid-cols-5 gap-2 text-center text-muted-foreground">
          <OutboxCountCell label="待发" count={outbox.pending} highlight={outbox.pending > 0} />
          <OutboxCountCell label="发送中" count={outbox.sending} />
          <OutboxCountCell label="已发" count={outbox.sent} />
          <OutboxCountCell label="失败" count={outbox.failed} highlight={outbox.failed > 0} danger />
          <OutboxCountCell label="已取消" count={outbox.cancelled} />
        </dl>
      </div>
    </div>
  );
}

function OutboxCountCell({
  label,
  count,
  highlight,
  danger,
}: {
  label: string;
  count: number;
  highlight?: boolean;
  danger?: boolean;
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-[9.5px] uppercase tracking-wide opacity-70">{label}</dt>
      <dd
        className={cn(
          "font-mono text-[12px] font-semibold",
          highlight && !danger && "text-amber-700 dark:text-amber-300",
          highlight && danger && "text-red-700 dark:text-red-300",
        )}
      >
        {count}
      </dd>
    </div>
  );
}

function labelForIlinkStatus(status: string): string {
  switch (status) {
    case "connected":
      return "已连接";
    case "expired":
      return "登录过期";
    case "disconnected":
      return "已断开";
    default:
      return status || "未知";
  }
}

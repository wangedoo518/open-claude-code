// S2 Settings subpage — Subscription & Codex Pool (read-only).
//
// Per ClawWiki canonical §9.2, the Codex broker pool is OWNED by the
// Rust process and this panel is the ONLY user-facing surface. It
// shows aggregate counts + the redacted account list and a "Clear
// pool" button. There is intentionally no editing, no API-key paste
// form, no provider picker — those would violate cut #6 of §11.1.
//
// Data flow on this page (all read-only):
//   GET /api/broker/status              -> pool_size / fresh / expiring / expired
//   GET /api/desktop/cloud/codex-accounts  -> redacted CloudAccountPublic[]
//   POST /api/desktop/cloud/codex-accounts/clear  (danger button)
//
// The sync route (`POST /api/desktop/cloud/codex-accounts`) is NOT
// called from this page. It runs automatically inside
// `billing/cloud-accounts-sync.ts` whenever the user's subscription
// status changes. This panel is informational only.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Loader2, ServerCog, Shield, Trash2 } from "lucide-react";
import {
  clearCloudCodexAccounts,
  getBrokerStatus,
  listCloudCodexAccounts,
  type BrokerPublicStatus,
  type CloudAccountPublic,
  type CodexAccountStatus,
} from "../../api/private-cloud";
import { SettingGroup, SettingRow } from "../../components/SettingGroup";

const brokerKeys = {
  status: () => ["broker", "status"] as const,
  accounts: () => ["broker", "accounts"] as const,
};

export function SubscriptionCodexPool() {
  const queryClient = useQueryClient();

  const statusQuery = useQuery({
    queryKey: brokerKeys.status(),
    queryFn: getBrokerStatus,
    staleTime: 15_000,
    refetchInterval: 30_000,
  });

  const accountsQuery = useQuery({
    queryKey: brokerKeys.accounts(),
    queryFn: listCloudCodexAccounts,
    staleTime: 15_000,
  });

  const clearMutation = useMutation({
    mutationFn: () => clearCloudCodexAccounts(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: brokerKeys.status() });
      void queryClient.invalidateQueries({ queryKey: brokerKeys.accounts() });
    },
  });

  return (
    <div className="space-y-4">
      <SettingGroup
        title="订阅与令牌池"
        description="由订阅交付的 Codex 账号。所有令牌都在 Rust 进程内部，不会暴露给前端或任何外部客户端。"
      >
        <StatusSnapshot status={statusQuery.data} isLoading={statusQuery.isLoading} />
      </SettingGroup>

      <SettingGroup
        title="账号列表"
        description="脱敏视图。仅显示别名和有效期，令牌永远不会离开 Rust 代理。"
      >
        <AccountListSection
          accounts={accountsQuery.data?.accounts}
          isLoading={accountsQuery.isLoading}
          error={accountsQuery.error}
        />
      </SettingGroup>

      <SettingGroup
        title="危险操作"
        description="清空池子将删除所有已交付账号和加密文件。下次订阅同步时会自动重新填充。"
      >
        <SettingRow
          label="清空令牌池"
          description="删除所有池条目和磁盘上的加密文件。安全操作——下次同步会自动重新填充。"
        >
          <button
            type="button"
            onClick={() => {
              if (window.confirm("Clear all Codex accounts from the pool?")) {
                clearMutation.mutate();
              }
            }}
            disabled={clearMutation.isPending}
            className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-body-sm font-medium text-muted-foreground transition-colors hover:border-destructive hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
          >
            {clearMutation.isPending ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Trash2 className="size-3" />
            )}
            清空
          </button>
        </SettingRow>
        {clearMutation.error && (
          <div
            className="mt-2 rounded-md border px-3 py-2 text-caption"
            style={{
              borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
              backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
              color: "var(--color-error)",
            }}
          >
            清空失败：{String(clearMutation.error)}
          </div>
        )}
      </SettingGroup>
    </div>
  );
}

function StatusSnapshot({
  status,
  isLoading,
}: {
  status: BrokerPublicStatus | undefined;
  isLoading: boolean;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center gap-2 text-caption text-muted-foreground">
        <Loader2 className="size-3 animate-spin" />
        加载代理状态…
      </div>
    );
  }
  if (!status) {
    return (
      <div
        className="rounded-md border px-3 py-2 text-caption"
        style={{
          borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        无法连接 Codex 代理。请检查 desktop-server 是否运行。
      </div>
    );
  }

  const nextRefreshLabel = status.next_refresh_at_epoch
    ? formatEpochRelative(status.next_refresh_at_epoch)
    : "—";

  return (
    <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
      <StatCard
        icon={ServerCog}
        label="池大小"
        value={String(status.pool_size)}
      />
      <StatCard
        icon={Shield}
        label="可用"
        value={String(status.fresh_count)}
        tint="var(--color-success)"
      />
      <StatCard
        icon={AlertTriangle}
        label="即将过期"
        value={String(status.expiring_count)}
        tint="var(--color-warning)"
      />
      <StatCard
        icon={AlertTriangle}
        label="已过期"
        value={String(status.expired_count)}
        tint="var(--color-error)"
      />
      <StatCard
        icon={ServerCog}
        label="今日请求"
        value={status.requests_today.toLocaleString()}
      />
      <div className="col-span-3 rounded-md border border-border bg-muted/10 px-3 py-2">
        <div className="text-caption text-muted-foreground">下次刷新</div>
        <div className="text-body-sm text-foreground">{nextRefreshLabel}</div>
        {status.next_refresh_at_epoch && (
          <div className="text-caption text-muted-foreground/60">
            {new Date(status.next_refresh_at_epoch * 1000).toISOString()}
          </div>
        )}
      </div>
    </div>
  );
}

function StatCard({
  icon: Icon,
  label,
  value,
  tint,
}: {
  icon: typeof ServerCog;
  label: string;
  value: string;
  tint?: string;
}) {
  return (
    <div className="rounded-md border border-border bg-muted/10 px-3 py-2">
      <div className="mb-1 flex items-center gap-1.5 text-caption text-muted-foreground">
        <Icon className="size-3" style={tint ? { color: tint } : undefined} />
        {label}
      </div>
      <div
        className="text-subhead font-semibold tabular-nums"
        style={tint ? { color: tint } : { color: "var(--color-foreground)" }}
      >
        {value}
      </div>
    </div>
  );
}

function AccountListSection({
  accounts,
  isLoading,
  error,
}: {
  accounts: CloudAccountPublic[] | undefined;
  isLoading: boolean;
  error: Error | null;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center gap-2 text-caption text-muted-foreground">
        <Loader2 className="size-3 animate-spin" />
        加载账号列表…
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="rounded-md border px-3 py-2 text-caption"
        style={{
          borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        账号列表加载失败：{error.message}
      </div>
    );
  }

  if (!accounts || accounts.length === 0) {
    return (
      <div className="rounded-md border border-dashed border-border px-3 py-4 text-caption text-muted-foreground">
        当前没有已同步的订阅账号。
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {accounts.map((account) => (
        <div
          key={`${account.codex_user_id}:${account.cloud_account_id ?? "none"}`}
          className="flex items-center justify-between rounded-md border border-border bg-muted/10 px-3 py-2"
        >
          <div className="min-w-0">
            <div className="truncate text-body-sm font-medium text-foreground">
              {account.alias}
            </div>
            <div className="text-caption text-muted-foreground">
              user={account.codex_user_id}
              {account.subscription_id ? ` · sub=${account.subscription_id}` : ""}
            </div>
          </div>
          <div className="ml-4 text-right">
            <StatusBadge status={account.status} />
            <div className="mt-1 text-caption text-muted-foreground">
              过期于 {formatEpochRelative(account.token_expires_at_epoch)}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function StatusBadge({ status }: { status: CodexAccountStatus }) {
  const styles: Record<CodexAccountStatus, { label: string; color: string }> = {
    fresh: { label: "可用", color: "var(--color-success)" },
    expiring: { label: "即将过期", color: "var(--color-warning)" },
    expired: { label: "已过期", color: "var(--color-error)" },
  };
  const meta = styles[status];
  return (
    <span
      className="inline-flex rounded-full px-2 py-0.5 text-caption"
      style={{
        color: meta.color,
        backgroundColor: `color-mix(in srgb, ${meta.color} 10%, transparent)`,
        border: `1px solid color-mix(in srgb, ${meta.color} 25%, transparent)`,
      }}
    >
      {meta.label}
    </span>
  );
}

function formatEpochRelative(epochSeconds: number): string {
  const deltaMs = epochSeconds * 1000 - Date.now();
  const absMinutes = Math.round(Math.abs(deltaMs) / 60_000);

  if (absMinutes < 1) return deltaMs >= 0 ? "不到 1 分钟后" : "刚刚过期";
  if (absMinutes < 60) {
    return deltaMs >= 0
      ? `${absMinutes} 分钟后`
      : `${absMinutes} 分钟前`;
  }

  const absHours = Math.round(absMinutes / 60);
  if (absHours < 48) {
    return deltaMs >= 0
      ? `${absHours} 小时后`
      : `${absHours} 小时前`;
  }

  const absDays = Math.round(absHours / 24);
  return deltaMs >= 0 ? `${absDays} 天后` : `${absDays} 天前`;
}

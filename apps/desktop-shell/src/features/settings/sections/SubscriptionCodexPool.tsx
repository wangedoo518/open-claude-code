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

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Loader2, ServerCog, Shield, AlertTriangle, Trash2 } from "lucide-react";
import { SettingGroup, SettingRow } from "../components/SettingGroup";
import {
  getBrokerStatus,
  listCloudCodexAccounts,
  clearCloudCodexAccounts,
  type BrokerPublicStatus,
  type CloudAccountPublic,
  type CodexAccountStatus,
} from "../api/client";

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
        title="Subscription & Codex Pool"
        description="Managed Codex accounts delivered by your subscription. All tokens live inside the Rust process — no HTTP surface exposes them to this page or any external client."
      >
        <StatusSnapshot status={statusQuery.data} isLoading={statusQuery.isLoading} />
      </SettingGroup>

      <SettingGroup
        title="Account list"
        description="Redacted view. You see the alias and expiry; access / refresh tokens never leave the Rust broker."
      >
        <AccountListSection
          accounts={accountsQuery.data?.accounts}
          isLoading={accountsQuery.isLoading}
          error={accountsQuery.error}
        />
      </SettingGroup>

      <SettingGroup
        title="Danger zone"
        description="Clearing the pool forgets every delivered account and deletes the encrypted blob under ~/.clawwiki/.clawwiki/cloud-accounts.enc. On next subscription sync, the pool will refill automatically."
      >
        <SettingRow
          label="Clear Codex pool"
          description="Delete all pool entries and the encrypted blob on disk. Safe — the next billing sync will refill it."
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
            Clear pool
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
            Failed to clear pool: {String(clearMutation.error)}
          </div>
        )}
      </SettingGroup>
    </div>
  );
}

/* ─── Status snapshot card ─────────────────────────────────────── */

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
        Loading broker status…
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
        Could not reach the Codex broker. Is desktop-server running?
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
        label="Pool size"
        value={String(status.pool_size)}
      />
      <StatCard
        icon={Shield}
        label="Fresh"
        value={String(status.fresh_count)}
        tint="var(--color-success)"
      />
      <StatCard
        icon={AlertTriangle}
        label="Expiring"
        value={String(status.expiring_count)}
        tint="var(--color-warning)"
      />
      <StatCard
        icon={AlertTriangle}
        label="Expired"
        value={String(status.expired_count)}
        tint="var(--color-error)"
      />
      <StatCard
        icon={ServerCog}
        label="Requests today"
        value={status.requests_today.toLocaleString()}
      />
      <div className="col-span-3 rounded-md border border-border bg-muted/10 px-3 py-2">
        <div className="text-caption text-muted-foreground">Next refresh</div>
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

/* ─── Account list section ─────────────────────────────────────── */

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
        Loading account list…
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
        Failed to list accounts: {error.message}
      </div>
    );
  }
  if (!accounts || accounts.length === 0) {
    return (
      <div className="rounded-md border border-border/50 bg-muted/10 px-3 py-4 text-center text-caption text-muted-foreground">
        No Codex accounts in the pool yet. Your subscription will
        deliver them automatically; this panel will refresh within
        30 seconds.
      </div>
    );
  }

  return (
    <ul className="divide-y divide-border/40">
      {accounts.map((account) => (
        <li key={account.codex_user_id} className="py-2">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="font-mono text-caption text-muted-foreground">
                  {account.codex_user_id}
                </span>
                <StatusPill status={account.status} />
              </div>
              <div className="mt-0.5 truncate text-body-sm font-medium text-foreground">
                {account.alias}
              </div>
              <div className="mt-0.5 text-caption text-muted-foreground">
                expires {formatEpochRelative(account.token_expires_at_epoch)}
                {" · "}
                {new Date(account.token_expires_at_epoch * 1000).toISOString()}
              </div>
            </div>
            {account.subscription_id != null && (
              <div className="shrink-0 text-right text-caption text-muted-foreground">
                <div>sub #{account.subscription_id}</div>
                {account.cloud_account_id != null && (
                  <div>acct #{account.cloud_account_id}</div>
                )}
              </div>
            )}
          </div>
        </li>
      ))}
    </ul>
  );
}

function StatusPill({ status }: { status: CodexAccountStatus }) {
  const config = {
    fresh: { label: "fresh", color: "var(--color-success)" },
    expiring: { label: "expiring", color: "var(--color-warning)" },
    expired: { label: "expired", color: "var(--color-error)" },
  }[status];

  return (
    <span
      className="rounded-full border px-1.5 py-0.5 text-caption font-medium"
      style={{
        borderColor: `color-mix(in srgb, ${config.color} 40%, transparent)`,
        color: config.color,
      }}
    >
      {config.label}
    </span>
  );
}

/* ─── Time formatting ──────────────────────────────────────────── */

function formatEpochRelative(epochSecs: number): string {
  const nowSecs = Math.floor(Date.now() / 1000);
  const delta = epochSecs - nowSecs;

  if (delta <= 0) {
    const past = -delta;
    if (past < 60) return `${past}s ago`;
    if (past < 3600) return `${Math.floor(past / 60)}m ago`;
    if (past < 86_400) return `${Math.floor(past / 3600)}h ago`;
    return `${Math.floor(past / 86_400)}d ago`;
  }

  if (delta < 60) return `in ${delta}s`;
  if (delta < 3600) return `in ${Math.floor(delta / 60)}m`;
  if (delta < 86_400) return `in ${Math.floor(delta / 3600)}h`;
  return `in ${Math.floor(delta / 86_400)}d`;
}

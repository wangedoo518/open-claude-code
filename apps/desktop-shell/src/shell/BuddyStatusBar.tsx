import { useQuery } from "@tanstack/react-query";
import {
  Bot,
  CheckCircle2,
  GitBranch,
  HeartPulse,
  Inbox,
  Shield,
  type LucideIcon,
} from "lucide-react";
import type { CSSProperties } from "react";
import { getPatrolReport, getWikiStats, listInboxEntries } from "@/api/wiki/repository";
import { getPermissionConfig } from "@/features/permission/permission-config";
import { useSettingsStore } from "@/state/settings-store";

export function BuddyStatusBar() {
  const permissionMode = useSettingsStore((state) => state.permissionMode);
  const permissionConfig = getPermissionConfig(permissionMode);
  const PermissionIcon = permissionConfig.icon;

  const statsQuery = useQuery({
    queryKey: ["wiki", "stats", "status-bar"],
    queryFn: () => getWikiStats(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });
  const inboxQuery = useQuery({
    queryKey: ["wiki", "inbox", "status-bar"],
    queryFn: () => listInboxEntries(),
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
  const patrolQuery = useQuery({
    queryKey: ["wiki", "patrol", "status-bar"],
    queryFn: () => getPatrolReport(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });

  const pending = inboxQuery.data?.pending_count ?? 0;
  const stats = statsQuery.data;
  const patrolSummary = patrolQuery.data?.summary;
  const riskCount =
    (patrolSummary?.schema_violations ?? 0) +
    (patrolSummary?.orphans ?? 0) +
    (patrolSummary?.stale ?? 0);
  const healthTone = riskCount > 0 || pending > 0 ? "warning" : "success";
  const vaultReady = !statsQuery.error;

  return (
    <footer className="ds-status-bar" aria-label="Buddy 状态栏">
      <div className="ds-status-bar-left">
        <StatusItem
          icon={HeartPulse}
          label={healthTone === "success" ? "外脑健康" : `待处理 ${pending + riskCount}`}
          tone={healthTone}
        />
        <StatusItem
          icon={Inbox}
          label={`Inbox ${pending}`}
          tone={pending > 0 ? "warning" : "muted"}
        />
        <StatusItem
          icon={GitBranch}
          label={vaultReady ? "Git 默认启用" : "Vault 离线"}
          tone={vaultReady ? "success" : "warning"}
        />
      </div>
      <div className="ds-status-bar-right">
        <StatusItem
          icon={PermissionIcon}
          label={permissionConfig.label}
          tone="muted"
          style={permissionConfig.color ? { color: permissionConfig.color } : undefined}
        />
        <StatusItem icon={Bot} label="外部 AI 只读" tone="muted" />
        <StatusItem icon={Shield} label="session / permanent" tone="muted" />
        {stats && (
          <StatusItem
            icon={CheckCircle2}
            label={`${stats.wiki_count} 页 / ${stats.raw_count} 素材`}
            tone="muted"
          />
        )}
      </div>
    </footer>
  );
}

function StatusItem({
  icon: Icon,
  label,
  tone,
  style,
}: {
  icon: LucideIcon;
  label: string;
  tone: "success" | "warning" | "muted";
  style?: CSSProperties;
}) {
  return (
    <span className="ds-status-item" data-tone={tone} style={style}>
      <Icon className="size-3" />
      <span>{label}</span>
    </span>
  );
}

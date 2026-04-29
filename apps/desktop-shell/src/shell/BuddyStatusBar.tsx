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
import {
  getExternalAiWritePolicy,
  getPatrolReport,
  getVaultGitStatus,
  getWikiStats,
  listInboxEntries,
} from "@/api/wiki/repository";
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
  const gitQuery = useQuery({
    queryKey: ["wiki", "git", "status-bar"],
    queryFn: () => getVaultGitStatus(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const externalAiQuery = useQuery({
    queryKey: ["wiki", "external-ai", "write-policy", "status-bar"],
    queryFn: () => getExternalAiWritePolicy(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });

  const pending = inboxQuery.data?.pending_count ?? 0;
  const stats = statsQuery.data;
  const git = gitQuery.data;
  const activeExternalAiGrants =
    externalAiQuery.data?.grants.filter((grant) => grant.enabled).length ?? 0;
  const patrolSummary = patrolQuery.data?.summary;
  const riskCount =
    (patrolSummary?.schema_violations ?? 0) +
    (patrolSummary?.orphans ?? 0) +
    (patrolSummary?.stale ?? 0);
  const healthTone = riskCount > 0 || pending > 0 ? "warning" : "success";
  const gitLabel = gitStatusLabel(git, Boolean(gitQuery.error));
  const gitTone =
    !git || gitQuery.error || !git.git_available || !git.initialized || git.dirty
      ? "warning"
      : "success";

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
          label={gitLabel}
          tone={gitTone}
        />
      </div>
      <div className="ds-status-bar-right">
        <StatusItem
          icon={PermissionIcon}
          label={permissionConfig.label}
          tone="muted"
          style={permissionConfig.color ? { color: permissionConfig.color } : undefined}
        />
        <StatusItem
          icon={Bot}
          label={
            activeExternalAiGrants > 0
              ? `外部 AI ${activeExternalAiGrants} 授权`
              : "外部 AI 只读"
          }
          tone={activeExternalAiGrants > 0 ? "warning" : "muted"}
        />
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

function gitStatusLabel(
  git:
    | {
        git_available: boolean;
        initialized: boolean;
        dirty: boolean;
        changed_count: number;
        ahead: number;
        behind: number;
      }
    | undefined,
  hasError: boolean,
) {
  if (hasError) return "Git 状态不可用";
  if (!git) return "Git 检查中";
  if (!git.git_available) return "未安装 Git";
  if (!git.initialized) return "Git 未启用";
  if (git.dirty) return `Git ${git.changed_count} 改动`;
  if (git.behind > 0) return `Git behind ${git.behind}`;
  if (git.ahead > 0) return `Git ahead ${git.ahead}`;
  return "Git clean";
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

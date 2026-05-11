import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
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
    // E25 — icon-only by default; full label on hover via title attr.
    // Vault group (left) is global state (待处理 / Inbox / Git);
    // session group (right) is current-context state. A 1px separator
    // between them makes the visual grouping unambiguous.
    // Per docs/desktop-shell/specs/2026-05-11-display-density-spec.md §4.
    <footer className="ds-status-bar" aria-label="Buddy 状态栏">
      <div className="ds-status-bar-left">
        <StatusItem
          icon={HeartPulse}
          label={healthTone === "success" ? "外脑健康" : "待处理"}
          count={healthTone === "success" ? undefined : pending + riskCount}
          tone={healthTone}
          to="/"
        />
        <StatusItem
          icon={Inbox}
          label="Inbox"
          count={pending > 0 ? pending : undefined}
          tone={pending > 0 ? "warning" : "muted"}
          to="/inbox"
        />
        <StatusItem
          icon={GitBranch}
          label={gitLabel}
          count={
            git && git.git_available && git.initialized && git.dirty
              ? git.changed_count
              : undefined
          }
          tone={gitTone}
          to="/connections#git"
        />
      </div>
      <div aria-hidden="true" className="ds-status-bar-divider" />
      <div className="ds-status-bar-right">
        <StatusItem
          icon={PermissionIcon}
          label={permissionConfig.label}
          tone="muted"
          style={permissionConfig.color ? { color: permissionConfig.color } : undefined}
          to="/settings?tab=permissions"
        />
        <StatusItem
          icon={Bot}
          label={
            activeExternalAiGrants > 0
              ? `外部 AI ${activeExternalAiGrants} 授权`
              : "外部 AI 只读"
          }
          count={activeExternalAiGrants > 0 ? activeExternalAiGrants : undefined}
          tone={activeExternalAiGrants > 0 ? "warning" : "muted"}
          to="/connections#external-ai"
        />
        <StatusItem
          icon={Shield}
          label="session / permanent"
          tone="muted"
          to="/connections#external-ai"
        />
        {stats && (
          <StatusItem
            icon={CheckCircle2}
            label={`${stats.wiki_count} 页 · ${stats.raw_count} 素材`}
            count={stats.wiki_count}
            tone="muted"
            to="/wiki"
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
  count,
  tone,
  style,
  to,
}: {
  icon: LucideIcon;
  label: string;
  /** Optional inline count badge (kept visible alongside the icon
   * because counts are the primary glanceable signal). */
  count?: number;
  tone: "success" | "warning" | "muted";
  style?: CSSProperties;
  to?: string;
}) {
  // E25 — icon + optional count visible by default; full text label
  // shown only on hover via the title attribute. Keeps the bar
  // readable at a glance without 8 simultaneous text strings.
  const tooltip = count !== undefined ? `${label} · ${count}` : label;
  const content = (
    <>
      <Icon className="size-3.5" />
      {count !== undefined && (
        <span className="ds-status-item-count">{count}</span>
      )}
    </>
  );
  if (to) {
    return (
      <Link
        className="ds-status-item"
        data-tone={tone}
        style={style}
        to={to}
        title={tooltip}
        aria-label={tooltip}
      >
        {content}
      </Link>
    );
  }
  return (
    <span
      className="ds-status-item"
      data-tone={tone}
      style={style}
      title={tooltip}
      aria-label={tooltip}
    >
      {content}
    </span>
  );
}

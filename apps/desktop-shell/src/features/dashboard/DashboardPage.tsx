import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  ArrowRight,
  Bot,
  CheckCircle2,
  ClipboardList,
  FileText,
  GitBranch,
  HeartPulse,
  Inbox,
  Loader2,
  MessageCircle,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";
import { Link } from "react-router-dom";
import {
  getExternalAiWritePolicy,
  getVaultGitStatus,
  getPatrolReport,
  getWikiStats,
  listInboxEntries,
} from "@/api/wiki/repository";

export function DashboardPage() {
  const statsQuery = useQuery({
    queryKey: ["wiki", "stats", "pulse"],
    queryFn: () => getWikiStats(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });
  const inboxQuery = useQuery({
    queryKey: ["wiki", "inbox", "pulse"],
    queryFn: () => listInboxEntries(),
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
  const patrolQuery = useQuery({
    queryKey: ["wiki", "patrol", "pulse"],
    queryFn: () => getPatrolReport(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });
  const gitQuery = useQuery({
    queryKey: ["wiki", "git", "pulse"],
    queryFn: () => getVaultGitStatus(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const externalAiQuery = useQuery({
    queryKey: ["wiki", "external-ai", "write-policy", "pulse"],
    queryFn: () => getExternalAiWritePolicy(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });

  const pendingInbox = inboxQuery.data?.pending_count ?? 0;
  const stats = statsQuery.data;
  const patrol = patrolQuery.data;
  const git = gitQuery.data;
  const activeExternalAiGrants =
    externalAiQuery.data?.grants.filter((grant) => grant.enabled).length ?? 0;
  const schemaViolations = patrol?.summary.schema_violations ?? 0;
  const stalePages = patrol?.summary.stale ?? 0;
  const orphanPages = patrol?.summary.orphans ?? 0;
  const topActions = useMemo(
    () =>
      [
        pendingInbox > 0
          ? {
              label: `审阅 ${pendingInbox} 条待整理建议`,
              href: "/inbox",
              tone: "warning" as const,
            }
          : null,
        schemaViolations > 0
          ? {
              label: `修复 ${schemaViolations} 个 schema 问题`,
              href: "/rules#validation",
              tone: "warning" as const,
            }
          : null,
        stalePages + orphanPages > 0
          ? {
              label: `处理 ${stalePages + orphanPages} 个知识质量风险`,
              href: "/wiki",
              tone: "warning" as const,
            }
          : null,
        git?.dirty
          ? {
              label: `提交 ${git.changed_count} 个 Vault 改动`,
              href: "/connections#git",
              tone: "warning" as const,
            }
          : null,
        {
          label: "问外脑一个问题",
          href: "/ask",
          tone: "neutral" as const,
        },
      ].filter(Boolean).slice(0, 3) as Array<{
        label: string;
        href: string;
        tone: "warning" | "neutral";
      }>,
    [git?.changed_count, git?.dirty, orphanPages, pendingInbox, schemaViolations, stalePages],
  );

  const gitRisk = git?.dirty ? git.changed_count : 0;
  const totalRisks = pendingInbox + schemaViolations + stalePages + orphanPages + gitRisk;
  const isLoading =
    statsQuery.isLoading || inboxQuery.isLoading || patrolQuery.isLoading || gitQuery.isLoading;
  const headline =
    totalRisks > 0
      ? `外脑体检发现 ${totalRisks} 项需要处理`
      : "外脑今天很干净";
  const primaryAction = topActions[0] ?? {
    label: "粘贴一条素材",
    href: "/raw",
    tone: "neutral" as const,
  };

  return (
    <main className="min-h-full overflow-y-auto bg-background px-6 py-5 text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-5">
        <header className="border-b border-border/50 pb-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <div className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
                Home / Pulse
              </div>
              <h1 className="mt-2 text-[28px] font-semibold tracking-normal">
                {headline}
              </h1>
              <p className="mt-2 max-w-2xl text-[13px] leading-6 text-muted-foreground">
                Buddy 把今天的摄入、维护、知识质量、Git/Vault 和外部 AI 权限压成一张体检单。
              </p>
            </div>
            <Link
              to={primaryAction.href}
              className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-[13px] font-medium text-primary-foreground"
            >
              {primaryAction.label}
              <ArrowRight className="size-4" />
            </Link>
          </div>
        </header>

        {isLoading && (
          <div className="inline-flex items-center gap-2 text-[12px] text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            正在体检…
          </div>
        )}

        <section className="grid gap-3 lg:grid-cols-3">
          <HealthCard
            icon={Inbox}
            title="待审阅"
            value={pendingInbox}
            unit="条"
            status={pendingInbox > 0 ? "需要处理" : "正常"}
            tone={pendingInbox > 0 ? "warning" : "success"}
            href="/inbox"
          />
          <HealthCard
            icon={ShieldCheck}
            title="知识质量"
            value={schemaViolations + stalePages + orphanPages}
            unit="项"
            status={
              schemaViolations + stalePages + orphanPages > 0 ? "有风险" : "正常"
            }
            tone={
              schemaViolations + stalePages + orphanPages > 0
                ? "warning"
                : "success"
            }
            href="/wiki"
          />
          <HealthCard
            icon={GitBranch}
            title="Buddy Vault / Git"
            value={git?.changed_count ?? 0}
            unit="改动"
            status={gitHealthLabel(git, Boolean(gitQuery.error))}
            tone={
              !git || gitQuery.error || !git.git_available || !git.initialized || git.dirty
                ? "warning"
                : "success"
            }
            href="/connections"
          />
        </section>

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <HeartPulse className="size-4 text-primary" />
              <h2 className="text-[15px] font-medium">Top 3 建议动作</h2>
            </div>
            <div className="mt-4 space-y-2">
              {topActions.map((action, index) => (
                <Link
                  key={`${action.href}-${action.label}`}
                  to={action.href}
                  className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-background px-3 py-3 text-[13px] transition-colors hover:border-primary/40 hover:bg-muted/30"
                >
                  <span className="flex min-w-0 items-center gap-3">
                    <span
                      className="grid size-6 shrink-0 place-items-center rounded bg-muted text-[11px] text-muted-foreground"
                      aria-hidden="true"
                    >
                      {index + 1}
                    </span>
                    <span className="truncate">{action.label}</span>
                  </span>
                  <ArrowRight className="size-3.5 shrink-0 text-muted-foreground" />
                </Link>
              ))}
            </div>
          </div>

          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <Bot className="size-4 text-primary" />
              <h2 className="text-[15px] font-medium">外部 AI 权限</h2>
            </div>
            <div className="mt-4 space-y-2 text-[12px] text-muted-foreground">
              <StatusRow label="默认模式" value="只读" />
              <StatusRow
                label="会话授权"
                value={`${countExternalAiGrants(externalAiQuery.data?.grants, "session")} 个`}
              />
              <StatusRow
                label="永久规则"
                value={`${countExternalAiGrants(externalAiQuery.data?.grants, "permanent")} 个`}
              />
              <StatusRow label="写入范围" value="wiki / templates / guidance" />
              <StatusRow
                label="当前写权限"
                value={activeExternalAiGrants > 0 ? `${activeExternalAiGrants} 个授权` : "无"}
              />
              <StatusRow
                label="Git checkpoint"
                value={git?.dirty ? `${git.changed_count} 改动待提交` : "已同步本地历史"}
              />
            </div>
          </div>
        </section>

        <section className="grid gap-3 md:grid-cols-4">
          <MiniStat icon={FileText} label="知识页" value={stats?.wiki_count ?? 0} />
          <MiniStat icon={ClipboardList} label="原始素材" value={stats?.raw_count ?? 0} />
          <MiniStat icon={MessageCircle} label="今日摄入" value={stats?.today_ingest_count ?? 0} />
          <MiniStat icon={AlertTriangle} label="Schema 风险" value={schemaViolations} />
        </section>
      </div>
    </main>
  );
}

function countExternalAiGrants(
  grants:
    | Array<{
        level: "session" | "permanent";
        enabled: boolean;
      }>
    | undefined,
  level: "session" | "permanent",
) {
  return grants?.filter((grant) => grant.enabled && grant.level === level).length ?? 0;
}

function gitHealthLabel(
  git:
    | {
        git_available: boolean;
        initialized: boolean;
        dirty: boolean;
        changed_count: number;
      }
    | undefined,
  hasError: boolean,
) {
  if (hasError) return "不可用";
  if (!git) return "检查中";
  if (!git.git_available) return "未安装 Git";
  if (!git.initialized) return "未启用";
  if (git.dirty) return "有未提交";
  return "干净";
}

function HealthCard({
  icon: Icon,
  title,
  value,
  unit,
  status,
  tone,
  href,
}: {
  icon: LucideIcon;
  title: string;
  value: number;
  unit: string;
  status: string;
  tone: "success" | "warning";
  href: string;
}) {
  const toneClass =
    tone === "success"
      ? "text-[var(--color-success)] bg-[var(--color-success)]/10"
      : "text-[var(--color-warning)] bg-[var(--color-warning)]/10";
  return (
    <Link
      to={href}
      className="rounded-lg border border-border bg-card px-4 py-4 transition-colors hover:border-primary/40 hover:bg-muted/30"
    >
      <div className="flex items-center justify-between gap-3">
        <span className="grid size-9 place-items-center rounded-md bg-muted text-muted-foreground">
          <Icon className="size-4" />
        </span>
        <span className={`rounded px-2 py-1 text-[11px] leading-none ${toneClass}`}>
          {status}
        </span>
      </div>
      <div className="mt-5 flex items-end gap-1">
        <span className="text-[30px] font-semibold leading-none">{value}</span>
        <span className="pb-1 text-[12px] text-muted-foreground">{unit}</span>
      </div>
      <div className="mt-2 text-[13px] text-muted-foreground">{title}</div>
    </Link>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md bg-muted/50 px-3 py-2">
      <span>{label}</span>
      <span className="text-foreground">{value}</span>
    </div>
  );
}

function MiniStat({
  icon: Icon,
  label,
  value,
}: {
  icon: LucideIcon;
  label: string;
  value: number;
}) {
  return (
    <div className="rounded-lg border border-border bg-card px-4 py-4">
      <div className="flex items-center gap-2 text-[12px] text-muted-foreground">
        <Icon className="size-3.5" />
        <span>{label}</span>
      </div>
      <div className="mt-3 flex items-center gap-2">
        <span className="text-[24px] font-semibold">{value}</span>
        {value === 0 ? (
          <span className="text-[var(--color-success)]">
            <CheckCircle2 className="size-4" />
          </span>
        ) : null}
      </div>
    </div>
  );
}

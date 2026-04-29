import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  FileText,
  GitBranch,
  LockKeyhole,
  Loader2,
  MessageCircle,
  RefreshCw,
  Save,
  ShieldAlert,
  type LucideIcon,
} from "lucide-react";
import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addExternalAiWriteGrant,
  commitVaultGit,
  getExternalAiWritePolicy,
  getVaultGitDiff,
  getVaultGitStatus,
  revokeExternalAiWriteGrant,
} from "@/api/wiki/repository";
import type { ExternalAiWriteGrant, VaultGitStatus } from "@/api/wiki/types";

const CONNECTIONS = [
  {
    id: "wechat",
    label: "微信入口",
    description: "消息、文章、URL 的主要捕获入口。",
    status: "待检查",
    tone: "warning",
    icon: MessageCircle,
    href: "/wechat",
  },
  {
    id: "git",
    label: "Buddy Vault / Git",
    description: "新建 Vault 默认初始化 Git，所有写入都应能被 diff 和回滚。",
    status: "默认启用",
    tone: "success",
    icon: GitBranch,
    href: "/settings?tab=data",
  },
  {
    id: "external-ai",
    label: "外部 AI 受控写入",
    description: "默认只读；写入 wiki/schema/templates/root guidance 前需要授权。",
    status: "只读",
    tone: "neutral",
    icon: Bot,
    href: "/rules",
  },
] as const;

const WRITE_SCOPES = [
  "wiki/",
  "schema/templates",
  "AGENTS.md / CLAUDE.md",
  "当前选中页面",
] as const;

export function ConnectionsPage() {
  const queryClient = useQueryClient();
  const [commitMessage, setCommitMessage] = useState("");
  const [permanentScope, setPermanentScope] = useState("schema/templates/research.md");
  const [permanentNote, setPermanentNote] = useState("");
  const gitQuery = useQuery({
    queryKey: ["wiki", "git", "connections"],
    queryFn: () => getVaultGitStatus(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const git = gitQuery.data;
  const diffQuery = useQuery({
    queryKey: ["wiki", "git", "diff", "connections"],
    queryFn: () => getVaultGitDiff(false),
    enabled: Boolean(git?.git_available && git?.initialized && git?.dirty),
    staleTime: 5_000,
  });
  const commitMutation = useMutation({
    mutationFn: (message: string) => commitVaultGit(message),
    onSuccess: () => {
      setCommitMessage("");
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
    },
  });
  const policyQuery = useQuery({
    queryKey: ["wiki", "external-ai", "write-policy", "connections"],
    queryFn: () => getExternalAiWritePolicy(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const addGrantMutation = useMutation({
    mutationFn: addExternalAiWriteGrant,
    onSuccess: () => {
      setPermanentNote("");
      void queryClient.invalidateQueries({
        queryKey: ["wiki", "external-ai", "write-policy"],
      });
    },
  });
  const revokeGrantMutation = useMutation({
    mutationFn: (id: string) => revokeExternalAiWriteGrant(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["wiki", "external-ai", "write-policy"],
      });
    },
  });
  const gitBadge = gitConnectionStatus(git, Boolean(gitQuery.error));
  const activeExternalAiGrants =
    policyQuery.data?.grants.filter((grant) => grant.enabled) ?? [];
  const externalAiBadge =
    activeExternalAiGrants.length > 0
      ? {
          label: `${activeExternalAiGrants.length} 授权`,
          tone: "warning" as const,
        }
      : {
          label: "只读",
          tone: "neutral" as const,
        };
  const suggestedCommitMessage = useMemo(() => {
    if (!git?.dirty) return "Checkpoint Buddy Vault";
    return `Checkpoint Buddy Vault: ${git.changed_count} changes`;
  }, [git?.changed_count, git?.dirty]);
  const connectionCards = CONNECTIONS.map((item) => {
    if (item.id === "git") {
      return { ...item, status: gitBadge.label, tone: gitBadge.tone };
    }
    if (item.id === "external-ai") {
      return {
        ...item,
        status: externalAiBadge.label,
        tone: externalAiBadge.tone,
      };
    }
    return item;
  });

  function handleCommit() {
    const message = (commitMessage || suggestedCommitMessage).trim();
    if (!message || commitMutation.isPending) return;
    commitMutation.mutate(message);
  }

  function handleSessionGrant() {
    if (addGrantMutation.isPending) return;
    addGrantMutation.mutate({
      level: "session",
      scope: "wiki/",
      note: "Session grant from Connections",
    });
  }

  function handlePermanentGrant() {
    const scope = permanentScope.trim();
    if (!scope || addGrantMutation.isPending) return;
    addGrantMutation.mutate({
      level: "permanent",
      scope,
      note: permanentNote,
    });
  }

  return (
    <main className="min-h-full overflow-y-auto bg-background px-6 py-5 text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-5">
        <header className="flex flex-wrap items-end justify-between gap-3 border-b border-border/50 pb-4">
          <div>
            <div className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              Connections
            </div>
            <h1 className="mt-1 text-[22px] font-semibold tracking-normal">
              连接
            </h1>
            <p className="mt-1 max-w-2xl text-[13px] leading-6 text-muted-foreground">
              微信、模型、Git、MCP 与外部 AI 都在这里显式连接、授权和撤销。
            </p>
          </div>
          <Link
            to="/wechat"
            className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-[13px] text-primary-foreground"
          >
            <MessageCircle className="size-4" />
            连接微信
          </Link>
        </header>

        <section className="grid gap-3 lg:grid-cols-3">
          {connectionCards.map((item) => {
            const Icon = item.icon;
            return (
              <Link
                key={item.id}
                to={item.href}
                className="rounded-lg border border-border bg-card px-4 py-4 text-card-foreground transition-colors hover:border-primary/40 hover:bg-muted/30"
              >
                <div className="flex items-start justify-between gap-3">
                  <span className="grid size-9 place-items-center rounded-md bg-muted text-muted-foreground">
                    <Icon className="size-4" />
                  </span>
                  <StatusBadge tone={item.tone}>{item.status}</StatusBadge>
                </div>
                <h2 className="mt-4 text-[15px] font-medium">{item.label}</h2>
                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
                  {item.description}
                </p>
              </Link>
            );
          })}
        </section>

        <section
          id="git"
          className="rounded-lg border border-border bg-card px-5 py-5"
        >
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <div className="flex items-center gap-2">
                <GitBranch className="size-4 text-primary" />
                <h2 className="text-[15px] font-medium">Buddy Vault / Git</h2>
                <StatusBadge tone={gitBadge.tone}>{gitBadge.label}</StatusBadge>
              </div>
              <p className="mt-2 max-w-2xl text-[12px] leading-5 text-muted-foreground">
                Buddy 把 Vault 写入都落到普通 Git diff 上：先看状态，再做 checkpoint。
              </p>
            </div>
            <button
              type="button"
              onClick={() => {
                void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
              }}
              className="inline-flex h-8 items-center gap-2 rounded-md border border-border px-3 text-[12px] text-muted-foreground hover:bg-muted"
            >
              <RefreshCw className="size-3.5" />
              刷新
            </button>
          </div>

          <div className="mt-4 grid gap-4 lg:grid-cols-[280px_minmax(0,1fr)]">
            <div className="space-y-2 text-[12px]">
              <GitFact label="Vault" value={git?.vault_path ?? "检查中"} />
              <GitFact label="Branch" value={git?.branch || "未识别"} />
              <GitFact
                label="Remote"
                value={git?.remote_connected ? "已连接" : "未连接"}
              />
              <GitFact
                label="History"
                value={git?.last_commit || "还没有 checkpoint"}
              />
              <GitFact
                label="Ahead / Behind"
                value={`${git?.ahead ?? 0} / ${git?.behind ?? 0}`}
              />
            </div>

            <div className="space-y-3">
              <div className="rounded-md border border-border/70 bg-background">
                <div className="flex items-center justify-between gap-3 border-b border-border/70 px-3 py-2">
                  <div className="flex items-center gap-2 text-[12px] font-medium">
                    <FileText className="size-3.5" />
                    工作区改动
                  </div>
                  <span className="text-[11px] text-muted-foreground">
                    {git?.changed_count ?? 0} files
                  </span>
                </div>
                <div className="max-h-44 overflow-auto px-3 py-2">
                  {gitQuery.isLoading ? (
                    <InlineState icon={Loader2} label="正在读取 Git 状态" spin />
                  ) : gitQuery.error ? (
                    <InlineState icon={AlertTriangle} label="Git 状态读取失败" />
                  ) : git?.changes.length ? (
                    <div className="space-y-1">
                      {git.changes.slice(0, 12).map((change) => (
                        <div
                          key={`${change.xy}-${change.path}`}
                          className="flex items-center gap-2 rounded bg-muted/40 px-2 py-1.5 text-[12px]"
                        >
                          <code className="w-7 shrink-0 text-[11px] text-muted-foreground">
                            {change.xy}
                          </code>
                          <span className="min-w-0 truncate">{change.path}</span>
                        </div>
                      ))}
                      {git.changes.length > 12 && (
                        <div className="px-2 py-1 text-[11px] text-muted-foreground">
                          还有 {git.changes.length - 12} 个改动未展示
                        </div>
                      )}
                    </div>
                  ) : (
                    <InlineState icon={CheckCircle2} label="工作区干净" />
                  )}
                </div>
              </div>

              <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_260px]">
                <div className="rounded-md border border-border/70 bg-background">
                  <div className="border-b border-border/70 px-3 py-2 text-[12px] font-medium">
                    Diff preview
                  </div>
                  <pre className="max-h-44 overflow-auto whitespace-pre-wrap px-3 py-3 text-[11px] leading-5 text-muted-foreground">
                    {diffQuery.isFetching
                      ? "Loading diff..."
                      : diffQuery.data?.diff ||
                        (git?.dirty
                          ? "未跟踪文件会在 checkpoint 时被纳入；当前没有可显示的 tracked diff。"
                          : "No diff.")}
                  </pre>
                </div>

                <div className="rounded-md border border-border/70 bg-background px-3 py-3">
                  <label className="text-[12px] font-medium" htmlFor="git-commit-message">
                    Checkpoint message
                  </label>
                  <textarea
                    id="git-commit-message"
                    value={commitMessage}
                    onChange={(event) => setCommitMessage(event.target.value)}
                    placeholder={suggestedCommitMessage}
                    className="mt-2 min-h-20 w-full resize-none rounded-md border border-border bg-card px-3 py-2 text-[12px] outline-none focus:border-primary"
                  />
                  <button
                    type="button"
                    onClick={handleCommit}
                    disabled={!git?.dirty || commitMutation.isPending}
                    className="mt-3 inline-flex h-8 w-full items-center justify-center gap-2 rounded-md bg-primary px-3 text-[12px] text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {commitMutation.isPending ? (
                      <Loader2 className="size-3.5 animate-spin" />
                    ) : (
                      <Save className="size-3.5" />
                    )}
                    创建 checkpoint
                  </button>
                  {commitMutation.error && (
                    <p className="mt-2 text-[11px] leading-5 text-[var(--color-warning)]">
                      {commitMutation.error instanceof Error
                        ? commitMutation.error.message
                        : "Checkpoint failed"}
                    </p>
                  )}
                </div>
              </div>
            </div>
          </div>
        </section>

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <LockKeyhole className="size-4 text-primary" />
              <h2 className="text-[15px] font-medium">受控自动写入授权</h2>
            </div>
            <div className="mt-4 grid gap-3 md:grid-cols-2">
              <AuthLevel
                title="本次会话有效"
                description="只在当前 app session 或 agent task 内生效，结束后自动回到只读。"
                badge="推荐"
              />
              <AuthLevel
                title="永久规则"
                description="写入 Rules/Connections，可撤销、可审计，并在 StatusBar 长期显示授权 badge。"
                badge="高风险"
              />
            </div>
            <div className="mt-4 grid gap-3 md:grid-cols-[220px_minmax(0,1fr)]">
              <button
                type="button"
                onClick={handleSessionGrant}
                disabled={addGrantMutation.isPending}
                className="inline-flex h-9 items-center justify-center gap-2 rounded-md bg-primary px-3 text-[12px] text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
              >
                {addGrantMutation.isPending ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <LockKeyhole className="size-3.5" />
                )}
                授权本次会话写 wiki/
              </button>
              <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_160px]">
                <input
                  value={permanentScope}
                  onChange={(event) => setPermanentScope(event.target.value)}
                  className="h-9 rounded-md border border-border bg-background px-3 text-[12px] outline-none focus:border-primary"
                  placeholder="schema/templates/research.md"
                />
                <button
                  type="button"
                  onClick={handlePermanentGrant}
                  disabled={addGrantMutation.isPending || !permanentScope.trim()}
                  className="inline-flex h-9 items-center justify-center gap-2 rounded-md border border-border px-3 text-[12px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <Save className="size-3.5" />
                  保存永久规则
                </button>
                <input
                  value={permanentNote}
                  onChange={(event) => setPermanentNote(event.target.value)}
                  className="h-9 rounded-md border border-border bg-background px-3 text-[12px] outline-none focus:border-primary md:col-span-2"
                  placeholder="可选说明，例如 research template tuning"
                />
              </div>
            </div>
            {addGrantMutation.error && (
              <p className="mt-3 text-[11px] leading-5 text-[var(--color-warning)]">
                {addGrantMutation.error instanceof Error
                  ? addGrantMutation.error.message
                  : "Grant failed"}
              </p>
            )}
            <div className="mt-4 rounded-md border border-border/70 bg-background">
              <div className="flex items-center justify-between gap-3 border-b border-border/70 px-3 py-2">
                <span className="text-[12px] font-medium">授权记录</span>
                <span className="text-[11px] text-muted-foreground">
                  {activeExternalAiGrants.length} active
                </span>
              </div>
              <div className="max-h-48 overflow-auto px-3 py-2">
                {policyQuery.isLoading ? (
                  <InlineState icon={Loader2} label="正在读取授权策略" spin />
                ) : activeExternalAiGrants.length ? (
                  <div className="space-y-2">
                    {activeExternalAiGrants.map((grant) => (
                      <GrantRow
                        key={grant.id}
                        grant={grant}
                        isRevoking={revokeGrantMutation.isPending}
                        onRevoke={() => revokeGrantMutation.mutate(grant.id)}
                      />
                    ))}
                  </div>
                ) : (
                  <InlineState icon={CheckCircle2} label="当前保持只读" />
                )}
              </div>
            </div>
          </div>

          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <ShieldAlert className="size-4 text-[var(--color-warning)]" />
              <h2 className="text-[15px] font-medium">允许写入范围</h2>
            </div>
            <div className="mt-4 space-y-2">
              {WRITE_SCOPES.map((scope) => (
                <div
                  key={scope}
                  className="flex items-center gap-2 rounded-md bg-muted/50 px-3 py-2 text-[12px]"
                >
                  <CheckCircle2 className="size-3.5 text-[var(--color-success)]" />
                  <span>{scope}</span>
                </div>
              ))}
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}

function gitConnectionStatus(
  git: VaultGitStatus | undefined,
  hasError: boolean,
): { label: string; tone: "success" | "warning" | "neutral" } {
  if (hasError) return { label: "不可用", tone: "warning" };
  if (!git) return { label: "检查中", tone: "neutral" };
  if (!git.git_available) return { label: "未安装 Git", tone: "warning" };
  if (!git.initialized) return { label: "未启用", tone: "warning" };
  if (git.dirty) return { label: `${git.changed_count} 改动`, tone: "warning" };
  return { label: "干净", tone: "success" };
}

function GitFact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 items-center justify-between gap-3 rounded-md bg-muted/50 px-3 py-2">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate text-right text-foreground">{value}</span>
    </div>
  );
}

function InlineState({
  icon: Icon,
  label,
  spin,
}: {
  icon: LucideIcon;
  label: string;
  spin?: boolean;
}) {
  return (
    <div className="flex items-center gap-2 py-2 text-[12px] text-muted-foreground">
      <Icon className={`size-3.5 ${spin ? "animate-spin" : ""}`} />
      <span>{label}</span>
    </div>
  );
}

function GrantRow({
  grant,
  isRevoking,
  onRevoke,
}: {
  grant: ExternalAiWriteGrant;
  isRevoking: boolean;
  onRevoke: () => void;
}) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md bg-muted/40 px-3 py-2 text-[12px]">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <StatusBadge tone={grant.level === "permanent" ? "warning" : "neutral"}>
            {grant.level === "permanent" ? "永久规则" : "本次会话"}
          </StatusBadge>
          <span className="min-w-0 truncate font-medium">{grant.scope}</span>
        </div>
        {grant.note && (
          <p className="mt-1 truncate text-[11px] text-muted-foreground">
            {grant.note}
          </p>
        )}
      </div>
      <button
        type="button"
        onClick={onRevoke}
        disabled={isRevoking}
        className="shrink-0 rounded border border-border px-2 py-1 text-[11px] text-muted-foreground hover:bg-background disabled:cursor-not-allowed disabled:opacity-50"
      >
        撤销
      </button>
    </div>
  );
}

function StatusBadge({
  tone,
  children,
}: {
  tone: "success" | "warning" | "neutral";
  children: string;
}) {
  const cls =
    tone === "success"
      ? "bg-[var(--color-success)]/10 text-[var(--color-success)]"
      : tone === "warning"
        ? "bg-[var(--color-warning)]/10 text-[var(--color-warning)]"
        : "bg-muted text-muted-foreground";
  return (
    <span className={`rounded px-2 py-1 text-[11px] leading-none ${cls}`}>
      {children}
    </span>
  );
}

function AuthLevel({
  title,
  description,
  badge,
}: {
  title: string;
  description: string;
  badge: string;
}) {
  return (
    <div className="rounded-md border border-border/70 bg-background px-4 py-4">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-[13px] font-medium">{title}</h3>
        <span className="rounded bg-muted px-2 py-1 text-[11px] text-muted-foreground">
          {badge}
        </span>
      </div>
      <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
        {description}
      </p>
    </div>
  );
}

import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  Download,
  FileText,
  GitBranch,
  LockKeyhole,
  Loader2,
  MessageCircle,
  RefreshCw,
  Save,
  ShieldAlert,
  Trash2,
  Upload,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addExternalAiWriteGrant,
  commitVaultGit,
  discardVaultGitChangeBlock,
  discardVaultGitHunk,
  discardVaultGitLine,
  discardVaultGitPath,
  getExternalAiWritePolicy,
  getVaultGitAudit,
  getVaultGitDiff,
  getVaultGitStatus,
  pullVaultGit,
  pushVaultGit,
  revokeExternalAiWriteGrant,
  setVaultGitRemote,
} from "@/api/wiki/repository";
import type {
  ExternalAiWriteGrant,
  VaultGitAuditEntry,
  VaultGitDiffHunk,
  VaultGitDiffLine,
  VaultGitStatus,
} from "@/api/wiki/types";

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

const EMPTY_HUNKS: VaultGitDiffHunk[] = [];

interface SelectedDiffLine {
  hunkIndex: number;
  lineIndex: number;
}

export function ConnectionsPage() {
  const queryClient = useQueryClient();
  const [commitMessage, setCommitMessage] = useState("");
  const [diffStaged, setDiffStaged] = useState(false);
  const [selectedDiffKey, setSelectedDiffKey] = useState<string | null>(null);
  const [selectedHunkIndex, setSelectedHunkIndex] = useState<number | null>(null);
  const [selectedDiffLine, setSelectedDiffLine] = useState<SelectedDiffLine | null>(null);
  const [remoteUrl, setRemoteUrl] = useState("");
  const [syncMessage, setSyncMessage] = useState<string | null>(null);
  const [permanentScope, setPermanentScope] = useState("schema/templates/research.md");
  const [permanentNote, setPermanentNote] = useState("");
  const gitQuery = useQuery({
    queryKey: ["wiki", "git", "connections"],
    queryFn: () => getVaultGitStatus(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const git = gitQuery.data;
  useEffect(() => {
    if (
      git?.git_available &&
      git.initialized &&
      git.staged_count > 0 &&
      git.unstaged_count === 0 &&
      git.untracked_count === 0
    ) {
      setDiffStaged(true);
    }
  }, [
    git?.git_available,
    git?.initialized,
    git?.staged_count,
    git?.unstaged_count,
    git?.untracked_count,
  ]);

  const diffQuery = useQuery({
    queryKey: ["wiki", "git", "diff", "connections", diffStaged],
    queryFn: () => getVaultGitDiff(diffStaged),
    enabled: Boolean(
      git?.git_available &&
        git?.initialized &&
        (diffStaged ? git.staged_count > 0 : git.dirty),
    ),
    staleTime: 5_000,
  });
  const auditQuery = useQuery({
    queryKey: ["wiki", "git", "audit", "connections"],
    queryFn: () => getVaultGitAudit(5),
    enabled: Boolean(git?.git_available && git?.initialized),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });
  const diffSections = diffQuery.data?.sections ?? [];
  const selectedDiffSection = useMemo(() => {
    if (!diffSections.length) return null;
    return (
      diffSections.find((section) => `${section.kind}:${section.path}` === selectedDiffKey) ??
      diffSections[0]
    );
  }, [diffSections, selectedDiffKey]);
  useEffect(() => {
    if (!diffSections.length) {
      if (selectedDiffKey) setSelectedDiffKey(null);
      return;
    }
    if (
      !selectedDiffKey ||
      !diffSections.some((section) => `${section.kind}:${section.path}` === selectedDiffKey)
    ) {
      const first = diffSections[0];
      setSelectedDiffKey(`${first.kind}:${first.path}`);
    }
  }, [diffSections, selectedDiffKey]);
  useEffect(() => {
    setSelectedHunkIndex(null);
    setSelectedDiffLine(null);
  }, [selectedDiffKey, diffStaged]);
  useEffect(() => {
    if (selectedHunkIndex === null) return;
    if (!selectedDiffSection?.hunks[selectedHunkIndex]) {
      setSelectedHunkIndex(null);
    }
  }, [selectedDiffSection, selectedHunkIndex]);
  useEffect(() => {
    if (!selectedDiffLine) return;
    if (!selectedDiffSection?.hunks[selectedDiffLine.hunkIndex]?.lines[selectedDiffLine.lineIndex]) {
      setSelectedDiffLine(null);
    }
  }, [
    selectedDiffLine?.hunkIndex,
    selectedDiffLine?.lineIndex,
    selectedDiffLine,
    selectedDiffSection,
  ]);

  const commitMutation = useMutation({
    mutationFn: (message: string) => commitVaultGit(message),
    onSuccess: () => {
      setCommitMessage("");
      setSyncMessage(null);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
    },
  });
  const pullMutation = useMutation({
    mutationFn: pullVaultGit,
    onSuccess: (result) => {
      setSyncMessage(result.summary || "Pull completed.");
      queryClient.setQueryData(["wiki", "git", "connections"], result.status);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
    },
  });
  const pushMutation = useMutation({
    mutationFn: pushVaultGit,
    onSuccess: (result) => {
      setSyncMessage(result.summary || "Push completed.");
      queryClient.setQueryData(["wiki", "git", "connections"], result.status);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
    },
  });
  const remoteMutation = useMutation({
    mutationFn: (url: string) => setVaultGitRemote({ remote: "origin", url }),
    onSuccess: (result) => {
      setRemoteUrl("");
      setSyncMessage(`origin -> ${result.remote_url_redacted}`);
      queryClient.setQueryData(["wiki", "git", "connections"], result.status);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
    },
  });
  const discardMutation = useMutation({
    mutationFn: discardVaultGitPath,
    onSuccess: (result) => {
      setSelectedDiffKey(null);
      setSelectedHunkIndex(null);
      setSyncMessage(result.summary);
      queryClient.setQueryData(["wiki", "git", "connections"], result.status);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
    },
  });
  const discardHunkMutation = useMutation({
    mutationFn: discardVaultGitHunk,
    onSuccess: (result) => {
      setSelectedHunkIndex(null);
      setSelectedDiffLine(null);
      setSyncMessage(result.summary);
      queryClient.setQueryData(["wiki", "git", "connections"], result.status);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
    },
  });
  const discardLineMutation = useMutation({
    mutationFn: discardVaultGitLine,
    onSuccess: (result) => {
      setSelectedDiffLine(null);
      setSyncMessage(result.summary);
      queryClient.setQueryData(["wiki", "git", "connections"], result.status);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
    },
  });
  const discardChangeBlockMutation = useMutation({
    mutationFn: discardVaultGitChangeBlock,
    onSuccess: (result) => {
      setSelectedDiffLine(null);
      setSyncMessage(result.summary);
      queryClient.setQueryData(["wiki", "git", "connections"], result.status);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git", "audit"] });
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
  const syncPending =
    pullMutation.isPending || pushMutation.isPending || remoteMutation.isPending;
  const auditEntries = auditQuery.data?.entries ?? [];
  const selectedHunks = selectedDiffSection?.hunks ?? EMPTY_HUNKS;
  const selectedHunk =
    selectedHunkIndex === null ? null : selectedHunks[selectedHunkIndex] ?? null;
  const selectedLineHunk =
    selectedDiffLine === null ? null : selectedHunks[selectedDiffLine.hunkIndex] ?? null;
  const selectedLine =
    selectedDiffLine === null || !selectedLineHunk
      ? null
      : selectedLineHunk.lines[selectedDiffLine.lineIndex] ?? null;
  const previewHunks = selectedHunk ? [selectedHunk] : selectedHunks;
  const selectedLineStats = useMemo(() => {
    const lines = selectedHunk
      ? selectedHunk.lines
      : selectedHunks.flatMap((hunk) => hunk.lines);
    return summarizeDiffLines(lines);
  }, [selectedHunk, selectedHunks]);
  const diffPreviewText = diffQuery.isFetching
    ? "Loading diff..."
    : selectedHunk
      ? formatDiffHunkPreview(selectedHunk)
    : selectedDiffSection?.diff ||
      diffQuery.data?.diff ||
      (diffStaged ? "No staged diff." : git?.dirty ? "No unstaged diff." : "No diff.");
  const canDiscardSelectedHunk = Boolean(
    selectedDiffSection?.kind === "tracked" && selectedHunk && !diffStaged,
  );
  const canSelectDiffLine = Boolean(selectedDiffSection?.kind === "tracked" && !diffStaged);
  const canDiscardSelectedLine = Boolean(
    canSelectDiffLine &&
      selectedDiffLine &&
      selectedLine &&
      selectedLineHunk &&
      isStandaloneAddedLine(selectedLineHunk, selectedDiffLine.lineIndex) &&
      !discardLineMutation.isPending,
  );
  const canDiscardSelectedChangeBlock = Boolean(
    canSelectDiffLine &&
      selectedDiffLine &&
      selectedLine &&
      selectedLineHunk &&
      isReplacementChangeBlock(selectedLineHunk, selectedDiffLine.lineIndex) &&
      !discardChangeBlockMutation.isPending,
  );
  const canSetRemote = Boolean(git?.git_available && git.initialized && remoteUrl.trim());
  const canRemoteSync = Boolean(
    git?.git_available && git.initialized && git.remote_connected && !git.dirty,
  );
  const syncDisabledReason = !git?.remote_connected
    ? "先设置 origin remote"
    : git?.dirty
      ? "先创建 checkpoint"
      : null;
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

  function handlePull() {
    if (!canRemoteSync || syncPending) return;
    setSyncMessage(null);
    pullMutation.mutate();
  }

  function handlePush() {
    if (!canRemoteSync || syncPending) return;
    setSyncMessage(null);
    pushMutation.mutate();
  }

  function handleSetRemote() {
    const url = remoteUrl.trim();
    if (!canSetRemote || remoteMutation.isPending) return;
    setSyncMessage(null);
    remoteMutation.mutate(url);
  }

  function handleDiscardSelectedPath() {
    if (!selectedDiffSection || discardMutation.isPending) return;
    const confirmed = window.confirm(`丢弃 ${selectedDiffSection.path} 的未提交改动？`);
    if (!confirmed) return;
    discardMutation.mutate(selectedDiffSection.path);
  }

  function handleDiscardSelectedHunk() {
    if (
      !selectedDiffSection ||
      !selectedHunk ||
      selectedHunkIndex === null ||
      !canDiscardSelectedHunk ||
      discardHunkMutation.isPending
    ) {
      return;
    }
    const confirmed = window.confirm(
      `只丢弃 ${selectedDiffSection.path} 的 H${selectedHunkIndex + 1}？`,
    );
    if (!confirmed) return;
    discardHunkMutation.mutate({
      path: selectedDiffSection.path,
      hunk_index: selectedHunkIndex,
      hunk_header: selectedHunk.header,
    });
  }

  function handleDiscardSelectedLine() {
    if (
      !selectedDiffSection ||
      !selectedDiffLine ||
      !selectedLineHunk ||
      !selectedLine ||
      !canDiscardSelectedLine
    ) {
      return;
    }
    const lineLabel = selectedLine.new_line ? `L${selectedLine.new_line}` : "选中行";
    const confirmed = window.confirm(
      `只丢弃 ${selectedDiffSection.path} 的新增行 ${lineLabel}？`,
    );
    if (!confirmed) return;
    discardLineMutation.mutate({
      path: selectedDiffSection.path,
      hunk_index: selectedDiffLine.hunkIndex,
      line_index: selectedDiffLine.lineIndex,
      hunk_header: selectedLineHunk.header,
      line_text: selectedLine.text,
      new_line: selectedLine.new_line ?? null,
    });
  }

  function handleDiscardSelectedChangeBlock() {
    if (
      !selectedDiffSection ||
      !selectedDiffLine ||
      !selectedLineHunk ||
      !selectedLine ||
      !canDiscardSelectedChangeBlock
    ) {
      return;
    }
    const lineLabel = selectedLine.new_line ? `L${selectedLine.new_line}` : "选中行";
    const confirmed = window.confirm(
      `恢复 ${selectedDiffSection.path} 的替换块 ${lineLabel}？`,
    );
    if (!confirmed) return;
    discardChangeBlockMutation.mutate({
      path: selectedDiffSection.path,
      hunk_index: selectedDiffLine.hunkIndex,
      line_index: selectedDiffLine.lineIndex,
      hunk_header: selectedLineHunk.header,
      line_text: selectedLine.text,
      new_line: selectedLine.new_line ?? null,
    });
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
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={handlePull}
                disabled={!canRemoteSync || syncPending}
                title={syncDisabledReason ?? "Fast-forward pull"}
                className="inline-flex h-8 items-center gap-2 rounded-md border border-border px-3 text-[12px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
              >
                {pullMutation.isPending ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Download className="size-3.5" />
                )}
                Pull
              </button>
              <button
                type="button"
                onClick={handlePush}
                disabled={!canRemoteSync || syncPending}
                title={syncDisabledReason ?? "Push checkpoints"}
                className="inline-flex h-8 items-center gap-2 rounded-md border border-border px-3 text-[12px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
              >
                {pushMutation.isPending ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Upload className="size-3.5" />
                )}
                Push
              </button>
              <button
                type="button"
                onClick={() => {
                  setSyncMessage(null);
                  void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
                }}
                className="inline-flex h-8 items-center gap-2 rounded-md border border-border px-3 text-[12px] text-muted-foreground hover:bg-muted"
              >
                <RefreshCw className="size-3.5" />
                刷新
              </button>
            </div>
          </div>

          <div className="mt-4 grid gap-4 lg:grid-cols-[280px_minmax(0,1fr)]">
            <div className="space-y-2 text-[12px]">
              <GitFact label="Vault" value={git?.vault_path ?? "检查中"} />
              <GitFact label="Branch" value={git?.branch || "未识别"} />
              <GitFact
                label="Remote"
                value={
                  git?.remote_connected
                    ? git.remote_name
                      ? `已连接 ${git.remote_name}`
                      : "已连接"
                    : "未连接"
                }
              />
              <GitFact
                label="Remote URL"
                value={git?.remote_url_redacted || "未设置"}
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
                  <div className="flex items-center justify-between gap-3 border-b border-border/70 px-3 py-2">
                    <div className="text-[12px] font-medium">
                      Diff preview
                    </div>
                    <div className="flex flex-wrap items-center justify-end gap-2">
                      <button
                        type="button"
                        onClick={handleDiscardSelectedLine}
                        disabled={!canDiscardSelectedLine}
                        title={
                          canSelectDiffLine
                            ? "选择独立新增行后可用"
                            : "仅支持未暂存 tracked diff"
                        }
                        className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border px-2 text-[11px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {discardLineMutation.isPending ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <Trash2 className="size-3.5" />
                        )}
                        丢弃新增行
                      </button>
                      <button
                        type="button"
                        onClick={handleDiscardSelectedChangeBlock}
                        disabled={!canDiscardSelectedChangeBlock}
                        title={
                          canSelectDiffLine
                            ? "选择替换型新增行后可用"
                            : "仅支持未暂存 tracked diff"
                        }
                        className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border px-2 text-[11px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {discardChangeBlockMutation.isPending ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <Trash2 className="size-3.5" />
                        )}
                        丢弃替换块
                      </button>
                      <button
                        type="button"
                        onClick={handleDiscardSelectedHunk}
                        disabled={!canDiscardSelectedHunk || discardHunkMutation.isPending}
                        title={
                          canDiscardSelectedHunk
                            ? "Discard selected unstaged hunk"
                            : "选择未暂存 tracked hunk 后可用"
                        }
                        className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border px-2 text-[11px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {discardHunkMutation.isPending ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <Trash2 className="size-3.5" />
                        )}
                        丢弃 Hunk
                      </button>
                      <button
                        type="button"
                        onClick={handleDiscardSelectedPath}
                        disabled={!selectedDiffSection || discardMutation.isPending}
                        className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border px-2 text-[11px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {discardMutation.isPending ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <Trash2 className="size-3.5" />
                        )}
                        丢弃文件
                      </button>
                      <div className="flex rounded-md border border-border/70 bg-card p-0.5">
                        {[
                          ["unstaged", "未暂存"],
                          ["staged", "已暂存"],
                        ].map(([id, label]) => {
                          const active = diffStaged === (id === "staged");
                          return (
                            <button
                              key={id}
                              type="button"
                              onClick={() => {
                                setDiffStaged(id === "staged");
                                setSelectedHunkIndex(null);
                                setSelectedDiffLine(null);
                              }}
                              className={`h-6 rounded px-2 text-[11px] ${
                                active
                                  ? "bg-primary text-primary-foreground"
                                  : "text-muted-foreground hover:bg-muted"
                              }`}
                            >
                              {label}
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  </div>
                  {diffQuery.isFetching || !previewHunks.length ? (
                    <pre className="max-h-44 overflow-auto whitespace-pre-wrap px-3 py-3 text-[11px] leading-5 text-muted-foreground">
                      {diffPreviewText}
                    </pre>
                  ) : (
                    <div className="max-h-44 overflow-auto bg-card/40 text-[11px]">
                      {previewHunks.map((hunk, index) => (
                        <DiffHunkBlock
                          key={`${hunk.header}-${index}`}
                          hunk={hunk}
                          index={
                            selectedHunkIndex === null ? index : selectedHunkIndex
                          }
                          selectedLine={selectedDiffLine}
                          lineSelectionEnabled={canSelectDiffLine}
                          onSelectLine={(lineIndex) => {
                            const hunkIndex =
                              selectedHunkIndex === null ? index : selectedHunkIndex;
                            setSelectedDiffLine({ hunkIndex, lineIndex });
                          }}
                        />
                      ))}
                    </div>
                  )}
                  {diffSections.length ? (
                    <div className="space-y-1 border-t border-border/60 px-3 py-2 text-[11px] text-muted-foreground">
                      <div className="flex flex-wrap items-center gap-2">
                        <span>
                          {diffSections.length} sections
                          {diffQuery.data?.truncated ? " · preview truncated" : ""}
                        </span>
                        {selectedDiffSection ? (
                          <span>
                            Hunks {selectedHunks.length} · +{selectedLineStats.added} / -
                            {selectedLineStats.removed}
                          </span>
                        ) : null}
                        {selectedLine ? (
                          <span>
                            Selected L{selectedLine.new_line ?? "?"}
                          </span>
                        ) : null}
                      </div>
                      <div className="flex flex-wrap gap-1">
                        {diffSections.slice(0, 8).map((section) => {
                          const key = `${section.kind}:${section.path}`;
                          const active = selectedDiffKey === key;
                          return (
                            <button
                              key={`${section.kind}:${section.path}`}
                              type="button"
                              onClick={() => {
                                setSelectedDiffKey(key);
                                setSelectedHunkIndex(null);
                                setSelectedDiffLine(null);
                              }}
                              className={`max-w-full truncate rounded border px-1.5 py-0.5 font-mono ${
                                active
                                  ? "border-primary bg-primary text-primary-foreground"
                                  : "border-border/60 bg-card hover:bg-muted"
                              }`}
                              title={section.path}
                            >
                              {section.kind}:{section.path}
                            </button>
                          );
                        })}
                        {diffSections.length > 8 ? (
                          <span className="rounded border border-border/60 bg-card px-1.5 py-0.5">
                            +{diffSections.length - 8}
                          </span>
                        ) : null}
                      </div>
                      {selectedHunks.length ? (
                        <div className="flex flex-wrap gap-1 pt-1">
                          <button
                            type="button"
                            onClick={() => {
                              setSelectedHunkIndex(null);
                              setSelectedDiffLine(null);
                            }}
                            className={`rounded border px-1.5 py-0.5 ${
                              selectedHunkIndex === null
                                ? "border-primary bg-primary text-primary-foreground"
                                : "border-border/60 bg-card hover:bg-muted"
                            }`}
                          >
                            全部
                          </button>
                          {selectedHunks.slice(0, 6).map((hunk, index) => {
                            const active = selectedHunkIndex === index;
                            return (
                              <button
                                key={`${hunk.header}-${index}`}
                                type="button"
                                onClick={() => {
                                  setSelectedHunkIndex(index);
                                  setSelectedDiffLine(null);
                                }}
                                className={`max-w-[220px] truncate rounded border px-1.5 py-0.5 font-mono ${
                                  active
                                    ? "border-primary bg-primary text-primary-foreground"
                                    : "border-border/60 bg-card hover:bg-muted"
                                }`}
                                title={hunk.header}
                              >
                                H{index + 1} {hunk.header}
                              </button>
                            );
                          })}
                          {selectedHunks.length > 6 ? (
                            <span className="rounded border border-border/60 bg-card px-1.5 py-0.5">
                              +{selectedHunks.length - 6}
                            </span>
                          ) : null}
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                  {(discardMutation.error ||
                    discardHunkMutation.error ||
                    discardLineMutation.error ||
                    discardChangeBlockMutation.error) && (
                    <p className="border-t border-border/60 px-3 py-2 text-[11px] leading-5 text-[var(--color-warning)]">
                      {discardMutation.error instanceof Error
                        ? discardMutation.error.message
                        : discardHunkMutation.error instanceof Error
                          ? discardHunkMutation.error.message
                        : discardLineMutation.error instanceof Error
                          ? discardLineMutation.error.message
                        : discardChangeBlockMutation.error instanceof Error
                          ? discardChangeBlockMutation.error.message
                        : "Discard failed"}
                    </p>
                  )}
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
                  <div className="mt-3 rounded-md border border-border/60 bg-card px-2.5 py-2 text-[11px] leading-5 text-muted-foreground">
                    <div className="font-medium text-foreground">Remote sync</div>
                    <div>
                      {canRemoteSync
                        ? "工作区干净，可执行 fast-forward pull 或 push。"
                        : syncDisabledReason ?? "正在读取 remote 状态"}
                    </div>
                    <div className="mt-2 grid gap-2">
                      <input
                        value={remoteUrl}
                        onChange={(event) => setRemoteUrl(event.target.value)}
                        className="h-8 rounded-md border border-border bg-background px-2 text-[11px] outline-none focus:border-primary"
                        placeholder="origin URL: git@github.com:org/buddy-vault.git"
                        aria-label="Buddy Vault origin URL"
                      />
                      <button
                        type="button"
                        onClick={handleSetRemote}
                        disabled={!canSetRemote || remoteMutation.isPending}
                        className="inline-flex h-8 items-center justify-center gap-2 rounded-md border border-border px-2 text-[11px] text-muted-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {remoteMutation.isPending ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <GitBranch className="size-3.5" />
                        )}
                        保存 origin
                      </button>
                    </div>
                  </div>
                  {(pullMutation.error || pushMutation.error || remoteMutation.error) && (
                    <p className="mt-2 text-[11px] leading-5 text-[var(--color-warning)]">
                      {pullMutation.error instanceof Error
                        ? pullMutation.error.message
                        : pushMutation.error instanceof Error
                          ? pushMutation.error.message
                          : remoteMutation.error instanceof Error
                            ? remoteMutation.error.message
                            : "Remote sync failed"}
                    </p>
                  )}
                  {syncMessage && (
                    <p className="mt-2 max-h-20 overflow-auto rounded-md bg-muted px-2 py-1.5 text-[11px] leading-5 text-muted-foreground">
                      {syncMessage}
                    </p>
                  )}
                  {auditEntries.length ? (
                    <div className="mt-3 border-t border-border/60 pt-2">
                      <div className="flex items-center justify-between gap-2 text-[11px]">
                        <span className="font-medium text-foreground">最近 Git 操作</span>
                        <span className="text-muted-foreground">{auditEntries.length}</span>
                      </div>
                      <div className="mt-2 space-y-1">
                        {auditEntries.slice(0, 3).map((entry) => (
                          <GitAuditRow
                            key={`${entry.timestamp_ms}-${entry.operation}-${entry.path ?? ""}`}
                            entry={entry}
                          />
                        ))}
                      </div>
                    </div>
                  ) : null}
                </div>
              </div>
            </div>
          </div>
        </section>

        <section
          id="external-ai"
          className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]"
        >
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

function summarizeDiffLines(lines: VaultGitDiffLine[]): { added: number; removed: number } {
  return lines.reduce(
    (summary, line) => {
      if (line.kind === "add") summary.added += 1;
      if (line.kind === "remove") summary.removed += 1;
      return summary;
    },
    { added: 0, removed: 0 },
  );
}

function isChangeLineKind(kind: string): boolean {
  return kind === "add" || kind === "remove";
}

function changeBlockLines(hunk: VaultGitDiffHunk, lineIndex: number): VaultGitDiffLine[] {
  const line = hunk.lines[lineIndex];
  if (!line || !isChangeLineKind(line.kind)) return [];

  let start = lineIndex;
  while (start > 0 && isChangeLineKind(hunk.lines[start - 1].kind)) {
    start -= 1;
  }
  let end = lineIndex + 1;
  while (end < hunk.lines.length && isChangeLineKind(hunk.lines[end].kind)) {
    end += 1;
  }

  return hunk.lines.slice(start, end);
}

function isStandaloneAddedLine(hunk: VaultGitDiffHunk, lineIndex: number): boolean {
  const line = hunk.lines[lineIndex];
  if (!line || line.kind !== "add") return false;
  return !changeBlockLines(hunk, lineIndex).some((candidate) => candidate.kind === "remove");
}

function isReplacementChangeBlock(hunk: VaultGitDiffHunk, lineIndex: number): boolean {
  const line = hunk.lines[lineIndex];
  if (!line || line.kind !== "add") return false;
  const block = changeBlockLines(hunk, lineIndex);
  return (
    block.some((candidate) => candidate.kind === "add") &&
    block.some((candidate) => candidate.kind === "remove")
  );
}

function isSelectableDiffLine(hunk: VaultGitDiffHunk, lineIndex: number): boolean {
  return isStandaloneAddedLine(hunk, lineIndex) || isReplacementChangeBlock(hunk, lineIndex);
}

function formatDiffHunkPreview(hunk: VaultGitDiffHunk): string {
  return [
    hunk.header,
    ...hunk.lines.map((line) => {
      if (line.kind === "meta") return line.text;
      return `${diffLineMarker(line.kind)}${line.text}`;
    }),
  ].join("\n");
}

function diffLineMarker(kind: string): string {
  if (kind === "add") return "+";
  if (kind === "remove") return "-";
  return " ";
}

function DiffHunkBlock({
  hunk,
  index,
  selectedLine,
  lineSelectionEnabled,
  onSelectLine,
}: {
  hunk: VaultGitDiffHunk;
  index: number;
  selectedLine: SelectedDiffLine | null;
  lineSelectionEnabled: boolean;
  onSelectLine: (lineIndex: number) => void;
}) {
  return (
    <div className="border-b border-border/60 last:border-b-0">
      <div className="flex items-center gap-2 bg-muted/60 px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
        <span className="rounded border border-border/60 bg-background px-1.5 py-0.5">
          H{index + 1}
        </span>
        <span className="min-w-0 truncate">{hunk.header}</span>
      </div>
      <div className="font-mono">
        {hunk.lines.map((line, lineIndex) => (
          <DiffLineRow
            key={`${line.kind}-${line.old_line ?? ""}-${line.new_line ?? ""}-${lineIndex}`}
            line={line}
            selected={
              selectedLine?.hunkIndex === index && selectedLine.lineIndex === lineIndex
            }
            selectable={lineSelectionEnabled && isSelectableDiffLine(hunk, lineIndex)}
            onSelect={() => onSelectLine(lineIndex)}
          />
        ))}
      </div>
    </div>
  );
}

function DiffLineRow({
  line,
  selected,
  selectable,
  onSelect,
}: {
  line: VaultGitDiffLine;
  selected: boolean;
  selectable: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={selectable ? onSelect : undefined}
      aria-pressed={selected}
      aria-disabled={!selectable}
      className={`grid w-full grid-cols-[42px_42px_20px_minmax(0,1fr)] border-t border-border/30 text-left leading-5 ${diffLineClass(line.kind)} ${
        selected ? "ring-1 ring-inset ring-primary" : ""
      } ${selectable ? "cursor-pointer hover:bg-primary/10" : "cursor-default"}`}
    >
      <span className="select-none border-r border-border/40 px-2 text-right text-muted-foreground">
        {line.old_line ?? ""}
      </span>
      <span className="select-none border-r border-border/40 px-2 text-right text-muted-foreground">
        {line.new_line ?? ""}
      </span>
      <span className="select-none px-1 text-center text-muted-foreground">
        {diffLineMarker(line.kind)}
      </span>
      <code className="min-w-0 whitespace-pre-wrap break-words pr-3">
        {line.text || " "}
      </code>
    </button>
  );
}

function diffLineClass(kind: string): string {
  if (kind === "add") {
    return "bg-[color:var(--color-diff-added)]/15 text-[color:var(--color-diff-added-word)]";
  }
  if (kind === "remove") {
    return "bg-[color:var(--color-diff-removed)]/15 text-[color:var(--color-diff-removed-word)]";
  }
  return "bg-background/70";
}

function GitAuditRow({ entry }: { entry: VaultGitAuditEntry }) {
  return (
    <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 rounded bg-muted/40 px-2 py-1.5 text-[11px]">
      <span className="rounded border border-border/60 bg-background px-1.5 py-0.5 font-mono text-muted-foreground">
        {gitAuditLabel(entry.operation)}
      </span>
      <span className="min-w-0 truncate text-foreground">
        {entry.path || entry.commit || entry.remote || entry.summary}
      </span>
      <span className="shrink-0 text-muted-foreground">
        {formatAuditTime(entry.timestamp_ms)}
      </span>
    </div>
  );
}

function gitAuditLabel(operation: string): string {
  if (operation === "discard-change-block") return "block";
  if (operation === "discard-line") return "line";
  if (operation === "discard-hunk") return "hunk";
  if (operation === "discard-path") return "discard";
  if (operation === "commit") return "commit";
  if (operation === "remote") return "remote";
  return operation;
}

function formatAuditTime(timestampMs: number): string {
  return new Date(timestampMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
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

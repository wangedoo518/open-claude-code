/**
 * Inbox · CCD 权限确认 + 任务审阅 (wireframes.html §07, SOUL ③+④)
 *
 * S4 MVP implementation. The canonical §7 wireframe shows a
 * SubagentPanel-style tree of tool calls for each maintainer task,
 * with per-step approve/reject buttons. S4 lands the *data pipeline*
 * end of that vision:
 *
 *   1. Every raw entry that lands under ~/.clawwiki/raw/ triggers a
 *      pending `new-raw` inbox task (see S4.2 desktop-server side-
 *      channel in ingest_wiki_raw_handler).
 *   2. This page lists those tasks, lets the user approve or reject
 *      each one, and reflects the status change across the UI.
 *   3. Sidebar badge shows live pending count.
 *
 * What's NOT in S4 MVP (deferred to the sprint that wires
 * codex_broker::chat_completion):
 *   - The actual Maintainer LLM run that produces the wiki page
 *   - MaintainerTaskTree visualization of the LLM's tool calls
 *   - Diff preview of the proposed wiki/ writes
 *
 * The component is intentionally structured as a LEFT list + RIGHT
 * detail pane so that when the LLM run lands we can stuff a
 * `<MaintainerTaskTree entry={...} />` into the right pane without
 * touching the page container.
 */

import { useEffect, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Loader2,
  Inbox as InboxIcon,
  CheckCircle2,
  XCircle,
  AlertCircle,
  Clock,
  FileText,
  ArrowRight,
  Sparkles,
  Save,
} from "lucide-react";
import { Link } from "react-router-dom";
import {
  approveInboxWithWrite,
  listInboxEntries,
  proposeForInboxEntry,
  resolveInboxEntry,
} from "@/features/ingest/persist";
import type {
  InboxEntry,
  InboxResolveAction,
  WikiPageProposal,
} from "@/features/ingest/types";

const inboxKeys = {
  list: () => ["wiki", "inbox", "list"] as const,
};

/** 翻译 inbox entry kind */
function translateKind(kind: string): string {
  const map: Record<string, string> = {
    "new-raw": "新素材",
    "stale": "待更新",
    "conflict": "冲突",
  };
  return map[kind] ?? kind;
}

/** 翻译 inbox entry status */
function translateStatus(status: string): string {
  const map: Record<string, string> = {
    "pending": "待处理",
    "approved": "已批准",
    "rejected": "已拒绝",
  };
  return map[status] ?? status;
}

export function InboxPage() {
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const listQuery = useQuery({
    queryKey: inboxKeys.list(),
    queryFn: () => listInboxEntries(),
    staleTime: 10_000,
    refetchInterval: 15_000,
  });

  const entries = listQuery.data?.entries ?? [];
  const selectedEntry = useMemo(
    () =>
      selectedId !== null
        ? (entries.find((e) => e.id === selectedId) ?? null)
        : null,
    [entries, selectedId],
  );

  // Review nit #5: clear the selection when a previously-selected
  // entry drops off the list (e.g. after a reject-then-archive flow,
  // or if the backing file got pruned out from under us). Without
  // this effect the right pane would get stuck on a stale id and the
  // EntryPlaceholder wouldn't re-appear.
  useEffect(() => {
    if (selectedId === null) return;
    if (listQuery.isLoading) return;
    const stillExists = entries.some((e) => e.id === selectedId);
    if (!stillExists) {
      setSelectedId(null);
    }
  }, [entries, selectedId, listQuery.isLoading]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Page head */}
      <div className="flex shrink-0 items-center justify-between border-b border-border/50 px-6 py-4">
        <div>
          <h1
            className="text-foreground"
            style={{ fontSize: 18, fontWeight: 600, fontFamily: "var(--font-serif, Lora, serif)" }}
          >
            Inbox
          </h1>
          <p className="mt-1 text-muted-foreground/60" style={{ fontSize: 11 }}>
            新素材自动入队 -- AI 生成知识页面 -- 审批后写入 Wiki
          </p>
        </div>
        <div className="flex items-center gap-2" style={{ fontSize: 11 }}>
          <span
            className="rounded-full border border-border/40 px-2 py-0.5 text-muted-foreground"
            style={{ color: "var(--color-warning)" }}
          >
            {listQuery.data?.pending_count ?? 0} 待处理
          </span>
          <span className="text-muted-foreground/40">
            {listQuery.data?.total_count ?? 0} 总计
          </span>
        </div>
      </div>

      {/* Body: split pane */}
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[360px] shrink-0 flex-col overflow-hidden border-r border-border/50">
          <EntryList
            entries={entries}
            isLoading={listQuery.isLoading}
            error={listQuery.error}
            selectedId={selectedId}
            onSelect={(id) => setSelectedId(id)}
          />
        </aside>
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {selectedEntry ? (
            <EntryDetail entry={selectedEntry} />
          ) : (
            <EntryPlaceholder />
          )}
        </main>
      </div>
    </div>
  );
}

/* ─── Entry list ───────────────────────────────────────────────── */

function EntryList({
  entries,
  isLoading,
  error,
  selectedId,
  onSelect,
}: {
  entries: InboxEntry[];
  isLoading: boolean;
  error: Error | null;
  selectedId: number | null;
  onSelect: (id: number) => void;
}) {
  if (isLoading) {
    return (
      <div className="flex-1 px-3 py-6 text-center text-caption text-muted-foreground">
        <Loader2 className="mx-auto mb-1.5 size-4 animate-spin" />
        加载收件箱…
      </div>
    );
  }
  if (error) {
    return (
      <div
        className="m-3 rounded-md border px-3 py-2 text-caption"
        style={{
          borderColor:
            "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor:
            "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        加载收件箱失败：{error.message}
      </div>
    );
  }
  if (entries.length === 0) {
    return (
      <div className="flex-1 px-4 py-8 text-center text-caption text-muted-foreground">
        <InboxIcon className="mx-auto mb-2 size-6 opacity-40" />
        <div>暂无维护任务。</div>
        <div className="mt-1 text-caption text-muted-foreground/70">
          在{" "}
          <Link to="/raw" className="text-primary hover:underline">
            素材库
          </Link>{" "}
          中入库一条素材来生成你的第一个任务。
        </div>
      </div>
    );
  }

  // Sort: pending first, then newest first. Wrapped in useMemo so
  // we don't re-sort on every render; React Query triggers a fresh
  // `entries` reference only when the underlying data actually
  // changes, so `useMemo([entries])` is sufficient.
  const sorted = useMemo(
    () =>
      [...entries].sort((a, b) => {
        if (a.status === "pending" && b.status !== "pending") return -1;
        if (b.status === "pending" && a.status !== "pending") return 1;
        return b.id - a.id;
      }),
    [entries],
  );

  return (
    <ul className="flex-1 divide-y divide-border/30 overflow-y-auto">
      {sorted.map((entry) => {
        const isActive = entry.id === selectedId;
        return (
          <li key={entry.id}>
            <button
              type="button"
              onClick={() => onSelect(entry.id)}
              className={
                "w-full px-4 py-2.5 text-left transition-colors hover:bg-accent/30 " +
                (isActive ? "border-l-[3px] border-l-primary" : "border-l-[3px] border-l-transparent")
              }
            >
              <div className="flex items-center justify-between gap-2">
                <StatusIcon status={entry.status} />
                <span
                  className="flex-1 truncate text-foreground"
                  style={{ fontSize: 13, fontWeight: isActive ? 500 : 400 }}
                >
                  {entry.title.replace(/^New raw entry/, "新素材")}
                </span>
                <span className="shrink-0 text-muted-foreground/50" style={{ fontSize: 11 }}>
                  {translateKind(entry.kind)}
                </span>
              </div>
              <div className="mt-1 truncate pl-6 text-muted-foreground/60" style={{ fontSize: 11 }}>
                {entry.description}
              </div>
              <div className="mt-0.5 flex items-center gap-2 pl-6 text-muted-foreground/40" style={{ fontSize: 11 }}>
                <Clock className="size-3" />
                {formatRelative(entry.created_at)}
              </div>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function StatusIcon({ status }: { status: InboxEntry["status"] }) {
  if (status === "pending") {
    return (
      <AlertCircle
        className="size-4 shrink-0"
        style={{ color: "var(--color-warning)" }}
      />
    );
  }
  if (status === "approved") {
    return (
      <CheckCircle2
        className="size-4 shrink-0"
        style={{ color: "var(--color-success)" }}
      />
    );
  }
  return (
    <XCircle
      className="size-4 shrink-0"
      style={{ color: "var(--color-error)" }}
    />
  );
}

/* ─── Detail pane ──────────────────────────────────────────────── */

function EntryDetail({ entry }: { entry: InboxEntry }) {
  const queryClient = useQueryClient();

  // Proposal state is scoped to the currently-selected entry so that
  // switching to a different entry clears the preview (a proposal for
  // entry #3 should not leak into the view for entry #4). We key the
  // reset effect on `entry.id`.
  const [proposal, setProposal] = useState<WikiPageProposal | null>(null);
  useEffect(() => {
    setProposal(null);
  }, [entry.id]);

  // Quick approve/reject without writing anything (backwards compat).
  const resolveMutation = useMutation({
    mutationFn: (action: InboxResolveAction) => resolveInboxEntry(entry.id, action),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
  });

  // Fire the maintainer proposal: one chat_completion through the
  // Codex broker. This does NOT mutate the inbox entry — the user
  // still has to click "Approve & Write Wiki Page" or "Reject" next.
  const proposeMutation = useMutation({
    mutationFn: () => proposeForInboxEntry(entry.id),
    onSuccess: (data) => {
      setProposal(data.proposal);
    },
  });

  // Persist the proposal as a wiki page AND resolve the inbox entry
  // as approved in one atomic-ish step (write first, then resolve;
  // worst case a half-failure leaves the page on disk and the user
  // can retry the approve from the UI).
  const writeMutation = useMutation({
    mutationFn: (p: WikiPageProposal) => approveInboxWithWrite(entry.id, p),
    onSuccess: () => {
      setProposal(null);
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
  });

  const isResolved = entry.status !== "pending";
  const canMaintain =
    !isResolved &&
    entry.kind === "new-raw" &&
    entry.source_raw_id != null &&
    proposal === null;

  const anyPending =
    resolveMutation.isPending || proposeMutation.isPending || writeMutation.isPending;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <div className="flex items-center gap-2">
          <StatusIcon status={entry.status} />
          <span className="font-mono text-muted-foreground/40" style={{ fontSize: 11 }}>
            #{String(entry.id).padStart(5, "0")}
          </span>
          <span className="text-muted-foreground/50" style={{ fontSize: 11 }}>
            {entry.kind}
          </span>
          <StatusPill status={entry.status} />
        </div>
        <h2
          className="mt-2 text-foreground"
          style={{ fontSize: 18, fontWeight: 600, fontFamily: "var(--font-serif, Lora, serif)" }}
        >
          {entry.title}
        </h2>
        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-muted-foreground/40" style={{ fontSize: 11 }}>
          <span>创建于: {entry.created_at}</span>
          {entry.resolved_at && <span>处理于: {entry.resolved_at}</span>}
          {entry.source_raw_id != null && (
            <Link
              to="/raw"
              className="inline-flex items-center gap-1 text-primary hover:underline"
              style={{ fontSize: 11 }}
            >
              <FileText className="size-3" />
              raw #{String(entry.source_raw_id).padStart(5, "0")}
              <ArrowRight className="size-3" />
            </Link>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-auto px-6 py-5">
        <h3 className="mb-2 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
          描述
        </h3>
        <p className="whitespace-pre-wrap text-foreground/90" style={{ fontSize: 14, lineHeight: 1.6 }}>
          {entry.description
            .replace(/^Raw entry/, "素材")
            .replace("was ingested from WeChat user", "由微信用户")
            .replace("Proposed action: summarise into a concept page.", "转发入库。建议操作：总结为概念知识页面。")
          }
        </p>

        {/* ── Maintainer proposal preview ───────────────────────── */}
        {proposal ? (
          <>
            <h3 className="mb-2 mt-6 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
              生成的知识页面
            </h3>
            <ProposalPreview proposal={proposal} />
          </>
        ) : canMaintain ? (
          <>
            <h3 className="mb-2 mt-6 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
              维护器
            </h3>
            <div className="rounded-md border border-border/40 px-4 py-5">
              <div className="mb-2 text-foreground/90" style={{ fontSize: 14, lineHeight: 1.6 }}>
                让维护 AI 将这条素材总结为概念知识页面。
              </div>
              <div className="mb-3 text-muted-foreground/50" style={{ fontSize: 11 }}>
                调用一次 AI 总结（≤200 词）。在你批准之前不会写入磁盘。
              </div>
              <button
                type="button"
                onClick={() => proposeMutation.mutate()}
                disabled={anyPending}
                className="flex items-center gap-1.5 rounded-md bg-primary px-4 py-1.5 text-body-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
              >
                {proposeMutation.isPending ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : (
                  <Sparkles className="size-3" />
                )}
                开始维护
              </button>
            </div>
          </>
        ) : (
          <>
            <h3 className="mb-2 mt-6 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
              维护任务树
            </h3>
            <div className="rounded-md border border-border/40 px-4 py-5 text-center">
              <div className="mb-1 text-muted-foreground/60" style={{ fontSize: 13 }}>
                任务树可视化即将上线。
              </div>
              <div className="text-muted-foreground/40" style={{ fontSize: 11 }}>
                目前支持按条目级别批准或拒绝。
              </div>
            </div>
          </>
        )}

        {/* ── Error strip: surfaces propose/write/resolve errors ── */}
        {(proposeMutation.error || writeMutation.error || resolveMutation.error) && (
          <div
            className="mt-4 rounded-md border px-3 py-2 text-caption"
            style={{
              borderColor:
                "color-mix(in srgb, var(--color-error) 30%, transparent)",
              backgroundColor:
                "color-mix(in srgb, var(--color-error) 5%, transparent)",
              color: "var(--color-error)",
            }}
          >
            {proposeMutation.error && (
              <div>生成失败：{String(proposeMutation.error)}</div>
            )}
            {writeMutation.error && (
              <div>写入失败：{String(writeMutation.error)}</div>
            )}
            {resolveMutation.error && (
              <div>处理失败：{String(resolveMutation.error)}</div>
            )}
          </div>
        )}
      </div>

      <div className="shrink-0 border-t border-border/50 px-6 py-3">
        {isResolved ? (
          <div className="flex items-center justify-between gap-3">
            <div className="text-caption text-muted-foreground">
              该任务已{translateStatus(entry.status)}。
            </div>
          </div>
        ) : (
          <div className="flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={() => resolveMutation.mutate("reject")}
              disabled={anyPending}
              className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-body-sm font-medium text-muted-foreground transition-colors hover:border-destructive hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
            >
              <XCircle className="size-3" />
              拒绝
            </button>
            {proposal ? (
              <button
                type="button"
                onClick={() => writeMutation.mutate(proposal)}
                disabled={anyPending}
                className="flex items-center gap-1.5 rounded-md bg-primary px-4 py-1.5 text-body-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
              >
                {writeMutation.isPending ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : (
                  <Save className="size-3" />
                )}
                批准并写入知识页面
              </button>
            ) : (
              <button
                type="button"
                onClick={() => resolveMutation.mutate("approve")}
                disabled={anyPending}
                className="flex items-center gap-1.5 rounded-md bg-primary px-4 py-1.5 text-body-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
              >
                {resolveMutation.isPending ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : (
                  <CheckCircle2 className="size-3" />
                )}
                批准
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Render a `WikiPageProposal` as a reviewable card: slug + title +
 * summary + body preview. Hard-pins the body to pre-wrap so the
 * LLM's newlines survive, and caps the visible body at a reasonable
 * height (the whole body is always there, just in an internal
 * scrollable region so the detail pane doesn't jump in size).
 */
function ProposalPreview({ proposal }: { proposal: WikiPageProposal }) {
  return (
    <div className="rounded-md border border-border/40 bg-background">
      <div className="flex items-start justify-between gap-2 border-b border-border/30 px-4 py-3">
        <div className="flex-1">
          <div className="flex items-center gap-2 text-muted-foreground/40" style={{ fontSize: 11 }}>
            <span className="font-mono">
              {proposal.slug}
            </span>
            <span>
              from raw #{String(proposal.source_raw_id).padStart(5, "0")}
            </span>
          </div>
          <div
            className="mt-1.5 text-foreground"
            style={{ fontSize: 16, fontWeight: 600, fontFamily: "var(--font-serif, Lora, serif)" }}
          >
            {proposal.title}
          </div>
          <div className="mt-1 text-muted-foreground/60" style={{ fontSize: 12 }}>
            {proposal.summary}
          </div>
        </div>
      </div>
      <div className="max-h-64 overflow-auto px-4 py-3">
        <pre className="whitespace-pre-wrap text-body-sm text-foreground/90">
          {proposal.body}
        </pre>
      </div>
    </div>
  );
}

function StatusPill({ status }: { status: InboxEntry["status"] }) {
  const config = {
    pending: { label: "待处理", color: "var(--color-warning)" },
    approved: { label: "已批准", color: "var(--color-success)" },
    rejected: { label: "已拒绝", color: "var(--color-error)" },
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

function EntryPlaceholder() {
  return (
    <div className="flex flex-1 items-center justify-center p-6 text-center">
      <div className="max-w-sm">
        <InboxIcon className="mx-auto mb-2 size-8 opacity-30" />
        <p className="text-body text-muted-foreground">
          选择左侧任务进行审阅。
        </p>
      </div>
    </div>
  );
}

/* ─── Time formatting ──────────────────────────────────────────── */

function formatRelative(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;
  const deltaSecs = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (deltaSecs < 60) return `${deltaSecs}秒前`;
  if (deltaSecs < 3600) return `${Math.floor(deltaSecs / 60)}分钟前`;
  if (deltaSecs < 86_400) return `${Math.floor(deltaSecs / 3600)}小时前`;
  return `${Math.floor(deltaSecs / 86_400)}天前`;
}

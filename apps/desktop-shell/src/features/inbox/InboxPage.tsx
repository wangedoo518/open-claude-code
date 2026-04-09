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
} from "lucide-react";
import { Link } from "react-router-dom";
import { listInboxEntries, resolveInboxEntry } from "@/features/ingest/persist";
import type {
  InboxEntry,
  InboxResolveAction,
} from "@/features/ingest/types";

const inboxKeys = {
  list: () => ["wiki", "inbox", "list"] as const,
};

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
      <div className="flex shrink-0 items-start gap-3 border-b border-border/50 px-6 py-4">
        <div className="text-xl">📨</div>
        <div className="flex-1">
          <h1
            className="text-head font-semibold text-foreground"
            style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
          >
            Inbox · Maintenance Inbox
          </h1>
          <p className="mt-0.5 text-label text-muted-foreground">
            CCD 灵魂 ③+④ · Maintainer 提的待审任务 · approve / reject · S4 MVP：
            新 raw 自动入队；完整 TaskTree + LLM 写回 delay 到 codex_broker 接通后
          </p>
        </div>
        <div className="flex items-center gap-1.5 text-caption text-muted-foreground">
          <span
            className="rounded-md border border-border bg-background px-1.5 py-0.5"
            style={{ color: "var(--color-warning)" }}
          >
            {listQuery.data?.pending_count ?? 0} pending
          </span>
          <span className="rounded-md border border-border bg-background px-1.5 py-0.5">
            {listQuery.data?.total_count ?? 0} total
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
        Loading inbox…
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
        Failed to list inbox: {error.message}
      </div>
    );
  }
  if (entries.length === 0) {
    return (
      <div className="flex-1 px-4 py-8 text-center text-caption text-muted-foreground">
        <InboxIcon className="mx-auto mb-2 size-6 opacity-40" />
        <div>No maintainer tasks yet.</div>
        <div className="mt-1 text-caption text-muted-foreground/70">
          Ingest a raw entry from the{" "}
          <Link to="/raw" className="text-primary hover:underline">
            Raw Library
          </Link>{" "}
          to generate your first task.
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
    <ul className="flex-1 divide-y divide-border/40 overflow-y-auto">
      {sorted.map((entry) => {
        const isActive = entry.id === selectedId;
        return (
          <li key={entry.id}>
            <button
              type="button"
              onClick={() => onSelect(entry.id)}
              className={
                "w-full px-4 py-2.5 text-left transition-colors " +
                (isActive ? "bg-primary/10" : "hover:bg-accent/40")
              }
            >
              <div className="flex items-center justify-between gap-2">
                <StatusIcon status={entry.status} />
                <span className="flex-1 truncate text-body-sm font-medium text-foreground">
                  {entry.title}
                </span>
                <span className="shrink-0 rounded-sm bg-muted/40 px-1 text-caption text-muted-foreground">
                  {entry.kind}
                </span>
              </div>
              <div className="mt-1 truncate pl-6 text-caption text-muted-foreground/80">
                {entry.description}
              </div>
              <div className="mt-0.5 flex items-center gap-2 pl-6 text-caption text-muted-foreground/60">
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
  const mutation = useMutation({
    mutationFn: (action: InboxResolveAction) => resolveInboxEntry(entry.id, action),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
  });

  const isResolved = entry.status !== "pending";

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="shrink-0 border-b border-border/50 bg-muted/10 px-6 py-4">
        <div className="flex items-center gap-2">
          <StatusIcon status={entry.status} />
          <span className="font-mono text-caption text-muted-foreground">
            #{String(entry.id).padStart(5, "0")}
          </span>
          <span className="rounded-sm bg-muted/40 px-1 text-caption text-muted-foreground">
            {entry.kind}
          </span>
          <StatusPill status={entry.status} />
        </div>
        <h2
          className="mt-1.5 text-subhead font-semibold text-foreground"
          style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
        >
          {entry.title}
        </h2>
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-caption text-muted-foreground">
          <span>created: {entry.created_at}</span>
          {entry.resolved_at && <span>resolved: {entry.resolved_at}</span>}
          {entry.source_raw_id != null && (
            <Link
              to="/raw"
              className="inline-flex items-center gap-1 text-primary hover:underline"
            >
              <FileText className="size-3" />
              raw #{String(entry.source_raw_id).padStart(5, "0")}
              <ArrowRight className="size-3" />
            </Link>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-auto px-6 py-4">
        <h3 className="mb-2 text-caption font-semibold uppercase tracking-wide text-muted-foreground">
          Description
        </h3>
        <p className="whitespace-pre-wrap text-body text-foreground/90">
          {entry.description}
        </p>

        <h3 className="mb-2 mt-5 text-caption font-semibold uppercase tracking-wide text-muted-foreground">
          Maintainer Task Tree
        </h3>
        <div className="rounded-md border border-border/50 bg-muted/10 px-4 py-6 text-center text-caption text-muted-foreground">
          <div className="mb-1 text-body-sm text-muted-foreground">
            🌲 TaskTree visualization lands once codex_broker wires
            chat_completion.
          </div>
          <div className="text-caption text-muted-foreground/60">
            For now the task is approved/rejected at the entry level —
            one button per task, not per tool call.
          </div>
        </div>

        {mutation.error && (
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
            Failed to resolve: {String(mutation.error)}
          </div>
        )}
      </div>

      <div className="shrink-0 border-t border-border/50 bg-muted/5 px-6 py-3">
        {isResolved ? (
          <div className="flex items-center justify-between gap-3">
            <div className="text-caption text-muted-foreground">
              This task has already been {entry.status}.
            </div>
          </div>
        ) : (
          <div className="flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={() => mutation.mutate("reject")}
              disabled={mutation.isPending}
              className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-body-sm font-medium text-muted-foreground transition-colors hover:border-destructive hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
            >
              <XCircle className="size-3" />
              Reject
            </button>
            <button
              type="button"
              onClick={() => mutation.mutate("approve")}
              disabled={mutation.isPending}
              className="flex items-center gap-1.5 rounded-md bg-primary px-4 py-1.5 text-body-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
            >
              {mutation.isPending ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <CheckCircle2 className="size-3" />
              )}
              Approve
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function StatusPill({ status }: { status: InboxEntry["status"] }) {
  const config = {
    pending: { label: "pending", color: "var(--color-warning)" },
    approved: { label: "approved", color: "var(--color-success)" },
    rejected: { label: "rejected", color: "var(--color-error)" },
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
          Select a task on the left to review it.
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
  if (deltaSecs < 60) return `${deltaSecs}s ago`;
  if (deltaSecs < 3600) return `${Math.floor(deltaSecs / 60)}m ago`;
  if (deltaSecs < 86_400) return `${Math.floor(deltaSecs / 3600)}h ago`;
  return `${Math.floor(deltaSecs / 86_400)}d ago`;
}

/**
 * Inbox · CCD 权限确认 + 任务审阅
 *
 * Left: pending-first task list. Right (W1): the Maintainer Workbench,
 * a three-section pane — §1 Evidence, §2 Maintain, §3 Result — that
 * replaces the flat Propose→Approve detail pane. The legacy two-step
 * flow is preserved as a collapsible fallback inside §2.
 *
 * Deep-link UX: `?task=N` is the source of truth for the focused
 * entry. Selection is driven by `useDeepLinkState`; a persistent
 * `DeepLinkFocusChip` and `DeepLinkNotFoundBanner` handle reverse-sync
 * paste, missing targets, and explicit dismissal.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient, useQueries } from "@tanstack/react-query";
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
  CheckSquare,
  CheckSquare2,
  Square,
  X,
  HelpCircle,
  Users,
} from "lucide-react";
import { Link, useNavigate } from "react-router-dom";
import {
  approveInboxWithWrite,
  listInboxEntries,
  proposeForInboxEntry,
  resolveInboxEntry,
} from "@/features/ingest/persist";
import type {
  InboxEntry,
  InboxResolveAction,
  MaintainAction,
  MaintainOutcome,
  MaintainResponse,
  UpdateProposal,
  WikiPageProposal,
} from "@/features/ingest/types";
import type { IngestDecision } from "@/lib/tauri";
import {
  applyProposal,
  cancelProposal,
  createProposal,
  fetchRawById,
  maintainInboxEntry,
} from "@/lib/tauri";
import { parsePositiveInt, useDeepLinkState } from "@/lib/deep-link";
import {
  CopyDeepLinkButton,
  DeepLinkFocusChip,
  DeepLinkNotFoundBanner,
} from "@/components/deep-link";
import { EmptyState } from "@/components/ui/empty-state";
import { FailureBanner } from "@/components/ui/failure-banner";
import { InfoTooltip } from "@/components/ui/info-tooltip";
import { IngestDecisionBadge } from "@/features/inbox/components/IngestDecisionBadge";
import { URLTrackBadge } from "@/features/inbox/components/URLTrackBadge";
import { BodyPreviewPanel } from "@/features/inbox/components/BodyPreviewPanel";
import { MaintainActionRadio } from "@/features/inbox/components/MaintainActionRadio";
import { MaintainerResultCard } from "@/features/inbox/components/MaintainerResultCard";
import { WikiPageDiffPreview } from "@/features/inbox/components/WikiPageDiffPreview";
import { RecommendedActionBadge } from "@/features/inbox/components/RecommendedActionBadge";
import { QueueGroupHeader } from "@/features/inbox/components/QueueGroupHeader";
import { BatchActionsToolbar } from "@/features/inbox/components/BatchActionsToolbar";
import {
  CombinedPreviewDialog,
  type CombinedApplyResponse,
} from "@/features/inbox/components/CombinedPreviewDialog";
import { TargetCandidatePicker } from "@/features/inbox/components/TargetCandidatePicker";
import { DuplicateGuardBanner } from "@/features/inbox/components/DuplicateGuardBanner";
import { DuplicateGuardDialog } from "@/features/inbox/components/DuplicateGuardDialog";
import { SharedTargetBadge } from "@/features/inbox/components/SharedTargetBadge";
import { InboxLineageSummary } from "@/features/inbox/components/InboxLineageSummary";
import {
  computeQueueIntelligence,
  groupAndSortByAction,
  markCohorts,
  type QueueIntelligence,
} from "@/features/inbox/queue-intelligence";
import type { TargetCandidate } from "@/lib/tauri";
import { fetchInboxCandidates } from "@/features/ingest/persist";

/** Q2 — Layer 1 duplicate-guard trigger threshold (inclusive). */
const DUPLICATE_GUARD_SCORE_THRESHOLD = 75;

// Worker C integration — the real WikiPageSearchField picker for
// update_existing and the navigateToWikiPage handoff for the
// MaintainerResultCard "打开 Wiki 页" CTA. Both live under
// `features/wiki/` (Worker C's canonical home for wiki-global
// primitives). The components degrade gracefully if props/ctx are
// missing, so no null-sentinel is needed here.
import { WikiPageSearchField } from "@/features/wiki/WikiPageSearchField";
import {
  buildAskBindUrl,
  navigateToWikiPage,
} from "@/features/wiki/navigate-helpers";

/** InboxEntry enriched with the queue-intelligence envelope + decision. */
type IntelligentEntry = InboxEntry & {
  intelligence: QueueIntelligence;
  decision: IngestDecision | null;
};

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
  const queryClient = useQueryClient();

  // URL is the source of truth for the focused task. The hook handles
  // lazy init from ?task=N, reverse-sync on external URL changes
  // (paste, link click, back button), and URL writes via replace.
  const [selectedId, setSelectedId] = useDeepLinkState(
    "task",
    parsePositiveInt,
  );

  // Q1 sprint — batch mode toggle. When on, left-side rows sprout
  // checkboxes and the right pane renders `<BatchActionsToolbar />`
  // instead of the per-entry Workbench.
  const [batchMode, setBatchMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  // W3 sprint — CombinedPreviewDialog open state. The dialog receives
  // the derived `mergeTargetSlug` + selected ids so it can fan out to
  // Worker A's /combined-proposal endpoint on mount.
  const [combinedOpen, setCombinedOpen] = useState(false);

  const listQuery = useQuery({
    queryKey: inboxKeys.list(),
    queryFn: () => listInboxEntries(),
    staleTime: 10_000,
    refetchInterval: 15_000,
  });

  const entries = listQuery.data?.entries ?? [];

  // Batch-fetch raw detail for every entry carrying a source_raw_id.
  // We only need `last_ingest_decision` from each raw to feed the
  // queue-intelligence rules r3-r5 (content_duplicate / reused_*).
  // `useQueries` parallelises the fetches behind one React Query key
  // per raw id so the cache is shared with the detail pane's own
  // raw fetch — no duplicate network traffic on selection.
  const rawIds = useMemo(() => {
    const ids: number[] = [];
    for (const entry of entries) {
      if (entry.source_raw_id != null) ids.push(entry.source_raw_id);
    }
    return Array.from(new Set(ids));
  }, [entries]);

  const rawQueries = useQueries({
    queries: rawIds.map((id) => ({
      queryKey: ["raw", id],
      queryFn: () => fetchRawById(id),
      staleTime: 60_000,
    })),
  });

  /** id → IngestDecision (when present + well-formed). */
  const decisionByRawId = useMemo(() => {
    const map = new Map<number, IngestDecision>();
    rawQueries.forEach((query, idx) => {
      const data = query.data;
      const rawId = rawIds[idx];
      if (rawId == null) return;
      const candidate = data?.entry?.last_ingest_decision;
      if (
        candidate &&
        typeof candidate === "object" &&
        "kind" in candidate &&
        typeof (candidate as { kind?: unknown }).kind === "string"
      ) {
        map.set(rawId, candidate as IngestDecision);
      }
    });
    return map;
  }, [rawQueries, rawIds]);

  // Enrich entries with intelligence — always recompute when entries
  // or decisions change; the function is cheap (~7 if-branches per
  // entry) and the result feeds grouping / selection pre-seeding.
  const intelligentEntries: IntelligentEntry[] = useMemo(() => {
    const enriched = entries.map<IntelligentEntry>((entry) => {
      const decision =
        entry.source_raw_id != null
          ? decisionByRawId.get(entry.source_raw_id) ?? null
          : null;
      return {
        ...entry,
        intelligence: computeQueueIntelligence(entry, decision),
        decision,
      };
    });
    markCohorts(enriched);
    return enriched;
  }, [entries, decisionByRawId]);

  const selectedEntry = useMemo(
    () =>
      selectedId !== null
        ? (intelligentEntries.find((e) => e.id === selectedId) ?? null)
        : null,
    [intelligentEntries, selectedId],
  );

  // Q2 — per-slug count across the whole queue. A row's
  // `SharedTargetBadge` only lights up when `count > 1`. We build the
  // map once per enrichment pass so every row reads an O(1) lookup.
  const sharedTargetCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const entry of intelligentEntries) {
      const slug = entry.intelligence.target_candidate?.slug;
      if (slug) {
        counts.set(slug, (counts.get(slug) ?? 0) + 1);
      }
    }
    return counts;
  }, [intelligentEntries]);

  // Focus state: renders to chip/banner/placeholder in a mutually
  // exclusive way. Unlike the pre-F2 silent-clear useEffect, we never
  // mutate URL/state behind the user's back — the "missing" branch is
  // a visible banner with an explicit dismiss button.
  //   none     → ?task absent → baseline placeholder
  //   loading  → ?task present but list still resolving (don't flash)
  //   focused  → target exists → show EntryDetail + focus chip
  //   missing  → list finished, target id not in entries → banner
  let focusState: "none" | "loading" | "focused" | "missing";
  if (selectedId === null) {
    focusState = "none";
  } else if (listQuery.isLoading) {
    focusState = "loading";
  } else if (listQuery.isError) {
    // Errors are already surfaced inside EntryList; don't double-render
    // a missing banner on top of a list-level error strip.
    focusState = "none";
  } else if (selectedEntry) {
    focusState = "focused";
  } else {
    focusState = "missing";
  }

  const selectedIdLabel =
    selectedId !== null ? `#${String(selectedId).padStart(5, "0")}` : "";

  // Scroll deep-linked task into view on initial mount only. NOT
  // dependent on selectedId — the hook's reverse-sync updates selection
  // on same-page URL pastes, and scrolling there would jitter every
  // click-to-select. Mount-time covers the primary deep-link UX
  // (external link → fresh mount → scroll to target).
  useEffect(() => {
    if (selectedId !== null) {
      requestAnimationFrame(() => {
        const el = document.getElementById(`inbox-task-${selectedId}`);
        el?.scrollIntoView({ behavior: "smooth", block: "nearest" });
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toggleSelected = useCallback((id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const bulkSelect = useCallback((ids: number[], on: boolean) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  }, []);

  const handleToggleBatchMode = useCallback(() => {
    setBatchMode((prev) => {
      const next = !prev;
      if (next) {
        // Entering batch mode: drop the per-entry focus so the detail
        // pane collapses cleanly into the toolbar surface.
        setSelectedId(null);
      } else {
        // Leaving batch mode: clear the multi-selection so the
        // checkboxes don't persist silently across the toggle.
        setSelectedIds(new Set());
      }
      return next;
    });
  }, [setSelectedId]);

  const clearBatchSelection = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  const selectedIdList = useMemo(
    () => Array.from(selectedIds),
    [selectedIds],
  );

  // W3 — derive the shared target slug across the current selection.
  // The "一并更新 (N)" button renders ONLY when this is non-null, so
  // the rules here are intentionally strict:
  //   * batchMode must be on (no accidental triggers from single mode)
  //   * selection size ≥ 2 (single-row merges go through the Workbench)
  //   * every selected entry has a `target_candidate.slug`
  //   * all those slugs agree
  // Any violation returns null → button is hidden (not disabled) per
  // spec: the presence of the button is itself the "cohort is
  // mergeable" affordance.
  const mergeTargetSlug = useMemo<string | null>(() => {
    if (!batchMode || selectedIds.size < 2) return null;
    let agreed: string | null = null;
    for (const id of selectedIds) {
      const entry = intelligentEntries.find((e) => e.id === id);
      const slug = entry?.intelligence?.target_candidate?.slug;
      if (!slug) return null;
      if (agreed === null) {
        agreed = slug;
      } else if (agreed !== slug) {
        return null;
      }
    }
    return agreed;
  }, [batchMode, selectedIds, intelligentEntries]);

  // W3 — title / raw-id / score maps for the CombinedPreviewDialog's
  // items list. Built once per intelligence pass so the dialog can
  // render its list synchronously before the preview round-trip
  // returns.
  const combinedTitles = useMemo(() => {
    const map = new Map<number, string>();
    for (const entry of intelligentEntries) map.set(entry.id, entry.title);
    return map;
  }, [intelligentEntries]);

  const combinedSourceRawIds = useMemo(() => {
    const map = new Map<number, number | null | undefined>();
    for (const entry of intelligentEntries) {
      map.set(entry.id, entry.source_raw_id ?? null);
    }
    return map;
  }, [intelligentEntries]);

  const combinedScores = useMemo(() => {
    const map = new Map<number, number>();
    for (const entry of intelligentEntries) {
      map.set(entry.id, entry.intelligence.score);
    }
    return map;
  }, [intelligentEntries]);

  const handleCombinedApplied = useCallback(
    (_result: CombinedApplyResponse) => {
      // Refetch the inbox list so applied ids drop to the appropriate
      // resolved bucket, then clear the selection + exit batch mode
      // so the user lands back on the single-row workbench.
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
      setSelectedIds(new Set());
      setBatchMode(false);
      setCombinedOpen(false);
    },
    [queryClient],
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Page head */}
      <div className="flex shrink-0 items-center justify-between border-b border-border/50 px-6 py-4">
        <div>
          <h1 className="text-lg text-foreground">
            Inbox
          </h1>
          <p className="mt-1 text-muted-foreground/60" style={{ fontSize: 11 }}>
            新素材自动入队 -- AI 生成知识页面 -- 审批后写入 Wiki
          </p>
        </div>
        <div className="flex items-center gap-2" style={{ fontSize: 11 }}>
          {(listQuery.data?.pending_count ?? 0) > 0 && (
            <button
              type="button"
              onClick={async () => {
                if (!window.confirm(`确定要清空所有 ${listQuery.data?.pending_count} 条待处理任务？`)) return;
                const pending = entries.filter((e) => e.status === "pending");
                for (const e of pending) {
                  try { await resolveInboxEntry(e.id, "reject"); } catch { /* ok */ }
                }
                void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
              }}
              className="rounded-md border border-border/40 px-2 py-0.5 text-muted-foreground transition-colors hover:border-destructive hover:text-destructive"
            >
              全部清除
            </button>
          )}
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
          <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/30 px-3 py-1.5">
            <span
              className="font-mono uppercase tracking-widest text-muted-foreground/60"
              style={{ fontSize: 10 }}
            >
              队列 · Queue
            </span>
            <button
              type="button"
              onClick={handleToggleBatchMode}
              className={
                "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 transition-colors " +
                (batchMode
                  ? "border-primary/50 bg-primary/10 text-primary"
                  : "border-border/40 text-muted-foreground hover:border-border hover:text-foreground")
              }
              style={{ fontSize: 10 }}
              title={batchMode ? "退出批量模式" : "进入批量模式（勾选行后批量拒绝）"}
            >
              {batchMode ? (
                <CheckSquare2 className="size-3" aria-hidden />
              ) : (
                <Square className="size-3" aria-hidden />
              )}
              {batchMode ? "批量中" : "批量编辑"}
            </button>
          </div>
          <EntryList
            entries={intelligentEntries}
            isLoading={listQuery.isLoading}
            error={listQuery.error}
            selectedId={selectedId}
            onSelect={(id) => setSelectedId(id)}
            batchMode={batchMode}
            selectedIds={selectedIds}
            onToggleSelect={toggleSelected}
            onBulkSelect={bulkSelect}
            sharedTargetCounts={sharedTargetCounts}
          />
        </aside>
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-xl">
          {batchMode ? (
            <BatchModePane
              selectedIds={selectedIdList}
              totalPending={listQuery.data?.pending_count ?? 0}
              onClearSelection={clearBatchSelection}
              mergeTargetSlug={mergeTargetSlug}
              onMergeClick={() => setCombinedOpen(true)}
              onResolved={({ succeededIds, failedIds }) => {
                // Prune succeeded ids from the selection; failed ids
                // stay so the user can retry. Refetch the list either
                // way so counters + statuses stay in sync.
                setSelectedIds((prev) => {
                  const next = new Set(prev);
                  for (const id of succeededIds) next.delete(id);
                  return next;
                });
                void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
                // If everything went through, drop back into single
                // mode automatically — no selection left to act on.
                if (failedIds.length === 0) {
                  setBatchMode(false);
                }
              }}
            />
          ) : focusState === "focused" && selectedEntry ? (
            <EntryDetail
              key={selectedEntry.id}
              entry={selectedEntry}
              selectedIdLabel={selectedIdLabel}
              onClearFocus={() => setSelectedId(null)}
            />
          ) : focusState === "missing" ? (
            <div className="flex flex-1 flex-col overflow-hidden">
              <div className="shrink-0 px-6 py-4">
                <DeepLinkNotFoundBanner
                  message="该任务不存在或已被删除"
                  detail={<>task {selectedIdLabel}</>}
                  onClear={() => setSelectedId(null)}
                />
              </div>
              <EntryPlaceholder />
            </div>
          ) : focusState === "loading" ? (
            <div className="flex flex-1 items-center justify-center p-6 text-center">
              <Loader2 className="size-5 animate-spin text-muted-foreground/60" />
            </div>
          ) : (
            <EntryPlaceholder />
          )}
        </main>
      </div>

      {/*
        W3 — CombinedPreviewDialog is rendered at the page root so its
        modal overlay floats above the split pane. The dialog itself
        handles its own open/close animations; we only gate the mount
        when we have both a target slug and ≥2 ids to avoid a flash of
        "合并 0 条…" during the batch-mode exit animation.
      */}
      {mergeTargetSlug && combinedOpen && selectedIds.size >= 2 && (
        <CombinedPreviewDialog
          open={combinedOpen}
          onOpenChange={setCombinedOpen}
          targetSlug={mergeTargetSlug}
          inboxIds={selectedIdList}
          titles={combinedTitles}
          sourceRawIds={combinedSourceRawIds}
          scores={combinedScores}
          onApplied={handleCombinedApplied}
        />
      )}
    </div>
  );
}

/**
 * Right-side placeholder rendered when batch mode is on. Shows the
 * toolbar + a gentle reminder of what batch mode covers.
 */
function BatchModePane({
  selectedIds,
  totalPending,
  onClearSelection,
  mergeTargetSlug,
  onMergeClick,
  onResolved,
}: {
  selectedIds: number[];
  totalPending: number;
  onClearSelection: () => void;
  /** W3 — pass-through to `BatchActionsToolbar`. `null` hides the merge button. */
  mergeTargetSlug: string | null;
  /** W3 — fires when the user clicks "一并更新". */
  onMergeClick: () => void;
  onResolved: (result: {
    succeededIds: number[];
    failedIds: number[];
    totalAttempted: number;
  }) => void;
}) {
  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <BatchActionsToolbar
        selectedIds={selectedIds}
        totalPending={totalPending}
        onClearSelection={onClearSelection}
        onResolved={onResolved}
        mergeTargetSlug={mergeTargetSlug}
        onMergeClick={onMergeClick}
      />
      <div className="flex flex-1 items-center justify-center p-6 text-center">
        <div className="max-w-sm">
          <CheckSquare2 className="mx-auto mb-2 size-8 opacity-30" />
          <p className="text-foreground/80" style={{ fontSize: 13 }}>
            {selectedIds.length > 0
              ? `已选 ${selectedIds.length} 条任务 — 点右上角按钮批量处理`
              : "在左侧勾选任务，右上角按钮批量处理"}
          </p>
          <p className="mt-2 text-muted-foreground/60" style={{ fontSize: 11 }}>
            批量模式仅支持拒绝 — 其他动作请退出批量模式后单条处理。
          </p>
        </div>
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
  batchMode,
  selectedIds,
  onToggleSelect,
  onBulkSelect,
  sharedTargetCounts,
}: {
  entries: IntelligentEntry[];
  isLoading: boolean;
  error: Error | null;
  selectedId: number | null;
  onSelect: (id: number) => void;
  batchMode: boolean;
  selectedIds: Set<number>;
  onToggleSelect: (id: number) => void;
  onBulkSelect: (ids: number[], on: boolean) => void;
  /** Q2 — slug → queue-wide count map for `SharedTargetBadge`. */
  sharedTargetCounts: Map<string, number>;
}) {
  const queryClient = useQueryClient();

  // Q1 — group entries by recommended action. The queue-intelligence
  // module owns the 5-group ordering + intra-group score sort; we
  // just map the result.
  //
  // MUST be called before any early-return — Rules of Hooks.
  const groups = useMemo(() => groupAndSortByAction(entries), [entries]);

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
    // R1 sprint — wrap the empty state in the shared `EmptyState`
    // primitive so it looks consistent with Graph / Raw / Wiki, and
    // teach the user what an Inbox actually is + the three upstream
    // funnels that feed it.
    return (
      <div className="flex flex-1 items-center justify-center px-2">
        <EmptyState
          size="full"
          icon={InboxIcon}
          title="暂无维护任务"
          description={
            <>
              Inbox 是新入库素材的审阅队列。
              <br />
              当你在 WeChat 发消息、Ask 粘链接、或素材库手动添加，每条素材会自动排队到这里。
            </>
          }
          primaryAction={{
            label: "打开素材库",
            onClick: () => {
              window.location.hash = "#/raw";
            },
          }}
          secondaryAction={{
            label: "打开 Ask",
            onClick: () => {
              window.location.hash = "#/chat";
            },
          }}
        />
      </div>
    );
  }

  return (
    <ul className="flex-1 overflow-y-auto">
      {groups.map((group) => {
        const groupIds = group.entries.map((e) => e.id);
        const selectedInGroup = groupIds.filter((id) =>
          selectedIds.has(id),
        );
        return (
          <div key={group.action}>
            <QueueGroupHeader
              action={group.action}
              count={group.entries.length}
              batchMode={batchMode}
              selectedInGroup={selectedInGroup}
              allIds={groupIds}
              onSelectAll={() => onBulkSelect(groupIds, true)}
              onDeselectAll={() => onBulkSelect(groupIds, false)}
            />
            {group.entries.map((entry) => {
              const isActive = !batchMode && entry.id === selectedId;
              const isChecked = selectedIds.has(entry.id);
              return (
                <li
                  key={entry.id}
                  id={`inbox-task-${entry.id}`}
                  className="border-b border-border/20 last:border-b-0"
                >
                  <div
                    role="button"
                    tabIndex={0}
                    onClick={() => {
                      if (batchMode) {
                        onToggleSelect(entry.id);
                      } else {
                        onSelect(entry.id);
                      }
                    }}
                    onKeyDown={(ev) => {
                      if (ev.key === "Enter" || ev.key === " ") {
                        ev.preventDefault();
                        if (batchMode) onToggleSelect(entry.id);
                        else onSelect(entry.id);
                      }
                    }}
                    className={
                      "flex w-full cursor-pointer items-start gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50 " +
                      (isActive
                        ? "bg-accent border-l-[3px] border-primary"
                        : "border-l-[3px] border-l-transparent")
                    }
                  >
                    {batchMode ? (
                      <input
                        type="checkbox"
                        checked={isChecked}
                        onClick={(ev) => ev.stopPropagation()}
                        onChange={() => onToggleSelect(entry.id)}
                        className="mt-1 size-3.5 shrink-0 cursor-pointer accent-primary"
                        aria-label={`选中任务 #${entry.id}`}
                      />
                    ) : (
                      <StatusIcon status={entry.status} />
                    )}
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <IngestDecisionBadge
                          decision={entry.decision}
                          compact
                        />
                        <span
                          className="flex-1 truncate text-foreground"
                          style={{
                            fontSize: 12,
                            fontWeight: isActive ? 500 : 400,
                          }}
                        >
                          {entry.title.replace(/^New raw entry/, "新素材")}
                        </span>
                      </div>
                      <div className="mt-0.5 flex flex-wrap items-center gap-1">
                        <RecommendedActionBadge
                          action={entry.intelligence.recommended_action}
                          compact
                        />
                        {/* Q2 — shared-target cohort indicator. Only
                            renders when ≥ 2 entries target the same slug. */}
                        {(() => {
                          const slug =
                            entry.intelligence.target_candidate?.slug;
                          if (!slug) return null;
                          const count = sharedTargetCounts.get(slug) ?? 0;
                          return (
                            <SharedTargetBadge slug={slug} count={count} />
                          );
                        })()}
                        {entry.intelligence.cohort_raw_id != null && (
                          <span
                            className="inline-flex items-center gap-0.5 rounded-full border border-border/40 px-1.5 py-0.5 text-muted-foreground/70"
                            style={{ fontSize: 10 }}
                            title={`同 raw #${String(entry.intelligence.cohort_raw_id).padStart(5, "0")} 还有其他任务`}
                          >
                            <Users className="size-2.5" aria-hidden />
                            同源
                          </span>
                        )}
                        {entry.intelligence.why_long ? (
                          <InfoTooltip side="right">
                            <div
                              className="space-y-1"
                              style={{ fontSize: 11, lineHeight: 1.5 }}
                            >
                              <div className="font-medium text-foreground">
                                {entry.intelligence.why}
                              </div>
                              <div className="text-muted-foreground/80">
                                {entry.intelligence.why_long}
                              </div>
                            </div>
                          </InfoTooltip>
                        ) : null}
                        <span
                          className="ml-auto shrink-0 text-muted-foreground/40"
                          style={{ fontSize: 10 }}
                        >
                          {translateKind(entry.kind)}
                        </span>
                      </div>
                      <div
                        className="mt-0.5 truncate text-muted-foreground/50"
                        style={{ fontSize: 10 }}
                      >
                        {entry.description}
                      </div>
                      <div
                        className="mt-0.5 flex items-center gap-1 text-muted-foreground/40"
                        style={{ fontSize: 10 }}
                      >
                        <Clock className="size-2.5" />
                        {formatRelative(entry.created_at)}
                      </div>
                    </div>
                    {!batchMode && entry.status === "pending" && (
                      <button
                        type="button"
                        className="shrink-0 rounded p-0.5 text-muted-foreground/30 transition-colors hover:text-destructive"
                        onClick={(ev) => {
                          ev.stopPropagation();
                          void resolveInboxEntry(entry.id, "reject").then(
                            () => {
                              void queryClient.invalidateQueries({
                                queryKey: inboxKeys.list(),
                              });
                            },
                          );
                        }}
                        title="删除"
                        aria-label="删除任务"
                      >
                        <XCircle className="size-3" />
                      </button>
                    )}
                  </div>
                </li>
              );
            })}
          </div>
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

function EntryDetail({
  entry,
  selectedIdLabel,
  onClearFocus,
}: {
  entry: IntelligentEntry;
  selectedIdLabel: string;
  onClearFocus: () => void;
}) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  // Legacy proposal state (scoped to entry.id — reset on switch).
  const [proposal, setProposal] = useState<WikiPageProposal | null>(null);

  // W1 Workbench state: §2 decision, §3 result envelope.
  const [maintainAction, setMaintainAction] = useState<MaintainAction>("create_new");
  const [targetPageSlug, setTargetPageSlug] = useState<string | null>(null);
  const [rejectionReason, setRejectionReason] = useState<string>("");
  const [maintainResult, setMaintainResult] = useState<MaintainResponse | null>(null);

  // W2 update_existing preview/apply state. Holds the live
  // `UpdateProposal` returned by `createProposal`; falls back to
  // reconstructing the envelope from `entry.proposal_*` fields on
  // reload (see `displayProposal` below). Reset on entry switch.
  const [activeProposal, setActiveProposal] = useState<UpdateProposal | null>(null);

  // Q2 — Duplicate-concept guard state. `guardDismissed` suppresses
  // both Layer 1 (ambient banner) and Layer 2 (modal) for the rest of
  // this entry's lifecycle. `showGuardDialog` gates Layer 2 visibility.
  // Both reset on entry switch (see the effect below).
  const [guardDismissed, setGuardDismissed] = useState(false);
  const [showGuardDialog, setShowGuardDialog] = useState(false);

  // Raw detail — drives §1 Evidence (skipped when source_raw_id is null).
  const rawQuery = useQuery({
    queryKey: ["raw", entry.source_raw_id ?? null],
    queryFn: () =>
      entry.source_raw_id != null
        ? fetchRawById(entry.source_raw_id)
        : Promise.resolve(null),
    enabled: entry.source_raw_id != null,
    staleTime: 30_000,
  });
  const rawEntry = rawQuery.data?.entry ?? null;
  const rawBody = rawQuery.data?.body ?? null;

  // Q2 — target-candidate query. Only fetched while the user is in a
  // decision state where the candidates actually matter (`update_existing`
  // picker or `create_new` duplicate guard). A 30 s staleTime keeps the
  // picker snappy on intra-entry toggles without starving cold reloads.
  const candidatesQuery = useQuery({
    queryKey: ["inbox", entry.id, "candidates"],
    queryFn: () => fetchInboxCandidates(entry.id, { with_graph: true }),
    enabled:
      maintainAction === "update_existing" ||
      maintainAction === "create_new",
    staleTime: 30_000,
  });
  const candidates: TargetCandidate[] =
    candidatesQuery.data?.candidates ?? [];
  const topCandidate: TargetCandidate | null = candidates[0] ?? null;

  // Legacy Propose → Approve flow — fallback inside §2, unchanged.
  const resolveMutation = useMutation({
    mutationFn: (action: InboxResolveAction) => resolveInboxEntry(entry.id, action),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
  });
  const proposeMutation = useMutation({
    mutationFn: () => proposeForInboxEntry(entry.id),
    onSuccess: (data) => setProposal(data.proposal),
  });
  const writeMutation = useMutation({
    mutationFn: (p: WikiPageProposal) => approveInboxWithWrite(entry.id, p),
    onSuccess: () => {
      setProposal(null);
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
  });

  // W1 primary mutation: `POST /api/wiki/inbox/{id}/maintain`.
  const maintainMutation = useMutation({
    mutationFn: () =>
      maintainInboxEntry(entry.id, {
        action: maintainAction,
        target_page_slug:
          maintainAction === "update_existing" ? targetPageSlug ?? undefined : undefined,
        rejection_reason:
          maintainAction === "reject" ? rejectionReason.trim() : undefined,
      }),
    onSuccess: (response) => {
      setMaintainResult(response);
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
    onError: (err) => {
      // Normalise errors into MaintainResponse so §3 renders a `failed` card.
      setMaintainResult({
        outcome: "failed",
        error: err instanceof Error ? err.message : String(err),
      });
    },
  });

  // W2 proposal mutations (update_existing preview/apply flow).
  // The three mutations all invalidate the inbox list query on
  // success so the proposal_status field on the entry snapshot stays
  // consistent with the backend; applyProposal additionally nudges
  // the wiki page detail cache so opening the target page shows the
  // freshly-written content instead of a stale body.
  const proposalMutation = useMutation({
    mutationFn: () =>
      createProposal(entry.id, (targetPageSlug ?? "").trim()),
    onSuccess: (proposal) => {
      setActiveProposal(proposal);
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
  });

  const applyMutation = useMutation({
    mutationFn: () => applyProposal(entry.id),
    onSuccess: (outcome) => {
      // Fold the outcome into the §3 result card so success/retry
      // UX stays identical to the create_new / reject paths.
      setMaintainResult({
        outcome: "updated",
        target_page_slug: outcome.target_page_slug,
      });
      setActiveProposal(null);
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
      void queryClient.invalidateQueries({
        queryKey: ["wiki", "pages", "detail", outcome.target_page_slug],
      });
    },
    onError: (err) => {
      setMaintainResult({
        outcome: "failed",
        error: err instanceof Error ? err.message : String(err),
      });
    },
  });

  const cancelProposalMutation = useMutation({
    mutationFn: () => cancelProposal(entry.id),
    onSuccess: () => {
      setActiveProposal(null);
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
  });

  // Reset Workbench state on entry switch. If the incoming entry
  // carries a persisted `proposal_status === "pending"` (W2), we
  // snap the action selector to `update_existing` so the user lands
  // directly on the diff preview instead of a stale default.
  //
  // Q1 extension — when no W2 proposal is live yet but the queue
  // intelligence recommends an action we can map 1:1 onto the
  // MaintainActionRadio vocabulary, pre-seed that action and (when
  // present) the target slug so the user lands on the right form
  // variant immediately. `open_diff_preview` funnels into the W2
  // pending branch (handled by `incomingHasPending` already);
  // `ask_first` / `defer` don't map and fall back to `create_new`.
  useEffect(() => {
    const incomingHasPending = entry.proposal_status === "pending";
    const rec = entry.intelligence.recommended_action;

    let initialAction: MaintainAction = "create_new";
    let initialSlug: string | null = null;
    if (incomingHasPending) {
      initialAction = "update_existing";
      initialSlug = entry.target_page_slug ?? null;
    } else if (rec === "update_existing") {
      initialAction = "update_existing";
      initialSlug =
        entry.intelligence.target_candidate?.slug ??
        entry.target_page_slug ??
        null;
    } else if (rec === "suggest_reject") {
      initialAction = "reject";
    } else if (rec === "create_new") {
      initialAction = "create_new";
    }
    setMaintainAction(initialAction);
    setTargetPageSlug(initialSlug);
    setRejectionReason("");
    setMaintainResult(null);
    setActiveProposal(null);
    // Q2 — reset duplicate-guard state so a previously-dismissed guard
    // on a different entry doesn't silently carry over.
    setGuardDismissed(false);
    setShowGuardDialog(false);

    // Scroll the decision section into view so the user lands on the
    // radio they're about to act on (instead of at the top of the
    // long detail pane). rAF defers the measurement until after the
    // render that mounts §2.
    requestAnimationFrame(() => {
      const el = document.getElementById("inbox-maintain-section");
      el?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }, [
    entry.id,
    entry.proposal_status,
    entry.target_page_slug,
    entry.intelligence.recommended_action,
    entry.intelligence.target_candidate?.slug,
  ]);

  const isResolved = entry.status !== "pending";
  const canMaintain =
    !isResolved &&
    entry.kind === "new-raw" &&
    entry.source_raw_id != null &&
    proposal === null;

  const anyPending =
    resolveMutation.isPending ||
    proposeMutation.isPending ||
    writeMutation.isPending ||
    maintainMutation.isPending ||
    proposalMutation.isPending ||
    applyMutation.isPending ||
    cancelProposalMutation.isPending;

  // W2 Phase 2 gate: prefer live mutation state, fall back to the
  // persisted `proposal_status === "pending"` snapshot so a reload
  // lands the user right back in the diff preview. The server also
  // writes `proposed_after_markdown` + `before_markdown_snapshot`
  // to the entry; we reconstruct an `UpdateProposal` envelope from
  // those so the render path stays uniform (`displayProposal` is a
  // single source of truth for Phase 2).
  //
  // Rare edge case: `proposal_status === "pending"` is set but the
  // markdown hasn't flushed (server race). We still treat it as
  // Phase 2 — the diff preview component surfaces an explicit
  // skeleton when both columns are empty, which is a clearer
  // affordance than snapping back to the Phase-1 form.
  const hasPendingProposal = entry.proposal_status === "pending";

  const displayProposal: UpdateProposal | null = useMemo(() => {
    if (activeProposal) return activeProposal;
    if (hasPendingProposal) {
      return {
        target_slug:
          entry.target_page_slug ?? targetPageSlug ?? "",
        before_markdown: entry.before_markdown_snapshot ?? "",
        after_markdown: entry.proposed_after_markdown ?? "",
        summary: entry.proposal_summary ?? "",
        generated_at: 0,
      };
    }
    return null;
  }, [
    activeProposal,
    hasPendingProposal,
    entry.target_page_slug,
    entry.before_markdown_snapshot,
    entry.proposed_after_markdown,
    entry.proposal_summary,
    targetPageSlug,
  ]);

  // Once a proposal exists, the radio+picker form freezes so the
  // user doesn't flip action mid-diff-review. `disabled` propagates
  // into the MaintainActionRadio slot too.
  const lockFormForProposal =
    maintainAction === "update_existing" && displayProposal !== null;

  // `执行` gate:
  //   - create_new  → always OK while pending.
  //   - update_existing Phase 1 (no proposal yet) → needs slug.
  //   - update_existing Phase 2 (has proposal)    → always OK.
  //   - reject      → needs ≥4 chars.
  const canExecuteMaintain = useMemo(() => {
    if (isResolved || anyPending) return false;
    switch (maintainAction) {
      case "create_new":
        return true;
      case "update_existing":
        if (displayProposal) return true;
        return (targetPageSlug?.trim().length ?? 0) > 0;
      case "reject":
        return rejectionReason.trim().length >= 4;
      default:
        return false;
    }
  }, [
    isResolved,
    anyPending,
    maintainAction,
    targetPageSlug,
    rejectionReason,
    displayProposal,
  ]);

  // Primary button label + click handler — both depend on the
  // Phase the update_existing branch is in. Other actions keep
  // the W1 "执行" semantics untouched.
  const executeLabel = useMemo(() => {
    if (maintainAction !== "update_existing") return "执行";
    return displayProposal ? "应用更新" : "生成提案";
  }, [maintainAction, displayProposal]);

  /** Raw mutation trigger — bypasses the Q2 Layer 2 guard. */
  const runExecute = () => {
    if (maintainAction === "update_existing") {
      if (displayProposal) {
        applyMutation.mutate();
      } else {
        proposalMutation.mutate();
      }
      return;
    }
    maintainMutation.mutate();
  };

  const handleExecute = () => {
    // Q2 — Layer 2 guard. When the user is about to commit a new page
    // but the top candidate score still crosses the threshold AND the
    // ambient banner was never dismissed, surface the modal first so
    // the action requires an explicit "继续新建" confirmation.
    if (
      maintainAction === "create_new" &&
      topCandidate &&
      topCandidate.score >= DUPLICATE_GUARD_SCORE_THRESHOLD &&
      !guardDismissed
    ) {
      setShowGuardDialog(true);
      return;
    }
    runExecute();
  };

  const executePending =
    maintainAction === "update_existing"
      ? displayProposal
        ? applyMutation.isPending
        : proposalMutation.isPending
      : maintainMutation.isPending;

  // §3 outcome: prefer live mutation result, fall back to server-stamped entry fields.
  const resolvedOutcome: MaintainOutcome | null = useMemo(() => {
    if (maintainResult) return maintainResult.outcome;
    if (entry.maintain_outcome) return entry.maintain_outcome;
    if (entry.status === "approved") return "updated";
    if (entry.status === "rejected") return "rejected";
    return null;
  }, [maintainResult, entry.maintain_outcome, entry.status]);

  // Runtime-guard `last_ingest_decision` (typed as `unknown` for forward-compat).
  const rawDecision: IngestDecision | null = useMemo(() => {
    const candidate = rawEntry?.last_ingest_decision;
    if (
      candidate &&
      typeof candidate === "object" &&
      "kind" in candidate &&
      typeof (candidate as { kind?: unknown }).kind === "string"
    ) {
      return candidate as IngestDecision;
    }
    return null;
  }, [rawEntry]);

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
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
            <h2 className="mt-2 text-lg text-foreground">
              {entry.title}
            </h2>
            {/* F2: persistent focus chip with copy-link action + clear */}
            <div className="mt-2">
              <DeepLinkFocusChip
                onClear={onClearFocus}
                action={<CopyDeepLinkButton variant="compact" />}
              >
                <CheckSquare className="size-3" />
                聚焦中 {selectedIdLabel}
              </DeepLinkFocusChip>
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-muted-foreground/40" style={{ fontSize: 11 }}>
              <span>创建于: {entry.created_at}</span>
              {entry.resolved_at && <span>处理于: {entry.resolved_at}</span>}
              {entry.source_raw_id != null && (
                <Link
                  to={`/raw?entry=${entry.source_raw_id}`}
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
          {/* F2 contract 4.6: explicit close button returns to the
              default list view. The FocusChip × duplicates this action,
              but a standalone affordance in the header corner matches
              the wider close-pattern vocabulary users expect on detail
              panes. */}
          <button
            type="button"
            onClick={onClearFocus}
            title="清除聚焦"
            aria-label="清除聚焦"
            className="shrink-0 rounded-md border border-border/40 p-1 text-muted-foreground transition-colors hover:border-border hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          >
            <X className="size-4" />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-auto px-6 py-5 space-y-6">
        {/* ── Description (unchanged, still the first thing users see) */}
        <div>
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
        </div>

        {/* ══ Section 1 · Evidence ══════════════════════════════════ */}
        <WorkbenchSection
          number={1}
          title="证据"
          english="Evidence"
          hint="素材的抓取来源、规范化 URL、正文预览 — 以及（若存在）AI 预生成草稿。"
        >
          <EvidenceSection
            entry={entry}
            rawBody={rawBody}
            rawFilename={rawEntry?.filename ?? null}
            rawIngestedAt={rawEntry?.ingested_at ?? null}
            rawDecision={rawDecision}
            canonicalUrl={rawEntry?.canonical_url ?? null}
            originalUrl={rawEntry?.original_url ?? null}
            sourceUrl={rawEntry?.source_url ?? null}
            isLoading={rawQuery.isLoading}
          />
        </WorkbenchSection>

        {/* ══ Section 2 · Maintain ══════════════════════════════════ */}
        <div id="inbox-maintain-section" />
        <WorkbenchSection
          number={2}
          title="决策"
          english="Maintain"
          hint="选择维护动作：新建 / 合并 / 拒绝。legacy propose→approve 保留在下方作为备用流程。"
        >
          {isResolved ? (
            <div
              className="rounded-md border border-border/40 bg-muted/10 px-4 py-3 text-muted-foreground/80"
              style={{ fontSize: 12 }}
            >
              任务{translateStatus(entry.status)} — 决策区已锁定。结果见下方 §3。
            </div>
          ) : (
            <div className="space-y-4">
              {/* Q1 — queue-intelligence hint strip. When the rec is
                  `ask_first`, we surface a prominent Ask CTA so the
                  user can fan out to a chat session before committing
                  to a maintain action (matches the UX the Main lock
                  asked for). Other recs render a slim context line so
                  the reasoning stays visible in §2 too. */}
              {entry.intelligence.recommended_action === "ask_first" ? (
                <div
                  className="flex items-start gap-2 rounded-md border px-3 py-2"
                  style={{
                    fontSize: 12,
                    borderColor:
                      "color-mix(in srgb, var(--color-warning) 40%, transparent)",
                    backgroundColor:
                      "color-mix(in srgb, var(--color-warning) 8%, transparent)",
                  }}
                >
                  <HelpCircle
                    className="mt-0.5 size-3.5 shrink-0"
                    style={{ color: "var(--color-warning)" }}
                    aria-hidden
                  />
                  <div className="min-w-0 flex-1">
                    <div
                      className="font-medium"
                      style={{ color: "var(--color-warning)" }}
                    >
                      {entry.intelligence.why}
                    </div>
                    {entry.intelligence.why_long ? (
                      <div className="mt-0.5 text-muted-foreground/80">
                        {entry.intelligence.why_long}
                      </div>
                    ) : null}
                    <button
                      type="button"
                      onClick={() => {
                        // Prefer the helper when the import resolves;
                        // fall back to a hand-rolled hash URL if the
                        // helper signature drifts.
                        const href = buildAskBindUrl({
                          kind: "inbox",
                          id: entry.id,
                          title: entry.title,
                        });
                        navigate(href.replace(/^#/, ""));
                      }}
                      className="mt-1.5 inline-flex items-center gap-1 rounded-md border px-2 py-0.5 font-medium transition-colors"
                      style={{
                        fontSize: 11,
                        borderColor:
                          "color-mix(in srgb, var(--color-warning) 60%, transparent)",
                        color: "var(--color-warning)",
                      }}
                    >
                      <HelpCircle className="size-3" aria-hidden />
                      先 Ask 再处理
                    </button>
                  </div>
                </div>
              ) : entry.intelligence.recommended_action !== "create_new" ? (
                <div
                  className="rounded-md border border-border/30 bg-muted/20 px-3 py-1.5 text-muted-foreground/80"
                  style={{ fontSize: 11 }}
                >
                  <span className="font-medium text-foreground/80">
                    推荐：
                  </span>
                  {entry.intelligence.why}
                  {entry.intelligence.why_long ? (
                    <span className="ml-1 text-muted-foreground/60">
                      · {entry.intelligence.why_long}
                    </span>
                  ) : null}
                </div>
              ) : null}
              <MaintainActionRadio
                value={maintainAction}
                onValueChange={setMaintainAction}
                targetPageSlug={targetPageSlug}
                rejectionReason={rejectionReason}
                onRejectionReasonChange={setRejectionReason}
                // Lock the action picker while a proposal is live so
                // the user can't accidentally flip to create_new mid
                // diff-review (which would silently leave a stale
                // proposal on disk). Otherwise the normal anyPending
                // gate still applies.
                disabled={anyPending || lockFormForProposal}
                wikiPageSearchSlot={
                  <div className="space-y-2">
                    {/* Q2 — server-ranked candidates sit above the manual
                        search field. The picker never hides the search,
                        so a zero-candidate response falls through to the
                        compact EmptyState + the original search path. */}
                    <TargetCandidatePicker
                      candidates={candidates}
                      onSelect={(slug) => setTargetPageSlug(slug)}
                      selectedSlug={
                        displayProposal?.target_slug ?? targetPageSlug
                      }
                      disabled={anyPending || lockFormForProposal}
                    />
                    {/* Worker C's WikiPageSearchField (fallback search). */}
                    <WikiPageSearchField
                      value={
                        (displayProposal?.target_slug ?? targetPageSlug) ??
                        undefined
                      }
                      onSelect={(slug) => setTargetPageSlug(slug)}
                    />
                  </div>
                }
              />

              {/* Q2 — Layer 1 duplicate-concept guard (ambient banner).
                  Triggers only in `create_new` with a high-confidence
                  top candidate (score ≥ 75). Interacts with the Q1
                  `ask_first` CTA as mutually exclusive by state: if the
                  user is in `create_new`, they've already moved past
                  the `ask_first` gate, so the two banners can't coexist
                  in practice. */}
              {maintainAction === "create_new" &&
                topCandidate &&
                topCandidate.score >= DUPLICATE_GUARD_SCORE_THRESHOLD &&
                !guardDismissed && (
                  <DuplicateGuardBanner
                    candidate={topCandidate}
                    onSwitchToUpdate={() => {
                      setMaintainAction("update_existing");
                      setTargetPageSlug(topCandidate.slug);
                      setGuardDismissed(true);
                    }}
                    onDismiss={() => setGuardDismissed(true)}
                  />
                )}

              {/* W2 Phase 2 — diff preview (only when update_existing
                  has a live or persisted proposal). Rendered above
                  the execute bar so the primary action ("应用更新")
                  sits right under the diff the user just reviewed. */}
              {maintainAction === "update_existing" && displayProposal && (
                <div className="space-y-2">
                  <WikiPageDiffPreview
                    before={displayProposal.before_markdown}
                    after={displayProposal.after_markdown}
                    summary={displayProposal.summary}
                  />
                </div>
              )}

              <div className="flex items-center justify-end gap-2 border-t border-border/30 pt-3">
                {/* Cancel proposal (Phase 2 only) — secondary, sits
                    to the left of the primary "应用更新" button. */}
                {maintainAction === "update_existing" && displayProposal && (
                  <button
                    type="button"
                    onClick={() => cancelProposalMutation.mutate()}
                    disabled={anyPending}
                    className="flex items-center gap-1.5 rounded-md border border-border/50 bg-background px-3 py-1.5 text-muted-foreground transition-colors hover:border-destructive hover:text-destructive disabled:opacity-50"
                    style={{ fontSize: 13 }}
                  >
                    {cancelProposalMutation.isPending ? (
                      <Loader2 className="size-3 animate-spin" />
                    ) : (
                      <XCircle className="size-3" />
                    )}
                    取消提案
                  </button>
                )}
                <button
                  type="button"
                  onClick={handleExecute}
                  disabled={!canExecuteMaintain}
                  className="flex items-center gap-1.5 rounded-md bg-primary px-4 py-1.5 font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
                  style={{ fontSize: 13 }}
                >
                  {executePending ? (
                    <Loader2 className="size-3 animate-spin" />
                  ) : (
                    <Sparkles className="size-3" />
                  )}
                  {executeLabel}
                </button>
              </div>

              {/* ── Legacy Propose / Approve fallback ───────────── */}
              <LegacyMaintainFallback
                canMaintain={canMaintain}
                anyPending={anyPending}
                proposal={proposal}
                onPropose={() => proposeMutation.mutate()}
                onReject={() => resolveMutation.mutate("reject")}
                onApprove={() => resolveMutation.mutate("approve")}
                onWrite={(p) => writeMutation.mutate(p)}
                proposeLoading={proposeMutation.isPending}
                rejectLoading={resolveMutation.isPending}
                approveLoading={resolveMutation.isPending}
                writeLoading={writeMutation.isPending}
              />
            </div>
          )}

          {/* Error strip — R1 trust layer: each mutation surfaces as a
              classifier-driven `FailureBanner`. High-frequency errors
              (BadJson, concurrent-edit) get friendly copy + a retry
              CTA; the rest fall through with the raw string under the
              technical-detail `<details>`. We render per-mutation so
              the user knows which step failed. */}
          <div className="mt-3 space-y-2">
            {maintainMutation.error && (
              <MaintainerErrorBanner
                stage="maintain"
                error={maintainMutation.error}
                onRetry={() => maintainMutation.mutate()}
              />
            )}
            {proposalMutation.error && (
              <MaintainerErrorBanner
                stage="proposal"
                error={proposalMutation.error}
                onRetry={() => proposalMutation.mutate()}
              />
            )}
            {applyMutation.error && (
              <MaintainerErrorBanner
                stage="apply"
                error={applyMutation.error}
                onRetry={() => applyMutation.mutate()}
              />
            )}
            {cancelProposalMutation.error && (
              <MaintainerErrorBanner
                stage="cancel"
                error={cancelProposalMutation.error}
                onRetry={() => cancelProposalMutation.mutate()}
              />
            )}
            {proposeMutation.error && (
              <MaintainerErrorBanner
                stage="legacy_propose"
                error={proposeMutation.error}
                onRetry={() => proposeMutation.mutate()}
              />
            )}
            {writeMutation.error && (
              <MaintainerErrorBanner
                stage="legacy_write"
                error={writeMutation.error}
              />
            )}
            {resolveMutation.error && (
              <MaintainerErrorBanner
                stage="legacy_resolve"
                error={resolveMutation.error}
              />
            )}
          </div>
        </WorkbenchSection>

        {/* ══ Section 3 · Result ══════════════════════════════════ */}
        {resolvedOutcome && (
          <WorkbenchSection
            number={3}
            title="结果"
            english="Result"
            hint="执行结果 — 成功时可直接打开 Wiki 页；失败可重试；拒绝附原因。"
          >
            <MaintainerResultCard
              outcome={resolvedOutcome}
              targetPageSlug={
                maintainResult?.target_page_slug ?? entry.target_page_slug ?? null
              }
              rejectionReason={
                maintainResult?.rejection_reason ?? entry.rejection_reason ?? null
              }
              errorMessage={maintainResult?.error ?? entry.maintain_error ?? null}
              onRetry={() => {
                setMaintainResult(null);
                maintainMutation.reset();
                applyMutation.reset();
              }}
              onOpenWikiPage={(slug) =>
                navigateToWikiPage(slug, entry.proposed_title ?? slug, "maintain-result")
              }
            />
          </WorkbenchSection>
        )}
      </div>

      {/* Q2 — Layer 2 duplicate-concept guard (modal). Mounted at the
          pane root so it overlays the full workbench. Rendered only
          when a viable top candidate exists — the open flag alone is
          not enough, since `topCandidate` can toggle between renders
          (candidate refetch, entry switch). */}
      {topCandidate && (
        <DuplicateGuardDialog
          open={showGuardDialog}
          onOpenChange={setShowGuardDialog}
          candidate={topCandidate}
          onSwitchToUpdate={() => {
            setMaintainAction("update_existing");
            setTargetPageSlug(topCandidate.slug);
            setGuardDismissed(true);
          }}
          onProceed={() => {
            // Mark dismissed so a later re-click of 执行 doesn't reopen
            // the dialog in an infinite loop — the user has explicitly
            // opted into the duplicate create.
            setGuardDismissed(true);
            runExecute();
          }}
        />
      )}
    </div>
  );
}

/* ─── Workbench section shell ──────────────────────────────────── */

/**
 * Shared card wrapper for the three Workbench sections. Numbered
 * badge + bilingual title + hint line + bordered body.
 */
function WorkbenchSection({
  number,
  title,
  english,
  hint,
  children,
}: {
  number: number;
  title: string;
  english: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <header className="mb-2 flex items-baseline gap-2">
        <span
          className="flex size-5 shrink-0 items-center justify-center rounded-full border border-primary/40 bg-primary/10 font-mono text-primary"
          style={{ fontSize: 10 }}
        >
          {number}
        </span>
        <h3 className="text-foreground" style={{ fontSize: 13, fontWeight: 500 }}>{title}</h3>
        <span className="font-mono uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 10 }}>{english}</span>
      </header>
      <p className="mb-2 text-muted-foreground/70" style={{ fontSize: 11, lineHeight: 1.5 }}>{hint}</p>
      <div className="rounded-md border border-border/30 bg-background/50 px-4 py-3">{children}</div>
    </section>
  );
}

/* ─── Evidence section ─────────────────────────────────────────── */

/**
 * Section 1 of the Workbench — three stacked sub-cards: source card
 * (decision + URL track + raw deep link), body preview, and an
 * optional Propose draft card (only when `entry.proposed_*` is set).
 */
function EvidenceSection({
  entry,
  rawBody,
  rawFilename,
  rawIngestedAt,
  rawDecision,
  canonicalUrl,
  originalUrl,
  sourceUrl,
  isLoading,
}: {
  entry: InboxEntry;
  rawBody: string | null;
  rawFilename: string | null;
  rawIngestedAt: string | null;
  rawDecision: IngestDecision | null;
  canonicalUrl: string | null;
  originalUrl: string | null;
  sourceUrl: string | null;
  isLoading: boolean;
}) {
  const hasProposedDraft = Boolean(
    entry.proposed_title?.length ||
      entry.proposed_summary?.length ||
      entry.proposed_content_markdown?.length,
  );
  const headerCls =
    "font-mono uppercase tracking-widest text-muted-foreground/60";
  return (
    <div className="space-y-3">
      {/* 1. Source card — decision + URL + raw deep link */}
      <div className="rounded-md border border-border/40 bg-muted/5 px-3 py-2 space-y-2">
        <div className={`flex items-center gap-2 ${headerCls}`} style={{ fontSize: 10 }}>
          <span>来源 / Source</span>
          {entry.source_raw_id != null && rawFilename && (
            <span className="text-muted-foreground/40">{rawFilename}</span>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <IngestDecisionBadge decision={rawDecision} />
          {rawIngestedAt && (
            <span className="font-mono text-muted-foreground/60" style={{ fontSize: 11 }}>
              {rawIngestedAt}
            </span>
          )}
        </div>
        <URLTrackBadge canonicalUrl={canonicalUrl} originalUrl={originalUrl} sourceUrl={sourceUrl} />
        {entry.source_raw_id != null && (
          <Link
            to={`/raw?entry=${entry.source_raw_id}`}
            className="inline-flex items-center gap-1 text-primary hover:underline"
            style={{ fontSize: 11 }}
          >
            <FileText className="size-3" />
            在 Raw Library 打开 raw #{String(entry.source_raw_id).padStart(5, "0")}
            <ArrowRight className="size-3" />
          </Link>
        )}
      </div>

      {/* 2. Body preview */}
      <div>
        <div className={`mb-1 ${headerCls}`} style={{ fontSize: 10 }}>正文预览 / Body</div>
        {isLoading ? (
          <div className="flex items-center gap-2 rounded-md border border-border/40 px-3 py-4 text-muted-foreground/60" style={{ fontSize: 11 }}>
            <Loader2 className="size-3 animate-spin" />
            加载 raw 正文中…
          </div>
        ) : (
          <BodyPreviewPanel body={rawBody ?? ""} heading={rawFilename ?? undefined} />
        )}
      </div>

      {/* 3. Propose draft (only when proposed_* fields are present) */}
      {hasProposedDraft && (
        <div>
          <div className={`mb-1 ${headerCls}`} style={{ fontSize: 10 }}>Propose 预览 / Draft</div>
          <div className="rounded-md border border-border/40 bg-muted/5 px-3 py-2">
            {entry.proposed_wiki_slug && (
              <div className="mb-1 font-mono text-muted-foreground/60" style={{ fontSize: 10 }}>
                {entry.proposed_wiki_slug}
              </div>
            )}
            {entry.proposed_title && (
              <div className="text-foreground" style={{ fontSize: 14, fontWeight: 500 }}>{entry.proposed_title}</div>
            )}
            {entry.proposed_summary && (
              <div className="mt-1 text-muted-foreground/80" style={{ fontSize: 12 }}>{entry.proposed_summary}</div>
            )}
            {entry.proposed_content_markdown && (
              <div className="mt-2">
                <BodyPreviewPanel body={entry.proposed_content_markdown} collapsedLines={6} />
              </div>
            )}
          </div>
        </div>
      )}

      {/* 4. Lineage summary — P1 sprint.
          Compact 2-arm (upstream / downstream) lineage block for the
          inbox task. Capped height so §1 Evidence doesn't drift when
          the backend returns a long lineage chain. */}
      <InboxLineageSummary entryId={entry.id} />
    </div>
  );
}

/* ─── Legacy maintain fallback ─────────────────────────────────── */

/** Collapsible pre-W1 propose/approve/reject flow — kept inside §2 as a fallback. */
function LegacyMaintainFallback({
  canMaintain, anyPending, proposal,
  onPropose, onReject, onApprove, onWrite,
  proposeLoading, rejectLoading, approveLoading, writeLoading,
}: {
  canMaintain: boolean;
  anyPending: boolean;
  proposal: WikiPageProposal | null;
  onPropose: () => void;
  onReject: () => void;
  onApprove: () => void;
  onWrite: (p: WikiPageProposal) => void;
  proposeLoading: boolean;
  rejectLoading: boolean;
  approveLoading: boolean;
  writeLoading: boolean;
}) {
  const btnBase = "flex items-center gap-1 rounded-md px-2 py-1 transition-colors disabled:opacity-50";
  const outlineCls = `${btnBase} border border-border/50 text-muted-foreground hover:border-border hover:text-foreground`;
  const primaryCls = `${btnBase} bg-primary/90 text-primary-foreground hover:bg-primary`;
  return (
    <details className="rounded-md border border-dashed border-border/40 px-3 py-2">
      <summary className="cursor-pointer select-none text-muted-foreground hover:text-foreground" style={{ fontSize: 11 }}>
        备用流程：Propose → Approve（legacy）
      </summary>
      <div className="mt-2 space-y-2">
        {proposal ? (
          <div className="space-y-2">
            <h4 className="uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 10 }}>生成的知识页面</h4>
            <ProposalPreview proposal={proposal} />
          </div>
        ) : (
          <div className="text-muted-foreground/70" style={{ fontSize: 12 }}>
            调用一次 AI 总结（≤200 词）。在你批准之前不会写入磁盘。
          </div>
        )}
        <div className="flex items-center justify-end gap-2" style={{ fontSize: 11 }}>
          {canMaintain && !proposal && (
            <button type="button" onClick={onPropose} disabled={anyPending} className={outlineCls}>
              {proposeLoading ? <Loader2 className="size-3 animate-spin" /> : <Sparkles className="size-3" />}
              开始维护（旧）
            </button>
          )}
          <button type="button" onClick={onReject} disabled={anyPending} className={`${btnBase} border border-border/50 text-muted-foreground hover:border-destructive hover:text-destructive`}>
            <XCircle className="size-3" />
            拒绝
          </button>
          {proposal ? (
            <button type="button" onClick={() => onWrite(proposal)} disabled={anyPending} className={primaryCls}>
              {writeLoading ? <Loader2 className="size-3 animate-spin" /> : <Save className="size-3" />}
              批准并写入
            </button>
          ) : (
            <button type="button" onClick={onApprove} disabled={anyPending} className={primaryCls}>
              {approveLoading || rejectLoading ? <Loader2 className="size-3 animate-spin" /> : <CheckCircle2 className="size-3" />}
              批准
            </button>
          )}
        </div>
      </div>
    </details>
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
            style={{ fontSize: 16 }}
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

/* ─── Maintainer error banner (R1 trust layer) ─────────────────── */

type MaintainerErrorStage =
  | "maintain"
  | "proposal"
  | "apply"
  | "cancel"
  | "legacy_propose"
  | "legacy_write"
  | "legacy_resolve";

const STAGE_LABELS: Record<MaintainerErrorStage, string> = {
  maintain: "维护失败",
  proposal: "生成提案失败",
  apply: "应用更新失败",
  cancel: "取消提案失败",
  legacy_propose: "生成失败",
  legacy_write: "写入失败",
  legacy_resolve: "处理失败",
};

/**
 * Classifier-driven error strip for a single inbox mutation. Three
 * high-frequency kinds get bespoke friendly copy; everything else
 * falls through to a generic banner with the raw string under
 * technical-detail.
 *
 * We don't plug retry into the legacy_write / legacy_resolve stages
 * because those both depend on state the UI already consumed by the
 * time the error arrives (the legacy `proposal` state object is
 * cleared in `writeMutation.onSuccess`, so a blind re-mutate would
 * fire with a stale closure).
 */
function MaintainerErrorBanner({
  stage,
  error,
  onRetry,
}: {
  stage: MaintainerErrorStage;
  error: unknown;
  onRetry?: () => void;
}) {
  const raw = error instanceof Error ? error.message : String(error);
  const classified = classifyInboxMutationError(raw);
  const defaultLabel = STAGE_LABELS[stage];

  if (classified.kind === "bad_json") {
    return (
      <FailureBanner
        severity="warning"
        title="⚠️ 无法生成知识页面提案"
        description="大模型返回的内容格式异常，可能是网络中断或 API 超时。重试通常能解决。"
        technicalDetail={raw}
        actions={
          onRetry
            ? [{ label: "重试", onClick: onRetry, variant: "primary" }]
            : undefined
        }
      />
    );
  }
  if (classified.kind === "concurrent_edit") {
    return (
      <FailureBanner
        severity="warning"
        title="🔄 内容已更新"
        description="这个 Wiki 页面在你生成提案后已被修改。请重新生成提案以合并最新内容。"
        technicalDetail={raw}
        actions={
          onRetry
            ? [
                {
                  label: "重新生成提案",
                  onClick: onRetry,
                  variant: "primary",
                },
              ]
            : undefined
        }
      />
    );
  }
  return (
    <FailureBanner
      severity="error"
      title={defaultLabel}
      description="后端在处理这一步时失败了。查看下方技术细节，或直接重试。"
      technicalDetail={raw}
      actions={
        onRetry
          ? [{ label: "重试", onClick: onRetry, variant: "primary" }]
          : undefined
      }
    />
  );
}

function classifyInboxMutationError(
  raw: string,
): { kind: "bad_json" | "concurrent_edit" | "unknown" } {
  const text = raw ?? "";
  if (/invalid\s+json|BadJson|failed\s+to\s+parse\s+json/i.test(text)) {
    return { kind: "bad_json" };
  }
  if (
    /changed\s+since\s+proposal|page\s+changed|stale\s+snapshot|concurrent\s+edit/i.test(
      text,
    )
  ) {
    return { kind: "concurrent_edit" };
  }
  return { kind: "unknown" };
}

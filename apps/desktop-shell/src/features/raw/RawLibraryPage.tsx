/**
 * Raw Library -- Material Manager (redesigned)
 *
 * Full-width card list with collapsed input behind "添加" button.
 * Search, batch actions, expandable detail, multi-select delete.
 */

import { useState, useRef, useCallback, useMemo, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import ReactMarkdown from "react-markdown";
import {
  Loader2,
  FileText,
  Link2,
  Upload,
  Copy,
  Check,
  Trash2,
  Plus,
  Search,
  X,
  Maximize2,
  Target,
} from "lucide-react";
import { listRawEntries, getRawEntry } from "@/features/ingest/persist";
import { ingestText } from "@/features/ingest/adapters/text";
import { ingestUrl, type IngestUrlResult } from "@/features/ingest/adapters/url";
import { fetchJson } from "@/lib/desktop/transport";
import type { RawEntry } from "@/features/ingest/types";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { parsePositiveInt, useDeepLinkState } from "@/lib/deep-link";
import {
  CopyDeepLinkButton,
  DeepLinkFocusChip,
  DeepLinkNotFoundBanner,
} from "@/components/deep-link";
import { RawEntryCard } from "@/components/ds/RawEntryCard";
import {
  SourceIcon,
  sourceBadgeStyle,
  translateSource,
  formatSize,
} from "@/components/ds/row-primitives";

const rawKeys = {
  list: () => ["wiki", "raw", "list"] as const,
  detail: (id: number) => ["wiki", "raw", "detail", id] as const,
};

/* ─── Main page ──────────────────────────────────────────────────── */

export function RawLibraryPage() {
  // URL is the source of truth for the focused entry. The hook handles
  // lazy init from ?entry=N, reverse-sync on external URL changes
  // (paste, link click, back button), and URL writes via replace. We
  // only need to think in terms of state here; URL is automatic.
  const [expandedId, setExpandedId] = useDeepLinkState(
    "entry",
    parsePositiveInt,
  );
  const [showAddPanel, setShowAddPanel] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const queryClient = useQueryClient();

  const listQuery = useQuery({
    queryKey: rawKeys.list(),
    queryFn: () => listRawEntries(),
    staleTime: 30_000,
  });

  const deleteMutation = useMutation({
    mutationFn: async (id: number) => {
      await fetchJson(`/api/wiki/raw/${id}`, { method: "DELETE" });
    },
    onSuccess: (_data, deletedId) => {
      void queryClient.invalidateQueries({ queryKey: rawKeys.list() });
      if (expandedId === deletedId) {
        setExpandedId(null);
      }
      setSelectedIds((prev) => {
        const next = new Set(prev);
        next.delete(deletedId);
        return next;
      });
    },
  });

  const batchDeleteMutation = useMutation({
    mutationFn: async (ids: number[]) => {
      await Promise.all(
        ids.map((id) => fetchJson(`/api/wiki/raw/${id}`, { method: "DELETE" })),
      );
      return ids.length;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: rawKeys.list() });
      setSelectedIds(new Set());
      setExpandedId(null);
    },
  });

  const batchCleanupMutation = useMutation({
    mutationFn: async () => {
      const small = (listQuery.data?.entries ?? []).filter(
        (e) => e.byte_size < 100,
      );
      await Promise.all(
        small.map((e) =>
          fetchJson(`/api/wiki/raw/${e.id}`, { method: "DELETE" }),
        ),
      );
      return small.length;
    },
    onSuccess: (count) => {
      void queryClient.invalidateQueries({ queryKey: rawKeys.list() });
      if (
        expandedId !== null &&
        (listQuery.data?.entries ?? []).some(
          (e) => e.id === expandedId && e.byte_size < 100,
        )
      ) {
        setExpandedId(null);
      }
      setSelectedIds(new Set());
      void count;
    },
  });

  const allEntries = listQuery.data?.entries ?? [];
  const smallCount = allEntries.filter((e) => e.byte_size < 100).length;

  // Filter entries by search query
  const filteredEntries = useMemo(() => {
    if (!searchQuery.trim()) return allEntries;
    const q = searchQuery.toLowerCase();
    return allEntries.filter(
      (e) =>
        e.slug.toLowerCase().includes(q) ||
        translateSource(e.source).includes(q) ||
        e.date.includes(q),
    );
  }, [allEntries, searchQuery]);

  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleIngested = (entry: RawEntry) => {
    setExpandedId(entry.id);
    setShowAddPanel(false);
  };

  // Scroll deep-linked entry into view on initial mount only.
  // Intentionally not dependent on `expandedId`: same-page URL pastes
  // (G1 path) update expandedId via the useDeepLinkState reverse-sync
  // effect, and scrolling there would jitter whenever the user toggles
  // an entry with a click. Mount-time behaviour covers the primary UX
  // (external link → fresh mount → scroll to target).
  useEffect(() => {
    if (expandedId !== null) {
      requestAnimationFrame(() => {
        const el = document.getElementById(`raw-entry-${expandedId}`);
        el?.scrollIntoView({ behavior: "smooth", block: "nearest" });
      });
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Only on mount — not on every selection change

  // Focus state: drives the chip/banner mutual exclusion.
  //   none     → no chip, no banner (baseline list view)
  //   loading  → don't show anything (avoid a one-frame "missing"
  //              flash while listQuery is still resolving)
  //   focused  → target exists in the full list (may be filtered out
  //              of `filteredEntries` by searchQuery — that's still
  //              "focused", not "missing"; searching just hides it
  //              from the current view)
  //   missing  → list finished loading, target id not in allEntries
  //              (deleted, invalid typed id, stale deep link)
  let focusState: "none" | "loading" | "focused" | "missing";
  let focusedEntry: RawEntry | null = null;
  if (expandedId === null) {
    focusState = "none";
  } else if (listQuery.isLoading) {
    focusState = "loading";
  } else if (listQuery.isError) {
    // An outright error is handled inside CardList; don't double-render
    // a missing banner on top of it.
    focusState = "none";
  } else {
    const target = allEntries.find((e) => e.id === expandedId) ?? null;
    if (target) {
      focusState = "focused";
      focusedEntry = target;
    } else {
      focusState = "missing";
    }
  }
  const expandedIdLabel =
    expandedId !== null ? `#${String(expandedId).padStart(5, "0")}` : "";

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* ── Header ──────────────────────────────────────────────── */}
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <div className="flex items-start justify-between">
          <div>
            <h1 className="text-lg text-foreground">
              素材库
            </h1>
            <p className="mt-0.5 text-muted-foreground/60" style={{ fontSize: 11 }}>
              所有原始素材按时间排列 · 微信文章、网页链接、文件、手动粘贴内容都会落在这里
            </p>
          </div>
          <div className="flex items-center gap-2">
            {/* Batch cleanup */}
            {smallCount > 0 && (
              <button
                type="button"
                onClick={() => batchCleanupMutation.mutate()}
                disabled={batchCleanupMutation.isPending}
                className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                style={{ fontSize: 11 }}
              >
                {batchCleanupMutation.isPending ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : (
                  <Trash2 className="size-3" />
                )}
                批量清理 ({smallCount})
              </button>
            )}
            {/* Batch delete selected */}
            {selectedIds.size > 0 && (
              <button
                type="button"
                onClick={() => batchDeleteMutation.mutate([...selectedIds])}
                disabled={batchDeleteMutation.isPending}
                className="flex items-center gap-1 rounded-md border border-destructive/50 bg-destructive/10 px-2 py-1 text-destructive transition-colors hover:bg-destructive/20 disabled:opacity-50"
                style={{ fontSize: 11 }}
              >
                {batchDeleteMutation.isPending ? (
                  <Loader2 className="size-3 animate-spin" />
                ) : (
                  <Trash2 className="size-3" />
                )}
                删除选中 ({selectedIds.size})
              </button>
            )}
            {/* Entry count */}
            <span className="text-muted-foreground/40" style={{ fontSize: 11 }}>
              {allEntries.length} 条
            </span>
            {/* Add button */}
            <Button
              variant={showAddPanel ? "default" : "outline"}
              size="sm"
              onClick={() => setShowAddPanel(!showAddPanel)}
            >
              {showAddPanel ? <X className="size-3.5" /> : <Plus className="size-3.5" />}
              添加
            </Button>
          </div>
        </div>

        {/* Search bar */}
        <div className="relative mt-3">
          <Search className="absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground/40" />
          <Input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="搜索素材标题、来源..."
            className="pl-8 pr-8"
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery("")}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground/40 hover:text-muted-foreground"
            >
              <X className="size-3" />
            </button>
          )}
        </div>
      </div>

      {/* ── Collapsible add panel ───────────────────────────────── */}
      {showAddPanel && (
        <div className="shrink-0 border-b border-border/50 bg-accent/20">
          <AddPanel
            onIngested={handleIngested}
            onClose={() => setShowAddPanel(false)}
          />
        </div>
      )}

      {/* ── Deep-link focus chip (above list, below header) ──────── */}
      {focusState === "focused" && focusedEntry !== null && (
        <div className="shrink-0 px-6 pt-3">
          <DeepLinkFocusChip
            onClear={() => setExpandedId(null)}
            action={<CopyDeepLinkButton variant="compact" />}
          >
            <Target className="size-3" aria-hidden="true" />
            <span>聚焦中 {expandedIdLabel}</span>
            <span
              className="ml-1 inline-flex items-center gap-1 rounded-full px-1.5 py-0.5"
              style={{
                backgroundColor: sourceBadgeStyle(focusedEntry.source).bg,
                color: sourceBadgeStyle(focusedEntry.source).text,
                fontSize: 10,
              }}
            >
              <SourceIcon
                source={focusedEntry.source}
                className="size-3"
              />
              {translateSource(focusedEntry.source)}
            </span>
          </DeepLinkFocusChip>
        </div>
      )}

      {/* ── Deep-link not-found banner (stale / invalid id) ──────── */}
      {focusState === "missing" && (
        <div className="shrink-0 px-6 pt-3">
          <DeepLinkNotFoundBanner
            message="该素材不存在或已被删除"
            detail={<>raw {expandedIdLabel}</>}
            onClear={() => setExpandedId(null)}
          />
        </div>
      )}

      {/* ── Main card list ──────────────────────────────────────── */}
      <div className="flex-1 overflow-y-auto">
        <CardList
          entries={filteredEntries}
          isLoading={listQuery.isLoading}
          error={listQuery.error}
          expandedId={expandedId}
          onToggleExpand={(id) => {
            setExpandedId(expandedId === id ? null : id);
          }}
          onClearExpand={() => setExpandedId(null)}
          onDelete={(id) => deleteMutation.mutate(id)}
          deletingId={deleteMutation.isPending ? (deleteMutation.variables ?? null) : null}
          selectedIds={selectedIds}
          onToggleSelect={toggleSelect}
        />
      </div>
    </div>
  );
}

/* ─── Add panel (collapsed by default) ────────────────────────────── */

interface AddPanelProps {
  onIngested: (entry: RawEntry) => void;
  onClose: () => void;
}

/** Response shape from the MarkItDown convert endpoint. */
interface MarkItDownResponse {
  ok: boolean;
  title: string;
  markdown: string;
  source: string;
  raw_id: number;
}

function AddPanel({ onIngested }: AddPanelProps) {
  const [mode, setMode] = useState<"text" | "url" | "file">("text");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [url, setUrl] = useState("");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const queryClient = useQueryClient();

  const tabCls = (active: boolean) =>
    "flex-1 rounded-md border px-2 py-1 font-medium transition-colors " +
    (active
      ? "border-primary bg-primary/10 text-primary"
      : "border-border text-muted-foreground hover:bg-accent");

  const ingestMutation = useMutation<IngestUrlResult, Error, void>({
    mutationFn: async () => {
      if (mode === "text") {
        if (!body.trim()) throw new Error("内容不能为空");
        // `ingestText` still returns a bare RawEntry (text ingest has no
        // URL-level dedupe layer yet). Wrap it into the envelope shape
        // so downstream handlers only branch on `decision?.kind`.
        const entry = await ingestText({ title, body });
        return { raw_entry: entry, decision: null };
      }
      if (!url.trim()) throw new Error("链接不能为空");
      return ingestUrl({ url, title });
    },
    onSuccess: (data) => {
      void queryClient.invalidateQueries({ queryKey: rawKeys.list() });
      setTitle("");
      setBody("");
      setUrl("");
      setErrorMessage(null);

      // M4: banner text reflects what the dedupe orchestrator actually
      // did. Falls through to "created_new" wording when either the
      // server didn't emit a decision (legacy backend) or this was a
      // text-ingest (which has no URL dedupe yet).
      const rawId = String(data.raw_entry.id).padStart(5, "0");
      const decision = data.decision;
      let message: string;
      if (!decision) {
        message = `✓ 已入库:${data.raw_entry.slug} (raw #${rawId})`;
      } else {
        switch (decision.kind) {
          case "reused_with_pending_inbox":
          case "reused_approved":
          case "reused_silent":
            message = `✓ 已复用此前入库的素材 raw #${rawId}(${data.raw_entry.slug})`;
            break;
          case "reused_after_reject":
            message = `✓ 已复用 raw #${rawId}(此前被拒;若需重抓请用 force)`;
            break;
          case "refreshed_content": {
            const prevId = String(decision.previous_raw_id).padStart(5, "0");
            message = `⟳ 内容已更新,新建 raw #${rawId}(原版本 raw #${prevId} 保留)`;
            break;
          }
          case "content_duplicate":
            message = `✓ 相同内容已存在于 raw #${rawId}(来自另一 URL)`;
            break;
          case "explicit_reingest":
            message = `✓ 已强制重抓,新 raw #${rawId}`;
            break;
          case "created_new":
          default:
            message = `✓ 已入库:${data.raw_entry.slug} (raw #${rawId})`;
            break;
        }
      }
      setSuccessMessage(message);

      onIngested(data.raw_entry);
    },
    onError: (err) => {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    },
  });

  const convertMutation = useMutation({
    mutationFn: async (filePath: string) => {
      return fetchJson<MarkItDownResponse>(
        "/api/desktop/markitdown/convert",
        {
          method: "POST",
          body: JSON.stringify({ path: filePath, ingest: true }),
        },
        120_000,
      );
    },
    onSuccess: (data) => {
      void queryClient.invalidateQueries({ queryKey: rawKeys.list() });
      setErrorMessage(null);
      setSuccessMessage(`已入库: ${data.title}`);
      onIngested({
        id: data.raw_id,
        filename: "",
        source: data.source,
        slug: data.title,
        date: new Date().toISOString().slice(0, 10),
        ingested_at: new Date().toISOString(),
        byte_size: 0,
      });
    },
    onError: (err) => {
      setSuccessMessage(null);
      setErrorMessage(err instanceof Error ? err.message : String(err));
    },
  });

  const pickAndConvert = useCallback(async () => {
    setErrorMessage(null);
    setSuccessMessage(null);
    let filePath: string | null = null;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, title: "选择文件" });
      if (selected && typeof selected === "string") {
        filePath = selected;
      }
    } catch {
      fileInputRef.current?.click();
      return;
    }
    if (filePath) {
      convertMutation.mutate(filePath);
    }
  }, [convertMutation]);

  const handleBrowserFile = useCallback(
    (file: File) => {
      setErrorMessage(null);
      setSuccessMessage(null);
      const path = (file as { path?: string }).path || file.name;
      convertMutation.mutate(path);
    },
    [convertMutation],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragOver(false);
      const file = e.dataTransfer.files[0];
      if (file) handleBrowserFile(file);
    },
    [handleBrowserFile],
  );

  return (
    <div className="px-6 py-3">
      {/* Tabs */}
      <div className="mb-2 flex items-center gap-1" style={{ fontSize: 12 }}>
        <button type="button" className={tabCls(mode === "text")} onClick={() => { setMode("text"); setSuccessMessage(null); }}>
          <FileText className="mr-1 inline size-3" />
          文本
        </button>
        <button type="button" className={tabCls(mode === "url")} onClick={() => { setMode("url"); setSuccessMessage(null); }}>
          <Link2 className="mr-1 inline size-3" />
          链接
        </button>
        <button type="button" className={tabCls(mode === "file")} onClick={() => { setMode("file"); setSuccessMessage(null); }}>
          <Upload className="mr-1 inline size-3" />
          文件
        </button>
      </div>

      {/* Text / URL inputs */}
      {mode !== "file" && (
        <div className="flex gap-2">
          <div className="min-w-0 flex-1 space-y-1.5">
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="标题（可选）"
              className="w-full rounded-md border border-input bg-background px-2 py-1 text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring/40"
              style={{ fontSize: 13 }}
            />
            {mode === "text" ? (
              <textarea
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder="粘贴 markdown 或文本..."
                rows={3}
                className="w-full resize-none rounded-md border border-input bg-background px-2 py-1.5 font-mono text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring/40"
                style={{ fontSize: 12 }}
              />
            ) : (
              <input
                type="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://..."
                className="w-full rounded-md border border-input bg-background px-2 py-1 text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring/40"
                style={{ fontSize: 13 }}
              />
            )}
          </div>
          <button
            type="button"
            onClick={() => ingestMutation.mutate()}
            disabled={
              ingestMutation.isPending ||
              (mode === "text" ? !body.trim() : !url.trim())
            }
            className="flex shrink-0 items-center gap-1 self-end rounded-md bg-primary px-3 py-1.5 font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
            style={{ fontSize: 13 }}
            title={
              mode === "text" && !body.trim()
                ? "请先粘贴内容"
                : mode === "url" && !url.trim()
                  ? "请先填写 URL"
                  : undefined
            }
          >
            {ingestMutation.isPending && <Loader2 className="size-3 animate-spin" />}
            {ingestMutation.isPending ? "入库中..." : "入库"}
          </button>
        </div>
      )}

      {/* File upload */}
      {mode === "file" && (
        <>
          <input
            ref={fileInputRef}
            type="file"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) handleBrowserFile(file);
              e.target.value = "";
            }}
          />
          {convertMutation.isPending ? (
            <div className="flex items-center justify-center gap-2 rounded-md border-2 border-dashed border-primary/40 bg-primary/5 px-3 py-4">
              <Loader2 className="size-4 animate-spin text-primary" />
              <span className="text-primary" style={{ fontSize: 13 }}>转换中，请稍候...</span>
            </div>
          ) : (
            <button
              type="button"
              onClick={pickAndConvert}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
              className={
                "flex w-full cursor-pointer items-center justify-center gap-2 rounded-md border-2 border-dashed px-3 py-4 transition-colors " +
                (isDragOver
                  ? "border-primary bg-primary/10"
                  : "border-border hover:border-primary/50 hover:bg-accent/30")
              }
            >
              <Upload className="size-4 text-muted-foreground/50" />
              <span className="text-muted-foreground" style={{ fontSize: 13 }}>
                拖放文件或点击选择
              </span>
              <span className="text-muted-foreground/40" style={{ fontSize: 11 }}>
                PDF / Word / Excel / PPT / 图片 / 音频
              </span>
            </button>
          )}
        </>
      )}

      {/* Feedback */}
      {errorMessage && (
        <div
          className="mt-1.5 rounded-md border px-2 py-1"
          style={{
            fontSize: 12,
            borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
            backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
            color: "var(--color-error)",
          }}
        >
          {errorMessage}
        </div>
      )}
      {successMessage && (
        <div
          className="mt-1.5 rounded-md border px-2 py-1"
          style={{
            fontSize: 12,
            borderColor: "color-mix(in srgb, var(--color-primary) 30%, transparent)",
            backgroundColor: "color-mix(in srgb, var(--color-primary) 5%, transparent)",
            color: "var(--color-primary)",
          }}
        >
          {successMessage}
        </div>
      )}
    </div>
  );
}

/* ─── Card list ───────────────────────────────────────────────────── */

interface CardListProps {
  entries: RawEntry[];
  isLoading: boolean;
  error: Error | null;
  expandedId: number | null;
  onToggleExpand: (id: number) => void;
  /**
   * Called when the user clicks the in-place "清除聚焦" affordance on
   * an expanded card. Semantically equivalent to re-clicking the card
   * header, but named explicitly so the UI can expose both paths —
   * mirrors the chip's X button.
   */
  onClearExpand: () => void;
  onDelete: (id: number) => void;
  deletingId: number | null;
  selectedIds: Set<number>;
  onToggleSelect: (id: number) => void;
}

function CardList({
  entries,
  isLoading,
  error,
  expandedId,
  onToggleExpand,
  onClearExpand,
  onDelete,
  deletingId,
  selectedIds,
  onToggleSelect,
}: CardListProps) {
  const navigate = useNavigate();

  // DS2.x-A — Ask handler lifted from the inline EntryCard so
  // `<RawEntryCard>` only receives a parameterless `onAsk` thunk. The
  // `?bind=raw:N&title=...` URL shape mirrors the pre-migration
  // implementation (used by `navigate-helpers::buildAskBindUrl`
  // callers elsewhere in the app).
  const handleAsk = useCallback(
    (entry: RawEntry) => {
      const params = new URLSearchParams();
      params.set("bind", `raw:${entry.id}`);
      params.set("title", entry.slug || `raw #${entry.id}`);
      navigate(`/ask?${params.toString()}`);
    },
    [navigate],
  );

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12 text-muted-foreground" style={{ fontSize: 13 }}>
        <Loader2 className="mr-2 size-4 animate-spin" />
        加载中...
      </div>
    );
  }
  if (error) {
    return (
      <div
        className="mx-6 mt-4 rounded-md border px-3 py-2"
        style={{
          fontSize: 13,
          borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        加载失败：{error.message}
      </div>
    );
  }
  if (entries.length === 0) {
    // R1 sprint — lift the empty state into the shared `EmptyState`
    // primitive so it matches Graph / Inbox / Wiki. Keeps the
    // existing "通过微信 / Ask / 添加" copy intact but places it
    // inside a proper titled card with an icon.
    return (
      <EmptyState
        size="full"
        icon={FileText}
        title="暂无素材"
        description={
          <>
            素材库是所有入库内容的主列表。
            <br />
            你可以通过微信转发、Ask 对话发链接、或点击「添加」手动入库。
          </>
        }
      />
    );
  }

  return (
    <div className="flex flex-col gap-3 px-6 py-3">
      {entries.map((entry) => {
        const isExpanded = entry.id === expandedId;
        return (
          <RawEntryCard
            key={entry.id}
            entry={entry}
            isSelected={selectedIds.has(entry.id)}
            isExpanded={isExpanded}
            isDeleting={deletingId === entry.id}
            onToggleSelect={() => onToggleSelect(entry.id)}
            onToggleExpand={() => onToggleExpand(entry.id)}
            onClearExpand={onClearExpand}
            onDelete={() => onDelete(entry.id)}
            onAsk={() => handleAsk(entry)}
            expandedContent={
              isExpanded ? (
                <ExpandedDetail id={entry.id} entry={entry} />
              ) : undefined
            }
          />
        );
      })}
    </div>
  );
}

/* ─── Expanded detail (inline below card) ─────────────────────────── */

function ExpandedDetail({ id, entry }: { id: number; entry: RawEntry }) {
  const detailQuery = useQuery({
    queryKey: rawKeys.detail(id),
    queryFn: () => getRawEntry(id),
    staleTime: 60_000,
  });

  const [copied, setCopied] = useState(false);
  const [showFullScreen, setShowFullScreen] = useState(false);

  if (detailQuery.isLoading) {
    return (
      <div className="flex items-center justify-center pb-2 pt-3 text-muted-foreground" style={{ fontSize: 12 }}>
        <Loader2 className="mr-2 size-3.5 animate-spin" />
        加载中...
      </div>
    );
  }
  if (detailQuery.error) {
    return (
      <div
        className="mt-3 rounded-md border px-2 py-1.5"
        style={{
          fontSize: 12,
          borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        加载失败：{detailQuery.error.message}
      </div>
    );
  }
  if (!detailQuery.data) return null;

  const { body } = detailQuery.data;

  const handleCopy = () => {
    void navigator.clipboard.writeText(body);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <>
      <div className="mt-3 overflow-hidden rounded-md border border-border/50 bg-background">
        {/* Metadata strip */}
        <div className="flex items-center justify-between border-b border-border/30 px-3 py-1.5">
          <div className="flex items-center gap-3 text-muted-foreground/40" style={{ fontSize: 11 }}>
            <span className="font-mono">#{String(entry.id).padStart(5, "0")}</span>
            <span>{entry.filename}</span>
            <span>{entry.ingested_at}</span>
            <span>{formatSize(entry.byte_size)}</span>
          </div>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={handleCopy}
              className="flex items-center gap-1 rounded px-1.5 py-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              style={{ fontSize: 11 }}
              title="复制正文内容"
            >
              {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
              {copied ? "已复制" : "复制"}
            </button>
            {/* Copy deep link (URL with ?entry=N) — distinct from
                "复制" above which copies body text. */}
            <CopyDeepLinkButton />
            <button
              type="button"
              onClick={() => setShowFullScreen(true)}
              className="flex items-center gap-1 rounded px-1.5 py-0.5 text-muted-foreground transition-colors hover:bg-primary/10 hover:text-primary"
              style={{ fontSize: 11 }}
            >
              <Maximize2 className="size-3" />
              全屏阅读
            </button>
          </div>
        </div>

        {/* Source URL */}
        {entry.source_url && (
          <div className="border-b border-border/30 px-3 py-1">
            <a
              href={entry.source_url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-primary underline decoration-primary/40 hover:decoration-primary"
              style={{ fontSize: 11 }}
            >
              {entry.source_url}
            </a>
          </div>
        )}

        {/* Body content */}
        <pre
          className="max-h-[500px] overflow-auto whitespace-pre-wrap px-3 py-2.5 font-mono text-foreground/90"
          style={{ fontSize: 12, lineHeight: 1.6 }}
        >
          {body}
        </pre>
      </div>

      {/* Fullscreen reading modal */}
      {showFullScreen && (
        <FullScreenReader
          entry={entry}
          body={body}
          onClose={() => setShowFullScreen(false)}
        />
      )}
    </>
  );
}

/* ─── Fullscreen reading modal ───────────────────────────────────── */

function FullScreenReader({
  entry,
  body,
  onClose,
}: {
  entry: RawEntry;
  body: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopyAll = () => {
    void navigator.clipboard.writeText(body);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  // Close on Escape key
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    },
    [onClose],
  );

  const badge = sourceBadgeStyle(entry.source);

  return (
    <div
      className="fixed inset-0 z-50 flex flex-col bg-background"
      onKeyDown={handleKeyDown}
      tabIndex={-1}
      ref={(el) => el?.focus()}
    >
      {/* Header */}
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <div className="mx-auto flex max-w-3xl items-start justify-between">
          <h1 className="min-w-0 flex-1 text-xl text-foreground">
            {entry.slug}
          </h1>
          <button
            type="button"
            onClick={onClose}
            className="ml-4 shrink-0 rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title="关闭"
          >
            <X className="size-5" />
          </button>
        </div>
      </div>

      {/* Metadata bar */}
      <div className="shrink-0 border-b border-border/30 bg-accent/20 px-6 py-2">
        <div className="mx-auto flex max-w-3xl flex-wrap items-center gap-3" style={{ fontSize: 12 }}>
          <span
            className="rounded-full px-2 py-0.5 font-medium"
            style={{ backgroundColor: badge.bg, color: badge.text }}
          >
            {translateSource(entry.source)}
          </span>
          {entry.filename && (
            <span className="text-muted-foreground/60">{entry.filename}</span>
          )}
          <span className="text-muted-foreground/50">{entry.date}</span>
          <span className="text-muted-foreground/40">{formatSize(entry.byte_size)}</span>
          {entry.source_url && (
            <a
              href={entry.source_url}
              target="_blank"
              rel="noopener noreferrer"
              className="truncate text-primary underline decoration-primary/40 hover:decoration-primary"
              style={{ maxWidth: 400 }}
            >
              {entry.source_url}
            </a>
          )}
        </div>
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-y-auto px-6 py-6">
        <article
          className="markdown-content mx-auto max-w-3xl text-foreground/90"
          style={{
            fontSize: "16px",
            lineHeight: "1.8",
          }}
        >
          {/* Typography by parent .markdown-content CSS. Link policy:
              Raw content is scraped external text — it contains http/
              https links to original sources, NOT wiki-internal refs
              (those only appear in wiki/ pages written by the maintainer).
              We do NOT run preprocessWikilinks or the full wiki-link
              renderer; instead a minimal safe-link component opens all
              URLs in a new tab with rel="noopener noreferrer". */}
          <ReactMarkdown
            components={{
              a: ({ href, children, ...props }) => (
                <a
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                  {...props}
                >
                  {children}
                </a>
              ),
            }}
          >
            {body}
          </ReactMarkdown>
        </article>
      </div>

      {/* Bottom bar */}
      <div className="shrink-0 border-t border-border/50 px-6 py-3">
        <div className="mx-auto flex max-w-3xl items-center justify-end gap-2">
          <button
            type="button"
            onClick={handleCopyAll}
            className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            style={{ fontSize: 13 }}
          >
            {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
            {copied ? "已复制" : "复制全文"}
          </button>
          <button
            type="button"
            onClick={onClose}
            className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 font-medium text-primary-foreground transition-colors hover:bg-primary/90"
            style={{ fontSize: 13 }}
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}

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
  CheckSquare2,
  ChevronDown,
  Sparkles,
} from "lucide-react";
import { listRawEntries, getRawEntry, triggerAbsorb, listInboxEntries } from "@/api/wiki/repository";
import { ingestText } from "@/features/ingest/adapters/text";
import { ingestUrl, type IngestUrlResult } from "@/features/ingest/adapters/url";
import { fetchJson } from "@/lib/desktop/transport";
import type { RawEntry } from "@/api/wiki/types";
import { Input } from "@/components/ui/input";
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
} from "@/components/ds/row-primitives";

const rawKeys = {
  list: () => ["wiki", "raw", "list"] as const,
  detail: (id: number) => ["wiki", "raw", "detail", id] as const,
};

function rawEntrySortTime(entry: RawEntry): number {
  const ingested = Date.parse(entry.ingested_at);
  if (!Number.isNaN(ingested)) return ingested;

  const dated = Date.parse(entry.date);
  if (!Number.isNaN(dated)) return dated;

  return 0;
}

type RawFilterMode = "all" | "wechat-article" | "wechat-message" | "link" | "pending";
type RawSortMode = "recent" | "oldest" | "words";
type AddMode = "text" | "url" | "file";

const RAW_FILTERS: Array<{ value: RawFilterMode; label: string }> = [
  { value: "all", label: "全部" },
  { value: "wechat-article", label: "微信文章" },
  { value: "wechat-message", label: "微信消息" },
  { value: "link", label: "链接" },
  { value: "pending", label: "待整理" },
];

function isWechatArticle(entry: RawEntry): boolean {
  return entry.source === "wechat-article";
}

function isWechatMessage(entry: RawEntry): boolean {
  return entry.source.startsWith("wechat") && entry.source !== "wechat-article" && entry.source !== "wechat-url";
}

function isLinkEntry(entry: RawEntry): boolean {
  return Boolean(entry.source_url || entry.canonical_url || entry.original_url) || entry.source.includes("url") || entry.source === "url";
}

function isPendingEntry(entry: RawEntry): boolean {
  const decision = entry.last_ingest_decision;
  if (decision && typeof decision === "object" && "kind" in decision) {
    return String((decision as { kind?: unknown }).kind).includes("pending");
  }
  return false;
}

function searchHaystack(entry: RawEntry): string {
  return [
    entry.slug,
    entry.filename,
    entry.source,
    entry.source_url,
    entry.canonical_url,
    entry.original_url,
    translateSource(entry.source),
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
}

function formatRawFriendlyTime(raw: string): string {
  const time = Date.parse(raw);
  if (Number.isNaN(time)) return raw;
  const date = new Date(time);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const thatDay = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const diff = Math.floor((today - thatDay) / (24 * 60 * 60 * 1000));
  const hh = String(date.getHours()).padStart(2, "0");
  const mm = String(date.getMinutes()).padStart(2, "0");
  if (diff <= 0) return `今天 ${hh}:${mm}`;
  if (diff === 1) return `昨天 ${hh}:${mm}`;
  if (diff < 7) return `${diff} 天前`;
  return `${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

function formatRawAmount(bytes: number): string {
  const words = Math.max(1, Math.round((bytes || 0) / 2));
  if (words >= 1000) return `约 ${Math.round(words / 100) / 10} 千字`;
  return `约 ${words} 字`;
}

/* ─── Main page ──────────────────────────────────────────────────── */

export function RawLibraryPage({ embedded = false }: { embedded?: boolean } = {}) {
  const navigate = useNavigate();
  // URL is the source of truth for the focused entry. The hook handles
  // lazy init from ?entry=N, reverse-sync on external URL changes
  // (paste, link click, back button), and URL writes via replace. We
  // only need to think in terms of state here; URL is automatic.
  const [expandedId, setExpandedId] = useDeepLinkState(
    "entry",
    parsePositiveInt,
  );
  const [showAddPanel, setShowAddPanel] = useState(false);
  const [addMode, setAddMode] = useState<AddMode>("url");
  const [showImportMenu, setShowImportMenu] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [filterMode, setFilterMode] = useState<RawFilterMode>("all");
  const [sortMode, setSortMode] = useState<RawSortMode>("recent");
  const [batchMode, setBatchMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const queryClient = useQueryClient();

  const listQuery = useQuery({
    queryKey: rawKeys.list(),
    queryFn: () => listRawEntries(),
    staleTime: 30_000,
  });
  const inboxQuery = useQuery({
    queryKey: ["wiki", "inbox", "list", "raw-library-filter"],
    queryFn: () => listInboxEntries(),
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

  const organizeMutation = useMutation({
    mutationFn: async (ids: number[]) => triggerAbsorb(ids),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: rawKeys.list() });
    },
  });

  const allEntries = listQuery.data?.entries ?? [];
  const pendingRawIds = useMemo(() => {
    const ids = new Set<number>();
    for (const entry of inboxQuery.data?.entries ?? []) {
      if (entry.status === "pending" && typeof entry.source_raw_id === "number") {
        ids.add(entry.source_raw_id);
      }
    }
    return ids;
  }, [inboxQuery.data]);

  // Filter entries by search query, then sort newest-first. The backend
  // historically returns raw ids in ascending order, which makes old
  // material appear first on the product surface.
  const filteredEntries = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    const filtered = allEntries.filter((entry) => {
      if (filterMode === "wechat-article" && !isWechatArticle(entry)) return false;
      if (filterMode === "wechat-message" && !isWechatMessage(entry)) return false;
      if (filterMode === "link" && !isLinkEntry(entry)) return false;
      if (filterMode === "pending" && !pendingRawIds.has(entry.id) && !isPendingEntry(entry)) return false;
      if (!q) return true;
      return searchHaystack(entry).includes(q);
    });

    return [...filtered].sort((a, b) => {
      if (sortMode === "oldest") {
        const byTime = rawEntrySortTime(a) - rawEntrySortTime(b);
        if (byTime !== 0) return byTime;
        return a.id - b.id;
      }
      if (sortMode === "words") {
        const bySize = b.byte_size - a.byte_size;
        if (bySize !== 0) return bySize;
      }
      const byTime = rawEntrySortTime(b) - rawEntrySortTime(a);
      if (byTime !== 0) return byTime;
      return b.id - a.id;
    });
  }, [allEntries, filterMode, pendingRawIds, searchQuery, sortMode]);

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
  const selectedIdList = useMemo(() => [...selectedIds], [selectedIds]);
  const organizingIds = useMemo(
    () => new Set(organizeMutation.isPending ? organizeMutation.variables ?? [] : []),
    [organizeMutation.isPending, organizeMutation.variables],
  );
  const openAddPanel = (mode: AddMode) => {
    setAddMode(mode);
    setShowAddPanel(true);
    setShowImportMenu(false);
  };
  const selectAllVisible = () => {
    setSelectedIds(new Set(filteredEntries.map((entry) => entry.id)));
    setBatchMode(true);
  };

  return (
    <div
      className="raw-library-page flex h-full flex-col overflow-hidden"
      data-embedded={embedded || undefined}
    >
      {/* ── Header ──────────────────────────────────────────────── */}
      <div className="raw-library-header raw-library-header-v2 shrink-0">
        <div className="raw-library-title-block">
          <h1>素材库</h1>
          <p>原料区 · 微信转发、网页链接、文件和手动粘贴都会先落在这里</p>
        </div>

        {selectedIds.size > 0 ? (
          <div className="raw-library-batchbar" aria-label="素材批量操作">
            <span className="raw-library-selected-count">已选 {selectedIds.size} 条</span>
            <button type="button" onClick={selectAllVisible}>全选</button>
            <button
              type="button"
              className="raw-library-danger-action"
              onClick={() => batchDeleteMutation.mutate(selectedIdList)}
              disabled={batchDeleteMutation.isPending}
            >
              {batchDeleteMutation.isPending ? <Loader2 className="size-3 animate-spin" /> : <Trash2 className="size-3" />}
              批量删除
            </button>
            <button
              type="button"
              onClick={() => organizeMutation.mutate(selectedIdList)}
              disabled={organizeMutation.isPending}
            >
              {organizeMutation.isPending ? <Loader2 className="size-3 animate-spin" /> : <Sparkles className="size-3" />}
              批量重新整理
            </button>
            <button
              type="button"
              onClick={() => {
                setSelectedIds(new Set());
                setBatchMode(false);
              }}
            >
              取消
            </button>
          </div>
        ) : (
          <div className="raw-library-toolbar" aria-label="素材库工具栏">
            <label className="raw-library-search">
              <Search className="size-3.5" strokeWidth={1.5} />
              <Input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="搜索素材标题、来源..."
              />
              {searchQuery && (
                <button
                  type="button"
                  onClick={() => setSearchQuery("")}
                  aria-label="清空搜索"
                >
                  <X className="size-3" />
                </button>
              )}
            </label>

            <div className="raw-library-filter" role="group" aria-label="素材筛选">
              {RAW_FILTERS.map((filter) => (
                <button
                  key={filter.value}
                  type="button"
                  data-active={filterMode === filter.value}
                  onClick={() => setFilterMode(filter.value)}
                >
                  {filter.label}
                </button>
              ))}
            </div>

            <button
              type="button"
              className="raw-library-batch-edit"
              onClick={() => setBatchMode(true)}
            >
              <CheckSquare2 className="size-3.5" />
              批量编辑
            </button>

            <select
              className="raw-library-sort"
              value={sortMode}
              onChange={(event) => setSortMode(event.target.value as RawSortMode)}
              aria-label="排序"
            >
              <option value="recent">最近入库</option>
              <option value="oldest">最早入库</option>
              <option value="words">内容最多</option>
            </select>

            <div className="raw-library-import">
              <button
                type="button"
                className="raw-library-import-trigger"
                onClick={() => setShowImportMenu((current) => !current)}
              >
                <Plus className="size-3.5" />
                导入
                <ChevronDown className="size-3" />
              </button>
              {showImportMenu && (
                <div className="raw-library-import-menu">
                  <button type="button" onClick={() => openAddPanel("url")}>粘贴链接</button>
                  <button type="button" onClick={() => openAddPanel("file")}>上传文件（PDF/Word/MD）</button>
                  <button type="button" onClick={() => openAddPanel("text")}>直接输入文本</button>
                  <button
                    type="button"
                    onClick={() => {
                      setShowImportMenu(false);
                      navigate("/connect-wechat");
                    }}
                  >
                    从微信转发
                  </button>
                </div>
              )}
            </div>
          </div>
        )}

        <div className="raw-library-flow-hint" aria-label="知识库流转">
          <span><b>{allEntries.length}</b> 条原料</span>
          <span>→</span>
          <span>进入页面整理</span>
          <span>→</span>
          <span>形成关系图</span>
        </div>
      </div>

      {/* ── Collapsible add panel ───────────────────────────────── */}
      {showAddPanel && (
        <div className="shrink-0 border-b border-border/50 bg-accent/20">
          <AddPanel
            mode={addMode}
            onModeChange={setAddMode}
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
          batchMode={batchMode}
          onSelectAll={selectAllVisible}
          onOrganize={(ids) => organizeMutation.mutate(ids)}
          organizingIds={organizingIds}
        />
      </div>
    </div>
  );
}

/* ─── Add panel (collapsed by default) ────────────────────────────── */

interface AddPanelProps {
  mode: AddMode;
  onModeChange: (mode: AddMode) => void;
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

function AddPanel({ mode, onModeChange, onIngested }: AddPanelProps) {
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
        <button type="button" className={tabCls(mode === "text")} onClick={() => { onModeChange("text"); setSuccessMessage(null); }}>
          <FileText className="mr-1 inline size-3" />
          文本
        </button>
        <button type="button" className={tabCls(mode === "url")} onClick={() => { onModeChange("url"); setSuccessMessage(null); }}>
          <Link2 className="mr-1 inline size-3" />
          链接
        </button>
        <button type="button" className={tabCls(mode === "file")} onClick={() => { onModeChange("file"); setSuccessMessage(null); }}>
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
  batchMode: boolean;
  onSelectAll: () => void;
  onOrganize: (ids: number[]) => void;
  organizingIds: Set<number>;
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
  batchMode,
  onSelectAll,
  onOrganize,
  organizingIds,
}: CardListProps) {
  const navigate = useNavigate();
  const [isDragSelecting, setIsDragSelecting] = useState(false);

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
    <div
      className="raw-library-list-v2"
      onMouseUp={() => setIsDragSelecting(false)}
      onMouseLeave={() => setIsDragSelecting(false)}
    >
      {batchMode && entries.length > 0 && (
        <div className="raw-library-inline-batch">
          <span>批量编辑已开启</span>
          <button type="button" onClick={onSelectAll}>选择当前 {entries.length} 条</button>
        </div>
      )}
      {entries.map((entry) => {
        const isExpanded = entry.id === expandedId;
        return (
          <RawEntryCard
            key={entry.id}
            entry={entry}
            isSelected={selectedIds.has(entry.id)}
            isExpanded={isExpanded}
            isDeleting={deletingId === entry.id}
            batchMode={batchMode}
            isOrganizing={organizingIds.has(entry.id)}
            onToggleSelect={() => onToggleSelect(entry.id)}
            onToggleExpand={() => onToggleExpand(entry.id)}
            onClearExpand={onClearExpand}
            onDelete={() => onDelete(entry.id)}
            onAsk={() => handleAsk(entry)}
            onOrganize={() => onOrganize([entry.id])}
            onBeginDragSelect={() => {
              if (!selectedIds.has(entry.id)) {
                onToggleSelect(entry.id);
              }
              setIsDragSelecting(true);
            }}
            onDragSelect={() => {
              if (isDragSelecting && !selectedIds.has(entry.id)) {
                onToggleSelect(entry.id);
              }
            }}
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
            <span>{formatRawFriendlyTime(entry.ingested_at)}</span>
            <span>{formatRawAmount(entry.byte_size)}</span>
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
          <span className="text-muted-foreground/50">{formatRawFriendlyTime(entry.ingested_at || entry.date)}</span>
          <span className="text-muted-foreground/40">{formatRawAmount(entry.byte_size)}</span>
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

/**
 * Raw Library -- Material Manager (redesigned)
 *
 * Full-width card list with collapsed input behind "添加" button.
 * Search, batch actions, expandable detail, multi-select delete.
 */

import { useState, useRef, useCallback, useMemo, useEffect } from "react";
import { useSearchParams } from "react-router-dom";
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
  ChevronDown,
  MessageSquare,
  Globe,
  X,
  File,
  Maximize2,
} from "lucide-react";
import { listRawEntries, getRawEntry } from "@/features/ingest/persist";
import { ingestText } from "@/features/ingest/adapters/text";
import { ingestUrl } from "@/features/ingest/adapters/url";
import { fetchJson } from "@/lib/desktop/transport";
import type { RawEntry } from "@/features/ingest/types";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

const rawKeys = {
  list: () => ["wiki", "raw", "list"] as const,
  detail: (id: number) => ["wiki", "raw", "detail", id] as const,
};

/* ─── Source helpers ──────────────────────────────────────────────── */

/** Translate known source labels to Chinese */
function translateSource(source: string): string {
  const map: Record<string, string> = {
    "wechat-url": "微信链接",
    "wechat-text": "微信消息",
    "wechat-article": "微信文章",
    "paste-text": "粘贴文本",
    "paste-url": "粘贴链接",
    paste: "粘贴",
    url: "网页",
    pdf: "PDF 文件",
    docx: "Word 文件",
    pptx: "PPT 文件",
    image: "图片",
  };
  return map[source] ?? source;
}

/** Color chip style per source category */
function sourceBadgeStyle(source: string): { bg: string; text: string } {
  if (source.startsWith("wechat")) return { bg: "rgba(34,197,94,0.12)", text: "rgb(22,163,74)" };
  if (source === "url" || source === "paste-url" || source === "wechat-url")
    return { bg: "rgba(59,130,246,0.12)", text: "rgb(37,99,235)" };
  if (["pdf", "docx", "pptx", "image"].includes(source))
    return { bg: "rgba(168,85,247,0.12)", text: "rgb(147,51,234)" };
  return { bg: "rgba(156,163,175,0.12)", text: "rgb(107,114,128)" };
}

/** Icon per source type */
function SourceIcon({ source, className, style }: { source: string; className?: string; style?: React.CSSProperties }) {
  if (source.startsWith("wechat")) return <MessageSquare className={className} style={style} />;
  if (source === "url" || source === "paste-url") return <Globe className={className} style={style} />;
  if (["pdf", "docx", "pptx", "image"].includes(source)) return <File className={className} style={style} />;
  return <FileText className={className} style={style} />;
}

/** Format byte size to human-readable */
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/* ─── Main page ──────────────────────────────────────────────────── */

export function RawLibraryPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [expandedId, setExpandedId] = useState<number | null>(() => {
    const param = searchParams.get("entry");
    return param ? Number(param) || null : null;
  });
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
        setSearchParams({}, { replace: true });
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
      setSearchParams({}, { replace: true });
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
        setSearchParams({}, { replace: true });
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
    setSearchParams({ entry: String(entry.id) }, { replace: true });
    setShowAddPanel(false);
  };

  // Scroll deep-linked entry into view on initial mount
  useEffect(() => {
    if (expandedId !== null) {
      requestAnimationFrame(() => {
        const el = document.getElementById(`raw-entry-${expandedId}`);
        el?.scrollIntoView({ behavior: "smooth", block: "nearest" });
      });
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Only on mount — not on every selection change

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
              所有入库的素材 — 微信文章、链接、文件、粘贴内容
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

      {/* ── Main card list ──────────────────────────────────────── */}
      <div className="flex-1 overflow-y-auto">
        <CardList
          entries={filteredEntries}
          isLoading={listQuery.isLoading}
          error={listQuery.error}
          expandedId={expandedId}
          onToggleExpand={(id) => {
            const next = expandedId === id ? null : id;
            setExpandedId(next);
            if (next !== null) {
              setSearchParams({ entry: String(next) }, { replace: true });
            } else {
              setSearchParams({}, { replace: true });
            }
          }}
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

  const ingestMutation = useMutation({
    mutationFn: async () => {
      if (mode === "text") {
        if (!body.trim()) throw new Error("内容不能为空");
        return ingestText({ title, body });
      }
      if (!url.trim()) throw new Error("链接不能为空");
      return ingestUrl({ url, title });
    },
    onSuccess: (entry) => {
      void queryClient.invalidateQueries({ queryKey: rawKeys.list() });
      setTitle("");
      setBody("");
      setUrl("");
      setErrorMessage(null);
      setSuccessMessage(null);
      onIngested(entry);
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
            disabled={ingestMutation.isPending}
            className="flex shrink-0 items-center gap-1 self-end rounded-md bg-primary px-3 py-1.5 font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
            style={{ fontSize: 13 }}
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
  onDelete,
  deletingId,
  selectedIds,
  onToggleSelect,
}: CardListProps) {
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
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <div className="mb-3 text-3xl opacity-15">📦</div>
        <p className="text-muted-foreground/60" style={{ fontSize: 13 }}>
          暂无素材。通过微信转发、Ask 对话发链接、或点击「添加」手动入库。
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 px-6 py-3">
      {entries.map((entry) => (
        <EntryCard
          key={entry.id}
          entry={entry}
          isExpanded={entry.id === expandedId}
          onToggleExpand={() => onToggleExpand(entry.id)}
          onDelete={() => onDelete(entry.id)}
          isDeleting={deletingId === entry.id}
          isSelected={selectedIds.has(entry.id)}
          onToggleSelect={() => onToggleSelect(entry.id)}
        />
      ))}
    </div>
  );
}

/* ─── Entry card ──────────────────────────────────────────────────── */

interface EntryCardProps {
  entry: RawEntry;
  isExpanded: boolean;
  onToggleExpand: () => void;
  onDelete: () => void;
  isDeleting: boolean;
  isSelected: boolean;
  onToggleSelect: () => void;
}

function EntryCard({
  entry,
  isExpanded,
  onToggleExpand,
  onDelete,
  isDeleting,
  isSelected,
  onToggleSelect,
}: EntryCardProps) {
  const badge = sourceBadgeStyle(entry.source);

  return (
    <div
      id={`raw-entry-${entry.id}`}
      className="group rounded-xl border bg-card p-4 shadow-warm-ring transition-shadow hover:shadow-warm-ring-hover"
      style={{
        borderLeft: isExpanded ? "3px solid var(--color-primary)" : undefined,
      }}
    >
      {/* Card row */}
      <div className="flex items-center gap-3">
        {/* Checkbox for multi-select */}
        <input
          type="checkbox"
          checked={isSelected}
          onChange={(e) => {
            e.stopPropagation();
            onToggleSelect();
          }}
          className="size-3.5 shrink-0 cursor-pointer rounded border-border accent-primary"
        />

        {/* Clickable content area */}
        <button
          type="button"
          onClick={onToggleExpand}
          className="flex min-w-0 flex-1 items-start gap-3 text-left"
        >
          {/* Source icon */}
          <div
            className="flex size-8 shrink-0 items-center justify-center rounded-md"
            style={{ backgroundColor: badge.bg }}
          >
            <SourceIcon source={entry.source} className="size-4" style={{ color: badge.text }} />
          </div>

          {/* Title + metadata */}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              {/* Source badge */}
              <span
                className="shrink-0 rounded-full px-1.5 py-0.5 font-medium"
                style={{
                  fontSize: 10,
                  backgroundColor: badge.bg,
                  color: badge.text,
                }}
              >
                {translateSource(entry.source)}
              </span>
              {/* Date + size */}
              <span className="text-muted-foreground/40" style={{ fontSize: 11 }}>
                {entry.date}
              </span>
              <span className="text-muted-foreground/30" style={{ fontSize: 11 }}>
                {formatSize(entry.byte_size)}
              </span>
            </div>
            {/* Title */}
            <div
              className="mt-0.5 truncate text-foreground"
              style={{ fontSize: 13, fontWeight: isExpanded ? 500 : 400 }}
            >
              {entry.slug}
            </div>
          </div>
        </button>

        {/* Delete button -- visible on hover */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          disabled={isDeleting}
          className="shrink-0 rounded-md p-1.5 text-muted-foreground/30 opacity-0 transition-all hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100 disabled:opacity-50"
          title="删除"
        >
          {isDeleting ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Trash2 className="size-3.5" />
          )}
        </button>

        {/* Expand indicator */}
        <ChevronDown
          className={
            "size-3.5 shrink-0 text-muted-foreground/30 transition-transform " +
            (isExpanded ? "rotate-180" : "")
          }
        />
      </div>

      {/* Expanded detail */}
      {isExpanded && <ExpandedDetail id={entry.id} entry={entry} />}
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
            >
              {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
              {copied ? "已复制" : "复制"}
            </button>
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

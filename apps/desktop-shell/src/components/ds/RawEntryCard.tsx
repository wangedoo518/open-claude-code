import type { ReactNode } from "react";
import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronDown,
  Loader2,
  MessageCircleQuestion,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { getRawEntry } from "@/api/wiki/repository";
import type { RawEntry } from "@/api/wiki/types";
import { RawLineageBadge } from "@/features/raw/RawLineageBadge";
import {
  SourceIcon,
  sourceBadgeStyle,
  translateSource,
} from "@/components/ds/row-primitives";

export interface RawEntryCardProps {
  entry: RawEntry;
  isSelected: boolean;
  isExpanded: boolean;
  isDeleting: boolean;
  batchMode?: boolean;
  isOrganizing?: boolean;
  onToggleSelect: () => void;
  onToggleExpand: () => void;
  onClearExpand: () => void;
  onDelete: () => void;
  onAsk: () => void;
  onOrganize?: () => void;
  onBeginDragSelect?: () => void;
  onDragSelect?: () => void;
  expandedContent?: ReactNode;
}

interface RawPresentation {
  title: string;
  subtitle: string;
  meta: string;
  sourceLabel: string;
  pendingTitle: boolean;
}

const HASHISH_RE = /^(u-|raw-|wechat-)?[a-z0-9_-]{10,}$/i;
const URL_RE = /https?:\/\/[^\s)）]+/i;

function shouldFetchBody(entry: RawEntry): boolean {
  if (entry.source.startsWith("wechat")) return true;
  if (entry.source_url) return true;
  return isMachineTitle(entry.slug) || isMachineTitle(entry.filename);
}

function isMachineTitle(value: string | null | undefined): boolean {
  if (!value) return true;
  const text = value.trim();
  if (!text) return true;
  if (URL_RE.test(text)) return true;
  if (/^mp-weixin-qq-com/i.test(text)) return true;
  if (/^wechat-[a-z0-9_-]+$/i.test(text)) return true;
  return HASHISH_RE.test(text);
}

function cleanTitle(value: string | null | undefined): string {
  return (value ?? "")
    .replace(/^#+\s*/, "")
    .replace(/^\[[^\]]+\]\(([^)]+)\)$/, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

function titleFromBody(body: string | null | undefined): string | null {
  if (!body) return null;
  const lines = body
    .replace(/^---[\s\S]*?---\s*/m, "")
    .split(/\r?\n/)
    .map(cleanTitle)
    .filter((line) => line.length > 0 && !URL_RE.test(line));
  const heading = lines.find((line) => line.length >= 4 && line.length <= 80);
  return heading ?? null;
}

function snippetFromBody(body: string | null | undefined): string | null {
  const text = (body ?? "")
    .replace(/^---[\s\S]*?---\s*/m, "")
    .replace(/[#>*_`[\]()]/g, "")
    .replace(URL_RE, "")
    .replace(/\s+/g, " ")
    .trim();
  if (!text) return null;
  return text.length > 30 ? `${text.slice(0, 30)}…` : text;
}

function hostFrom(entry: RawEntry): string | null {
  const raw = entry.source_url ?? entry.canonical_url ?? entry.original_url ?? extractUrl(entry.slug);
  if (!raw) return null;
  try {
    return new URL(raw).hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
}

function extractUrl(text: string | null | undefined): string | null {
  return text?.match(URL_RE)?.[0] ?? null;
}

function readableFallback(entry: RawEntry): string {
  const raw = cleanTitle(entry.filename || entry.slug);
  if (!raw || isMachineTitle(raw)) return "";
  return raw.replace(/\.[a-z0-9]{2,5}$/i, "").replace(/[-_]+/g, " ");
}

function estimateWords(bytes: number): string {
  const words = Math.max(1, Math.round((bytes || 0) / 2));
  if (words >= 1000) return `约 ${Math.round(words / 100) / 10} 千字`;
  return `约 ${words} 字`;
}

function materialMeta(entry: RawEntry): string {
  const source = entry.source.toLowerCase();
  const host = hostFrom(entry);
  if (host && (source.includes("url") || source === "url" || source.includes("article"))) {
    return host;
  }
  if (source.includes("image") || source === "image") return "图片素材";
  if (source.includes("voice") || source.includes("audio") || source.includes("video")) {
    return "音视频素材";
  }
  return estimateWords(entry.byte_size);
}

function formatMaterialDate(raw: string): string {
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

function buildPresentation(entry: RawEntry, body?: string | null): RawPresentation {
  const source = entry.source.toLowerCase();
  const sourceLabel = translateSource(entry.source);
  const bodyTitle = titleFromBody(body);
  const snippet = snippetFromBody(body);
  const host = hostFrom(entry);
  const fallback = readableFallback(entry);
  const meta = materialMeta(entry);
  const originalRef = host ?? entry.slug;

  if (source === "wechat-text" || source === "wechat-message") {
    if (snippet) {
      return {
        title: `微信消息 · ${snippet}`,
        subtitle: `来源 ${entry.slug}`,
        meta,
        sourceLabel,
        pendingTitle: false,
      };
    }
    return {
      title: source.includes("image") ? "微信图片消息 · 来自微信" : "未命名 · AI 推荐标题中…",
      subtitle: `来源 ${entry.slug}`,
      meta,
      sourceLabel,
      pendingTitle: true,
    };
  }

  if (source === "wechat-article") {
    const title = bodyTitle ?? fallback;
    return title
      ? { title, subtitle: host ?? "微信公众号文章", meta, sourceLabel, pendingTitle: false }
      : {
          title: "未命名 · AI 推荐标题中…",
          subtitle: originalRef,
          meta,
          sourceLabel,
          pendingTitle: true,
        };
  }

  if (source.includes("url") || source === "url") {
    const title = bodyTitle ?? fallback;
    return title
      ? { title, subtitle: host ?? entry.source_url ?? entry.slug, meta, sourceLabel, pendingTitle: false }
      : {
          title: "未命名 · AI 推荐标题中…",
          subtitle: host ?? entry.source_url ?? entry.slug,
          meta,
          sourceLabel,
          pendingTitle: true,
        };
  }

  if (source.includes("image") || source === "image") {
    return {
      title: fallback || "图片素材",
      subtitle: entry.filename || "本地图片",
      meta,
      sourceLabel,
      pendingTitle: false,
    };
  }

  const title = bodyTitle ?? fallback;
  return title
    ? { title, subtitle: entry.filename || entry.slug, meta, sourceLabel, pendingTitle: false }
    : {
        title: "未命名 · AI 推荐标题中…",
        subtitle: entry.filename || entry.slug,
        meta,
        sourceLabel,
        pendingTitle: true,
      };
}

export function RawEntryCard({
  entry,
  isSelected,
  isExpanded,
  isDeleting,
  batchMode = false,
  isOrganizing = false,
  onToggleSelect,
  onToggleExpand,
  onClearExpand,
  onDelete,
  onAsk,
  onOrganize,
  onBeginDragSelect,
  onDragSelect,
  expandedContent,
}: RawEntryCardProps) {
  const badge = sourceBadgeStyle(entry.source);
  const detailQuery = useQuery({
    queryKey: ["wiki", "raw", "detail", entry.id],
    queryFn: () => getRawEntry(entry.id),
    enabled: shouldFetchBody(entry),
    staleTime: 60_000,
  });

  const presentation = useMemo(
    () => buildPresentation(entry, detailQuery.data?.body ?? null),
    [detailQuery.data?.body, entry],
  );

  return (
    <article
      id={`raw-entry-${entry.id}`}
      className="raw-entry-card-v2 group"
      data-expanded={isExpanded || undefined}
      data-selected={isSelected || undefined}
      data-batch={batchMode || isSelected || undefined}
      onMouseDown={(event) => {
        if (batchMode && event.button === 0) {
          onBeginDragSelect?.();
        }
      }}
      onMouseEnter={() => {
        if (batchMode) {
          onDragSelect?.();
        }
      }}
    >
      <div className="raw-entry-card-v2-row">
        <label
          className="raw-entry-card-v2-check"
          aria-label={`选择素材 ${entry.id}`}
          onClick={(event) => event.stopPropagation()}
        >
          <input
            type="checkbox"
            checked={isSelected}
            onChange={onToggleSelect}
          />
        </label>

        <button
          type="button"
          className="raw-entry-card-v2-main"
          aria-expanded={isExpanded}
          onClick={() => {
            if (!batchMode) {
              onToggleExpand();
            }
          }}
          title={`${presentation.title} · ${presentation.subtitle}`}
        >
          <span
            className="raw-entry-card-v2-icon"
            style={
              presentation.pendingTitle
                ? { backgroundColor: "#FAECE7", color: "var(--claude-orange)" }
                : { backgroundColor: badge.bg, color: badge.text }
            }
          >
            {presentation.pendingTitle ? (
              <Sparkles className="size-4" strokeWidth={1.5} />
            ) : (
              <SourceIcon source={entry.source} className="size-4" />
            )}
          </span>

          <span className="raw-entry-card-v2-copy">
            <span className="raw-entry-card-v2-title-row">
              <span
                className="raw-entry-card-v2-title"
                data-pending={presentation.pendingTitle || undefined}
              >
                {presentation.title}
              </span>
              <span
                className="raw-entry-card-v2-source"
                style={{ backgroundColor: badge.bg, color: badge.text }}
              >
                {presentation.sourceLabel}
              </span>
            </span>
            <span className="raw-entry-card-v2-subtitle">
              {presentation.subtitle}
            </span>
            <span className="raw-entry-card-v2-meta">
              <span>{formatMaterialDate(entry.ingested_at || entry.date)}</span>
              <span>{presentation.meta}</span>
              <RawLineageBadge rawId={entry.id} onOrganize={onOrganize} />
            </span>
          </span>
        </button>

        <div className="raw-entry-card-v2-actions">
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onAsk();
            }}
            className="raw-entry-card-v2-action"
            title="用这条素材提问"
            aria-label="用这条素材提问"
          >
            <MessageCircleQuestion className="size-3.5" />
          </button>

          {onOrganize && (
            <button
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                onOrganize();
              }}
              disabled={isOrganizing}
              className="raw-entry-card-v2-action raw-entry-card-v2-action--organize"
              title="重新整理这条素材"
              aria-label="重新整理这条素材"
            >
              {isOrganizing ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Sparkles className="size-3.5" />
              )}
            </button>
          )}

          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onDelete();
            }}
            disabled={isDeleting}
            className="raw-entry-card-v2-action raw-entry-card-v2-action--delete"
            title="删除"
            aria-label="删除"
          >
            {isDeleting ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Trash2 className="size-3.5" />
            )}
          </button>

          {isExpanded && (
            <button
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                onClearExpand();
              }}
              title="清除聚焦"
              aria-label="清除聚焦"
              className="raw-entry-card-v2-action"
            >
              <X className="size-3.5" />
            </button>
          )}
        </div>

        <ChevronDown
          className="raw-entry-card-v2-chevron size-3.5"
          aria-hidden
        />
      </div>

      {isExpanded && expandedContent}
    </article>
  );
}

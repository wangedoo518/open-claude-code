import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  FileText,
  Loader2,
  Search,
  Sparkles,
} from "lucide-react";
import { getWikiGraph, listWikiPages } from "@/api/wiki/repository";
import type { WikiPageSummary } from "@/api/wiki/types";
import { ConfidenceBadge } from "./components/ConfidenceBadge";

type SortMode = "recent" | "oldest" | "words" | "refs";
type FilterMode = "all" | "concept" | "derived";
type GroupKey = "today" | "yesterday" | "week" | "older";

const GROUPS: Array<{ key: GroupKey; label: string }> = [
  { key: "today", label: "今天" },
  { key: "yesterday", label: "昨天" },
  { key: "week", label: "本周" },
  { key: "older", label: "更早" },
];

const ONE_DAY = 24 * 60 * 60 * 1000;
const URL_RE = /https?:\/\/[^\s)）]+/i;

function toTime(raw: string | null | undefined): number {
  if (!raw) return 0;
  const time = new Date(raw).getTime();
  return Number.isNaN(time) ? 0 : time;
}

function classifyPage(page: WikiPageSummary): "concept" | "derived" | "other" {
  const category = page.category?.toLowerCase();
  const slug = page.slug.toLowerCase();
  if (category === "concept" || slug.includes("concept/") || slug.includes("concepts/")) {
    return "concept";
  }
  if (typeof page.source_raw_id === "number" && page.source_raw_id > 0) {
    return "derived";
  }
  return "other";
}

function groupByTime(raw: string): GroupKey {
  const time = toTime(raw);
  if (!time) return "older";
  const now = new Date();
  const date = new Date(time);
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const dayDiff = Math.floor((startOfToday - startOfDate) / ONE_DAY);
  if (dayDiff <= 0) return "today";
  if (dayDiff === 1) return "yesterday";
  if (dayDiff < 7) return "week";
  return "older";
}

function formatKnowledgeTime(raw: string): string {
  const time = toTime(raw);
  if (!time) return "时间未知";
  const date = new Date(time);
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const dayDiff = Math.floor((startOfToday - startOfDate) / ONE_DAY);
  const hh = String(date.getHours()).padStart(2, "0");
  const mm = String(date.getMinutes()).padStart(2, "0");

  if (dayDiff <= 0) return `今天 ${hh}:${mm}`;
  if (dayDiff === 1) return `昨天 ${hh}:${mm}`;
  if (dayDiff < 7) return `${dayDiff} 天前`;
  return `${date.getMonth() + 1} 月 ${date.getDate()} 日 ${hh}:${mm}`;
}

function estimateWords(bytes: number): string {
  const words = Math.max(1, Math.round((bytes || 0) / 2));
  if (words >= 10_000) return `约 ${Math.round(words / 1000) / 10} 万字`;
  if (words >= 1000) return `约 ${Math.round(words / 100) / 10} 千字`;
  return `约 ${words} 字`;
}

function extractUrl(text: string | null | undefined): string | null {
  const match = text?.match(URL_RE);
  return match?.[0] ?? null;
}

function isUntitledPage(page: WikiPageSummary): boolean {
  const title = page.title.trim();
  if (!title) return true;
  if (URL_RE.test(title)) return true;
  if (title !== page.slug) return false;
  return /^[a-f0-9-]{12,}$/i.test(title) || title.length > 36;
}

function displayTitle(page: WikiPageSummary): string {
  return isUntitledPage(page) ? "未命名 · AI 推荐标题中…" : page.title || page.slug;
}

function displaySummary(page: WikiPageSummary): string {
  if (!isUntitledPage(page)) return page.summary || "暂无摘要";
  return extractUrl(page.title) ?? extractUrl(page.summary) ?? page.slug;
}

function pageKindLabel(page: WikiPageSummary): string {
  const kind = classifyPage(page);
  if (kind === "concept") return "概念";
  if (kind === "derived") return "素材衍生";
  return "知识页";
}

export function KnowledgePagesList() {
  const navigate = useNavigate();
  const [sortMode, setSortMode] = useState<SortMode>("recent");
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [searchTerm, setSearchTerm] = useState("");
  const [focusedSlug, setFocusedSlug] = useState<string | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Record<GroupKey, boolean>>({
    today: true,
    yesterday: false,
    week: false,
    older: false,
  });

  const listQuery = useQuery({
    queryKey: ["wiki", "pages", "list"],
    queryFn: () => listWikiPages(),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });

  const graphQuery = useQuery({
    queryKey: ["wiki", "graph", "page-list-degrees"],
    queryFn: () => getWikiGraph(),
    enabled: sortMode === "refs",
    staleTime: 30_000,
  });

  const pages: WikiPageSummary[] = useMemo(
    () => listQuery.data?.pages ?? [],
    [listQuery.data],
  );

  const degreeById = useMemo(() => {
    const degree = new Map<string, number>();
    for (const edge of graphQuery.data?.edges ?? []) {
      degree.set(edge.from, (degree.get(edge.from) ?? 0) + 1);
      degree.set(edge.to, (degree.get(edge.to) ?? 0) + 1);
    }
    for (const node of graphQuery.data?.nodes ?? []) {
      const count = degree.get(node.id);
      if (typeof count === "number") {
        degree.set(node.label, Math.max(degree.get(node.label) ?? 0, count));
      }
    }
    return degree;
  }, [graphQuery.data]);

  const displayPages = useMemo(() => {
    const q = searchTerm.trim().toLowerCase();
    return [...pages]
      .filter((page) => {
        const kind = classifyPage(page);
        if (filterMode === "concept" && kind !== "concept") return false;
        if (filterMode === "derived" && kind !== "derived") return false;
        if (!q) return true;
        return [page.title, page.summary, page.slug, pageKindLabel(page)]
          .join(" ")
          .toLowerCase()
          .includes(q);
      })
      .sort((a, b) => {
        if (sortMode === "oldest") return toTime(a.created_at) - toTime(b.created_at);
        if (sortMode === "words") return b.byte_size - a.byte_size;
        if (sortMode === "refs") {
          const da = degreeById.get(a.slug) ?? degreeById.get(a.title) ?? 0;
          const db = degreeById.get(b.slug) ?? degreeById.get(b.title) ?? 0;
          if (db !== da) return db - da;
        }
        return toTime(b.created_at) - toTime(a.created_at);
      });
  }, [degreeById, filterMode, pages, searchTerm, sortMode]);

  const groupedPages = useMemo(() => {
    const groups: Record<GroupKey, WikiPageSummary[]> = {
      today: [],
      yesterday: [],
      week: [],
      older: [],
    };
    for (const page of displayPages) {
      groups[groupByTime(page.created_at)].push(page);
    }
    return groups;
  }, [displayPages]);

  const rowIndexBySlug = useMemo(
    () => new Map(displayPages.map((page, index) => [page.slug, index])),
    [displayPages],
  );

  const total = listQuery.data?.total_count ?? pages.length;
  const largeList = pages.length >= 50;
  const searchActive = searchTerm.trim().length > 0;
  const defaultFocusedSlug = displayPages[1]?.slug ?? displayPages[0]?.slug ?? null;
  const activeFocusedSlug = focusedSlug ?? defaultFocusedSlug;

  if (listQuery.isLoading) {
    return (
      <div className="ds-kb-shell">
        <div className="ds-kb-header">
          <div>
            <h2 className="ds-kb-h">已整理的知识页面</h2>
            <p className="ds-kb-sub">正在加载…</p>
          </div>
        </div>
        <div className="flex items-center gap-2 text-muted-foreground text-[12px]">
          <Loader2 className="size-3 animate-spin" strokeWidth={1.5} />
          加载页面列表…
        </div>
      </div>
    );
  }

  if (listQuery.error) {
    return (
      <div className="ds-kb-shell">
        <div className="ds-kb-header">
          <div>
            <h2 className="ds-kb-h">已整理的知识页面</h2>
            <p className="ds-kb-sub">页面列表暂时不可用。</p>
          </div>
        </div>
        <div className="ds-kb-error">
          加载失败：
          {listQuery.error instanceof Error
            ? listQuery.error.message
            : String(listQuery.error)}
        </div>
      </div>
    );
  }

  if (pages.length === 0) {
    return (
      <div className="ds-kb-shell">
        <div className="ds-kb-header">
          <div>
            <h2 className="ds-kb-h">已整理的知识页面</h2>
            <p className="ds-kb-sub">知识页会在你批准整理建议后自动出现。</p>
          </div>
        </div>
        <div className="ds-empty-state">
          <BookOpen
            className="size-10"
            strokeWidth={1.2}
            style={{ color: "var(--color-muted-foreground)", marginBottom: 12 }}
          />
          <div className="ds-empty-title">还没有整理出知识页</div>
          <p className="ds-empty-desc">先去待整理批准一条 Maintainer 建议。</p>
          <button
            type="button"
            className="ds-empty-action"
            onClick={() => navigate("/inbox")}
          >
            去待整理 →
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="ds-kb-shell">
      <div className="ds-kb-header">
        <div className="ds-kb-header-main">
          <h2 className="ds-kb-h">已整理的知识页面</h2>
          <p className="ds-kb-sub">
            共 <b>{total}</b> 个页面 · 按当前筛选显示 {displayPages.length} 个
            {largeList ? " · 已启用分组折叠" : ""}
          </p>
        </div>

        <div className="ds-kb-toolbar" aria-label="知识页面筛选排序">
          <label className="ds-kb-search">
            <Search className="size-3.5" strokeWidth={1.5} />
            <input
              value={searchTerm}
              onChange={(event) => setSearchTerm(event.target.value)}
              placeholder="搜索标题、摘要或来源…"
            />
          </label>
          <select
            className="ds-kb-sort"
            value={sortMode}
            onChange={(event) => setSortMode(event.target.value as SortMode)}
            aria-label="排序"
          >
            <option value="recent">最近更新</option>
            <option value="oldest">最早创建</option>
            <option value="words">字数最多</option>
            <option value="refs">引用最多</option>
          </select>
          <div className="ds-kb-filter" role="group" aria-label="筛选">
            {[
              ["all", "全部"],
              ["concept", "概念"],
              ["derived", "素材衍生"],
            ].map(([value, label]) => (
              <button
                key={value}
                type="button"
                data-active={filterMode === value}
                onClick={() => setFilterMode(value as FilterMode)}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {displayPages.length === 0 ? (
        <div className="ds-empty-state ds-empty-state--compact">
          <div className="ds-empty-title">没有找到匹配页面</div>
          <p className="ds-empty-desc">换一个关键词，或切回「全部」筛选。</p>
        </div>
      ) : (
        <div className="ds-kb-groups" aria-label="知识页面分组列表">
          {GROUPS.map(({ key, label }) => {
            const items = groupedPages[key];
            if (items.length === 0) return null;
            const expanded = !largeList || searchActive || expandedGroups[key];
            return (
              <section className="ds-kb-group" key={key} aria-label={`${label}页面`}>
                <button
                  type="button"
                  className="ds-kb-group-head"
                  disabled={!largeList || searchActive}
                  onClick={() =>
                    setExpandedGroups((current) => ({
                      ...current,
                      [key]: !current[key],
                    }))
                  }
                >
                  <span>{label}</span>
                  <span className="ds-kb-count-badge">{items.length}</span>
                  {largeList && !searchActive ? (
                    <ChevronDown
                      className="size-3"
                      strokeWidth={1.5}
                      data-collapsed={!expanded}
                    />
                  ) : null}
                </button>

                {expanded ? (
                  <ul className="ds-kb-list">
                    {items.map((page) => {
                      const target = `/wiki/${encodeURIComponent(page.slug)}`;
                      const kind = classifyPage(page);
                      const untitled = isUntitledPage(page);
                      const focused = activeFocusedSlug === page.slug;
                      const rowIndex = rowIndexBySlug.get(page.slug) ?? 0;
                      return (
                        <li key={page.slug}>
                          <button
                            type="button"
                            className="ds-kb-item"
                            data-focused={focused}
                            style={{ animationDelay: `${Math.min(rowIndex, 12) * 30}ms` }}
                            onFocus={() => setFocusedSlug(page.slug)}
                            onMouseEnter={() => setFocusedSlug(page.slug)}
                            onClick={() => navigate(target)}
                          >
                            <span className="ds-kb-item-pin" aria-hidden="true" />
                            <span className="ds-kb-icon" data-kind={kind}>
                              {untitled ? (
                                <Sparkles className="size-4" strokeWidth={1.5} />
                              ) : (
                                <FileText className="size-4" strokeWidth={1.5} />
                              )}
                            </span>
                            <span className="ds-kb-body">
                              <span className="ds-kb-title-row">
                                <span className="ds-kb-title" data-muted={untitled}>
                                  {displayTitle(page)}
                                </span>
                                <span className="ds-kb-type" data-kind={kind}>
                                  {pageKindLabel(page)}
                                </span>
                              </span>
                              <span className="ds-kb-summary">{displaySummary(page)}</span>
                              <span className="ds-kb-meta-row">
                                <span className="ds-kb-meta-source">
                                  <FileText className="size-3" strokeWidth={1.4} />
                                  来自素材
                                </span>
                                <span>{formatKnowledgeTime(page.created_at)}</span>
                                <span>{estimateWords(page.byte_size)}</span>
                                <ConfidenceBadge confidence={page.confidence} />
                              </span>
                            </span>
                            <ChevronRight
                              className="ds-kb-chevron size-4"
                              strokeWidth={1.5}
                              aria-hidden="true"
                            />
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                ) : null}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

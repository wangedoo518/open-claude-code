/**
 * KnowledgePagesList — DS1.2 editorial replacement for the "pages" tab.
 *
 * The pre-DS1.2 `/wiki` page tab mounted WikiExplorerPage → WikiTab,
 * which rendered `wiki/index.md` as raw Markdown ("ClawWiki index /
 * Auto-generated catalog of every wiki page. Canonical §10, Karpathy
 * llm-wiki §'Indexing and logging'. Concept (6) / People (0) / Topic
 * (0) / Compare (0) / No people pages yet.") plus its own inner
 * "Wiki / Graph" tab bar. That content reads as engineering docs to a
 * regular user.
 *
 * DS1.2 replaces that default view with a DS-style list:
 *
 *   ┌──────────────────────────────────────────┐
 *   │ Icon   Title                  Chevron    │
 *   │        Summary (2-line clamp)            │
 *   │        badge · source · updated · size   │
 *   └──────────────────────────────────────────┘
 *
 * Data source unchanged:
 *   GET /api/wiki/pages            → `listWikiPages()`
 *
 * Navigation: clicking a row uses react-router `navigate('/wiki/<slug>')`,
 * which re-enters `/wiki/*` → KnowledgeHubPage. The parent detects the
 * slug segment and mounts `KnowledgeArticleView` (breadcrumb + WikiArticle)
 * instead of this list. No more inner WikiTabBar.
 */

import { useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { BookOpen, FileText, Loader2, ChevronRight, Hash } from "lucide-react";
import { listWikiPages } from "@/features/ingest/persist";
import type { WikiPageSummary } from "@/features/ingest/types";

/* ── Category inference (slug-based best-effort) ─────────────────
 * The backend's `WikiPageSummary` doesn't ship a category field, but
 * the canonical layer layout (`wiki/concepts/*` / `wiki/people/*` /
 * `wiki/topics/*` / `wiki/compare/*`) means most pages carry a
 * category hint in the slug prefix or as the folder shown in index.md.
 * We do a cheap classifier on title/slug heuristics; if nothing
 * matches we just omit the badge. Zero new API calls.
 */
type CategoryKey = "concept" | "person" | "topic" | "compare" | "unknown";

function classifyPage(p: WikiPageSummary): CategoryKey {
  const slug = p.slug.toLowerCase();
  if (slug.includes("people/") || slug.includes("person/")) return "person";
  if (slug.includes("topic/") || slug.includes("topics/")) return "topic";
  if (slug.includes("compare/") || slug.includes("compares/")) return "compare";
  // Default: the dominant category in the canonical layout is concept.
  // We only mark it explicitly when we're confident so non-matches fall
  // to "unknown" (no badge) rather than a noisy label.
  if (slug.includes("concept/") || slug.includes("concepts/")) return "concept";
  return "unknown";
}

const CATEGORY_LABELS: Record<CategoryKey, string> = {
  concept: "概念",
  person: "人物",
  topic: "主题",
  compare: "对比",
  unknown: "未分类",
};

const CATEGORY_BADGE_CLASS: Record<CategoryKey, string> = {
  concept: "ds-kb-badge ds-kb-badge-concept",
  person: "ds-kb-badge ds-kb-badge-person",
  topic: "ds-kb-badge ds-kb-badge-topic",
  compare: "ds-kb-badge",
  unknown: "ds-kb-badge",
};

/* ── Friendly timestamp ──────────────────────────────────────── */

function formatUpdated(raw: string | null | undefined): string {
  if (!raw) return "";
  const ts = new Date(raw).getTime();
  if (Number.isNaN(ts)) return "";
  const diffMs = Date.now() - ts;
  const oneDay = 24 * 60 * 60 * 1000;
  if (diffMs < oneDay) return "今天";
  if (diffMs < 2 * oneDay) return "昨天";
  if (diffMs < 7 * oneDay) return `${Math.floor(diffMs / oneDay)} 天前`;
  // Longer: absolute date like 2026-04-15
  const d = new Date(raw);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

function formatSize(bytes: number): string {
  if (!bytes || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 10) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${Math.round(bytes / 1024)} KB`;
}

/* ── Component ──────────────────────────────────────────────── */

export function KnowledgePagesList() {
  const navigate = useNavigate();
  const listQuery = useQuery({
    queryKey: ["wiki", "pages", "list"],
    queryFn: () => listWikiPages(),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });

  const pages: WikiPageSummary[] = useMemo(
    () => listQuery.data?.pages ?? [],
    [listQuery.data],
  );

  // Sort newest-first by created_at (the only temporal field on the
  // summary type). If the backend later adds updated_at we can swap.
  const sorted = useMemo(
    () =>
      [...pages].sort((a, b) => {
        const ta = new Date(a.created_at).getTime();
        const tb = new Date(b.created_at).getTime();
        return tb - ta;
      }),
    [pages],
  );

  const total = listQuery.data?.total_count ?? pages.length;

  if (listQuery.isLoading) {
    return (
      <div className="ds-kb-shell">
        <div className="ds-kb-header">
          <h2 className="ds-kb-h">已整理的知识页面</h2>
          <p className="ds-kb-sub">正在加载…</p>
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
          <h2 className="ds-kb-h">已整理的知识页面</h2>
        </div>
        <div
          className="rounded-md border px-4 py-3 text-[12px]"
          style={{
            borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--color-error) 4%, transparent)",
            color: "var(--color-error)",
          }}
        >
          加载失败：
          {listQuery.error instanceof Error
            ? listQuery.error.message
            : String(listQuery.error)}
        </div>
      </div>
    );
  }

  if (sorted.length === 0) {
    return (
      <div className="ds-kb-shell">
        <div className="ds-kb-header">
          <h2 className="ds-kb-h">已整理的知识页面</h2>
          <p className="ds-kb-sub">还没有内容，先从一条微信内容或 Ask 开始吧。</p>
        </div>
        <div className="ds-empty-state">
          <BookOpen
            className="size-10"
            strokeWidth={1.2}
            style={{ color: "var(--color-muted-foreground)", marginBottom: 12 }}
          />
          <div className="ds-empty-title">还没有知识页面</div>
          <p className="ds-empty-desc">
            从微信转发一条内容，或在待整理里批准一条提议后，这里会出现第一篇页面。
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="ds-kb-shell">
      <div className="ds-kb-header">
        <h2 className="ds-kb-h">已整理的知识页面</h2>
        <p className="ds-kb-sub">
          共 <b style={{ color: "var(--color-foreground)" }}>{total}</b> 个页面 · 按最近更新排列
        </p>
      </div>

      <ul className="ds-kb-list" aria-label="知识页面列表">
        {sorted.map((p) => {
          const cat = classifyPage(p);
          const updated = formatUpdated(p.created_at);
          const size = formatSize(p.byte_size);
          return (
            <li
              key={p.slug}
              className="ds-kb-item"
              onClick={() => navigate(`/wiki/${encodeURIComponent(p.slug)}`)}
              role="link"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  navigate(`/wiki/${encodeURIComponent(p.slug)}`);
                }
              }}
            >
              <span className="ds-kb-icon">
                <FileText className="size-4" strokeWidth={1.5} />
              </span>
              <div className="min-w-0">
                <div className="ds-kb-title truncate">{p.title || p.slug}</div>
                {p.summary && <p className="ds-kb-summary">{p.summary}</p>}
                <div className="ds-kb-meta-row">
                  {cat !== "unknown" && (
                    <span className={CATEGORY_BADGE_CLASS[cat]}>
                      {CATEGORY_LABELS[cat]}
                    </span>
                  )}
                  {typeof p.source_raw_id === "number" && p.source_raw_id > 0 && (
                    <span className="inline-flex items-center gap-1">
                      <Hash className="size-3" strokeWidth={1.5} />
                      来自素材 #{p.source_raw_id}
                    </span>
                  )}
                  {updated && <span>更新 · {updated}</span>}
                  {size && <span>{size}</span>}
                </div>
              </div>
              <ChevronRight
                className="ds-kb-chevron size-4"
                strokeWidth={1.5}
                aria-hidden="true"
              />
            </li>
          );
        })}
      </ul>
    </div>
  );
}

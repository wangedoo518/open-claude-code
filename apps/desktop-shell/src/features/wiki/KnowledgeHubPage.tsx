/**
 * Knowledge Hub — DS1.2 · editorial inner-surface.
 *
 * Renders `/wiki/*` with:
 *
 *   1. A serif h1 + caption header that frames the whole page.
 *   2. Pill-tabs (页面 / 关系图 / 素材库), URL-recoverable via `?view=`.
 *   3. Tab bodies:
 *        - pages  → KnowledgePagesList (new DS list — no Markdown index,
 *                  no inner Wiki/Graph tab bar, no engineering copy)
 *        - graph  → a small DS intro strip + GraphPage (unchanged)
 *        - raw    → a small DS intro strip + RawLibraryPage (unchanged)
 *   4. When the URL is `/wiki/<slug>` (slug non-empty), render
 *      `KnowledgeArticleView` (breadcrumb + WikiArticle) in place of
 *      the tabs. This covers wiki-internal links from articles /
 *      dashboard / palette that land on a specific page.
 *
 * What changed vs DS1.1 (external surface) / DS1 (IA):
 *   - Pre-DS1.2 the `pages` tab mounted WikiExplorerPage → WikiTab →
 *     "ClawWiki index · Auto-generated catalog …" Markdown with an
 *     inner Wiki/Graph tab bar. That was the main "looks like dev
 *     docs" complaint. KnowledgePagesList replaces it.
 *   - Pre-DS1.2 GraphPage / RawLibraryPage were mounted bare. DS1.2
 *     wraps each with a small serif intro strip so the hub reads as
 *     one continuous surface instead of three unrelated pages.
 *
 * Zero new backend endpoints. All data comes from existing queries.
 */

import { useCallback, useMemo } from "react";
import { useLocation, useSearchParams } from "react-router-dom";
import { BookOpen, FileStack, Network } from "lucide-react";
import { GraphPage } from "@/features/graph/GraphPage";
import { RawLibraryPage } from "@/features/raw/RawLibraryPage";
import { KnowledgePagesList } from "./KnowledgePagesList";
import { KnowledgeArticleView } from "./KnowledgeArticleView";

type HubView = "pages" | "graph" | "raw";

const HUB_TABS: ReadonlyArray<{
  id: HubView;
  label: string;
  icon: typeof BookOpen;
  hint: string;
}> = [
  { id: "pages", label: "页面", icon: BookOpen, hint: "已整理的知识页面" },
  { id: "graph", label: "关系图", icon: Network, hint: "页面之间的关联" },
  { id: "raw", label: "素材库", icon: FileStack, hint: "转发进来的原始内容" },
];

function parseView(raw: string | null): HubView {
  if (raw === "graph" || raw === "raw") return raw;
  return "pages";
}

/**
 * Parse `/wiki/<slug>` from the pathname. Returns the decoded slug if
 * present, `null` otherwise (including bare `/wiki` and `/wiki/`).
 */
function parseArticleSlug(pathname: string): string | null {
  const m = pathname.match(/^\/wiki\/([^/]+)/);
  if (!m) return null;
  try {
    const decoded = decodeURIComponent(m[1]).trim();
    return decoded.length > 0 ? decoded : null;
  } catch {
    return m[1].trim() || null;
  }
}

export function KnowledgeHubPage() {
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();

  const articleSlug = useMemo(
    () => parseArticleSlug(location.pathname),
    [location.pathname],
  );
  const view = useMemo(() => parseView(searchParams.get("view")), [searchParams]);

  const setView = useCallback(
    (next: HubView) => {
      const params = new URLSearchParams(searchParams);
      if (next === "pages") {
        params.delete("view");
      } else {
        params.set("view", next);
      }
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams],
  );

  // Article detail mode — no tabs / no header, just breadcrumb + body.
  if (articleSlug) {
    return (
      <div className="ds-canvas flex h-full min-h-0 flex-col overflow-hidden">
        <KnowledgeArticleView slug={articleSlug} />
      </div>
    );
  }

  return (
    <div className="ds-canvas flex h-full min-h-0 flex-col overflow-hidden">
      {/* DS1.2 header — serif title + caption above the pill-tabs.
          Per the design-system v2 KnowledgeBase.jsx pattern: the hub's
          main title is editorial (serif, 22px), the caption answers
          "what lives here" in plain Chinese, then the pill tabs route
          the user to each surface. */}
      <header className="shrink-0 border-b border-border/40 px-6 py-4">
        <div className="mx-auto w-full max-w-[1040px]">
          <h1
            className="text-foreground"
            style={{
              fontFamily: "var(--font-serif, \"Lora\", Georgia, serif)",
              fontWeight: 500,
              fontSize: 22,
              letterSpacing: "-0.2px",
              lineHeight: 1.25,
            }}
          >
            知识库
          </h1>
          <p
            className="mt-1 text-muted-foreground"
            style={{ fontSize: 13, lineHeight: 1.6 }}
          >
            浏览已整理的页面、关系图和原始素材。
          </p>

          <div className="mt-3 flex items-center gap-3">
            <div
              className="ds-pill-tabs"
              role="tablist"
              aria-label="知识库视图"
            >
              {HUB_TABS.map((t) => {
                const Icon = t.icon;
                const active = t.id === view;
                return (
                  <button
                    key={t.id}
                    id={`knowledge-hub-tab-${t.id}`}
                    type="button"
                    role="tab"
                    aria-selected={active}
                    aria-controls={`knowledge-hub-panel-${t.id}`}
                    onClick={() => setView(t.id)}
                    className="ds-pill-tab"
                    data-active={active || undefined}
                    title={t.hint}
                  >
                    <Icon className="size-3.5" strokeWidth={1.5} />
                    <span>{t.label}</span>
                  </button>
                );
              })}
            </div>
            <span className="hidden text-[11.5px] text-muted-foreground/70 md:inline">
              {HUB_TABS.find((t) => t.id === view)?.hint}
            </span>
          </div>
        </div>
      </header>

      {/* Body — one mounted tab at a time. */}
      <div
        id={`knowledge-hub-panel-${view}`}
        role="tabpanel"
        aria-labelledby={`knowledge-hub-tab-${view}`}
        className="min-h-0 flex-1 overflow-y-auto"
      >
        {view === "pages" && <KnowledgePagesList />}
        {view === "graph" && (
          <>
            <div className="ds-tab-intro">
              <h2>关系图</h2>
              <p>看看页面、素材和概念之间是怎么连接的。拖拽节点可以探索不同分支。</p>
            </div>
            <GraphPage />
          </>
        )}
        {view === "raw" && (
          <>
            <div className="ds-tab-intro">
              <h2>素材库</h2>
              <p>所有原始素材按时间排列。微信文章、网页链接、文件、手动粘贴内容都会落在这里。</p>
            </div>
            <RawLibraryPage />
          </>
        )}
      </div>
    </div>
  );
}

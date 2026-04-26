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
 *        - graph  → embedded GraphPage, no duplicate page hero
 *        - raw    → embedded RawLibraryPage, compact toolbar/search row
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
 *   - DS2.4 compacted the hub chrome: the tab bar carries the current
 *     surface label, so nested Graph/Raw headings are suppressed here.
 *
 * Zero new backend endpoints. All data comes from existing queries.
 */

import { useCallback, useMemo } from "react";
import { useLocation, useSearchParams } from "react-router-dom";
import { BookOpen, FileStack, Network } from "lucide-react";
import { GraphPage } from "@/features/graph/GraphPage";
import { RawLibraryPage } from "@/features/raw/RawLibraryPage";
import { PillTabs } from "@/components/ds/PillTabs";
import { KnowledgePagesList } from "./KnowledgePagesList";
import { KnowledgeArticleView } from "./KnowledgeArticleView";
import { AbsorbTriggerButton } from "./AbsorbTriggerButton";
import { SkillProgressCard } from "./SkillProgressCard";

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
      <header className="ds-knowledge-hub-bar shrink-0">
        <div className="ds-knowledge-hub-inner">
          <div className="ds-knowledge-hub-title">
            <span className="ds-knowledge-hub-kicker">知识库</span>
            <span className="ds-knowledge-hub-hint">
              {HUB_TABS.find((t) => t.id === view)?.hint}
            </span>
          </div>
          <PillTabs
            tabs={HUB_TABS}
            active={view}
            onChange={(id) => setView(id as HubView)}
            ariaLabel="知识库视图"
            idPrefix="knowledge-hub"
          />
          {/* Phase 1 MVP · §9.5 criterion 1 — surface /absorb trigger
              at the hub level. Pre-wiring it lived only inside
              WikiFileTree (compact variant) which the default
              /wiki route never mounts. See backlog item 9. */}
          {view !== "graph" && (
            <div className="ds-knowledge-hub-action">
              <AbsorbTriggerButton />
            </div>
          )}
        </div>
      </header>
      <SkillProgressCard placement={view === "graph" ? "bottom-toast" : "default"} />

      {/* Body — one mounted tab at a time. */}
      <div
        id={`knowledge-hub-panel-${view}`}
        role="tabpanel"
        aria-labelledby={`knowledge-hub-tab-${view}`}
        className="min-h-0 flex-1 overflow-y-auto"
      >
        {view === "pages" && <KnowledgePagesList />}
        {view === "graph" && <GraphPage embedded />}
        {view === "raw" && <RawLibraryPage embedded />}
      </div>
    </div>
  );
}

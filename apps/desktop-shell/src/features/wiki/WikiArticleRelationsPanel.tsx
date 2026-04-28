/**
 * WikiArticleRelationsPanel — G1 sprint.
 *
 * Replaces the single-column `BacklinksSection` previously inlined in
 * `WikiArticle.tsx`. Fetches the fuller `/api/wiki/pages/:slug/graph`
 * response (contract owned by Worker A — see `PageGraph` in
 * `@/lib/tauri`) and renders three inline subsections below the
 * markdown body:
 *
 *   1. Outgoing — pages this page links out to.
 *   2. Backlinks — pages that link back to this page.
 *   3. Related — scorer-surfaced non-adjacent pages with `reasons[]`
 *      caption lines.
 *
 * UX rules (from the G1 brief):
 *   - Panels with zero items are omitted from the render tree so the
 *     article doesn't grow an empty "Relations" header.
 *   - Sprint A update: when *all three* lists are empty, render one
 *     compact line so the article ends cleanly.
 *   - No modal / popover / extra layer: every item is a plain button
 *     that hands off to `navigateToWikiPage` with a per-panel
 *     WikiNavContext so future telemetry can distinguish origins.
 *   - `related[].reasons` render as small muted caption text joined
 *     with " · "; they do not compete with the title for attention.
 */

import { useQuery } from "@tanstack/react-query";
import { ExternalLink, Link2, Network, Sparkles } from "lucide-react";
import { Link } from "react-router-dom";

import { getWikiPageGraph } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { navigateToWikiPage, type WikiNavContext } from "./navigate-helpers";
import { WikiLineagePanel } from "./WikiLineagePanel";

interface WikiArticleRelationsPanelProps {
  slug: string;
  variant?: "default" | "sidebar";
}

type RelationItem = {
  slug: string;
  title: string;
  reasons?: string[];
};

export function WikiArticleRelationsPanel({
  slug,
  variant = "default",
}: WikiArticleRelationsPanelProps) {
  const { data } = useQuery({
    queryKey: ["wiki", "pages", "graph", slug] as const,
    queryFn: () => getWikiPageGraph(slug),
    staleTime: 60_000,
  });

  const outgoing = data?.outgoing ?? [];
  const backlinks = data?.backlinks ?? [];
  const related = data?.related ?? [];
  const isSidebar = variant === "sidebar";
  const hasOutgoing = outgoing.length > 0;
  const hasBacklinks = backlinks.length > 0;
  const hasRelated = related.length > 0;

  return (
    <section
      className={cn(
        "wiki-relations-panel",
        isSidebar
          ? "space-y-4 text-[13px]"
          : "mt-12 border-t border-[var(--color-border)] pt-6 lg:hidden",
      )}
    >
      {hasOutgoing && (
        <RelationSection
          icon={<ExternalLink className="inline size-3" aria-hidden />}
          title="链接到"
          items={outgoing}
          context="wiki-outgoing"
          isSidebar={isSidebar}
        />
      )}

      {hasBacklinks && (
        <RelationSection
          icon={<Link2 className="inline size-3" aria-hidden />}
          title="反链"
          items={backlinks}
          context="wiki-backlink"
          isSidebar={isSidebar}
        />
      )}

      {hasRelated && (
        <RelationSection
          icon={<Sparkles className="inline size-3" aria-hidden />}
          title="相关"
          items={related}
          context="wiki-related"
          isSidebar={isSidebar}
          showReasons
        />
      )}

      {!hasOutgoing && !hasBacklinks && !hasRelated && (
        <div className={cn(
          "italic text-muted-foreground",
          isSidebar ? "text-xs" : "py-2 text-sm",
        )}>
          暂无关系
        </div>
      )}

      <div className={cn(
        "border-t border-[var(--color-border)]/70 pt-3",
        isSidebar ? "mt-4" : "mt-6",
      )}>
        <Link
          to={`/graph?focus=${encodeURIComponent(slug)}`}
          className="inline-flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-primary"
        >
          <Network className="size-3" strokeWidth={1.5} />
          在图谱中查看
        </Link>
      </div>

      {!isSidebar && <WikiLineagePanel slug={slug} />}
    </section>
  );
}

interface RelationSectionProps {
  icon: React.ReactNode;
  title: string;
  items: RelationItem[];
  context: WikiNavContext;
  isSidebar: boolean;
  showReasons?: boolean;
}

function RelationSection({
  icon,
  title,
  items,
  context,
  isSidebar,
  showReasons = false,
}: RelationSectionProps) {
  const limit = isSidebar ? 8 : 20;
  const visibleItems = items.slice(0, limit);
  const hiddenCount = Math.max(0, items.length - limit);

  return (
    <section>
      <h3
        className={cn(
          "mb-2 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-muted-foreground",
          !isSidebar && "mb-3",
        )}
      >
        {icon}
        <span>{title}</span>
        <span className="opacity-60">({items.length})</span>
      </h3>
      <ul className="space-y-1">
        {visibleItems.map((item) => (
          <li key={item.slug}>
            <button
              type="button"
              onClick={() => navigateToWikiPage(item.slug, item.title, context)}
              className={cn(
                "block w-full truncate py-1 text-left text-sm text-foreground transition-colors hover:text-primary",
                !isSidebar && "text-[13px]",
              )}
              title={item.title}
            >
              {item.title}
            </button>
            {showReasons && item.reasons && item.reasons.length > 0 && (
              <div className="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground/70">
                {item.reasons.join(" · ")}
              </div>
            )}
          </li>
        ))}
        {hiddenCount > 0 && (
          <li className="py-1 text-xs italic text-muted-foreground">
            还有 {hiddenCount} 项…
          </li>
        )}
      </ul>
    </section>
  );
}

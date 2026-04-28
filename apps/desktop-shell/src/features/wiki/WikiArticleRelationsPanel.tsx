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
import { ExternalLink, Link2, Sparkles } from "lucide-react";

import { getWikiPageGraph, type PageGraphNode, type RelatedPageHit } from "@/lib/tauri";
import { navigateToWikiPage, type WikiNavContext } from "./navigate-helpers";
import { WikiLineagePanel } from "./WikiLineagePanel";

interface WikiArticleRelationsPanelProps {
  slug: string;
}

export function WikiArticleRelationsPanel({ slug }: WikiArticleRelationsPanelProps) {
  const { data } = useQuery({
    queryKey: ["wiki", "pages", "graph", slug] as const,
    queryFn: () => getWikiPageGraph(slug),
    staleTime: 60_000,
  });

  const outgoing = data?.outgoing ?? [];
  const backlinks = data?.backlinks ?? [];
  const related = data?.related ?? [];

  // Sprint A: keep the empty state short so it does not interrupt
  // the reading flow. P1 lineage still renders below it.
  if (outgoing.length === 0 && backlinks.length === 0 && related.length === 0) {
    return (
      <section className="wiki-relations-panel mt-12 border-t border-[var(--color-border)] pt-6">
        <div className="py-2 text-sm italic text-muted-foreground">
          暂无关系
        </div>
        <WikiLineagePanel slug={slug} />
      </section>
    );
  }

  return (
    <section className="wiki-relations-panel mt-12 border-t border-[var(--color-border)] pt-6">
      {outgoing.length > 0 && (
        <RelationsNodeList
          icon={<ExternalLink className="inline size-3" aria-hidden />}
          labelZh="本页链接到"
          labelEn="Outgoing"
          items={outgoing}
          context="wiki-outgoing"
        />
      )}

      {backlinks.length > 0 && (
        <RelationsNodeList
          icon={<Link2 className="inline size-3" aria-hidden />}
          labelZh="链接到此页"
          labelEn="Backlinks"
          items={backlinks}
          context="wiki-backlink"
        />
      )}

      {related.length > 0 && (
        <RelatedList items={related} />
      )}

      {/* P1 sprint — 4th section: lineage timeline. Rendered below the
          existing G1 relations lists so provenance history is visible
          on every wiki page without altering the G1 logic. */}
      <WikiLineagePanel slug={slug} />
    </section>
  );
}

/* ── Plain link list (outgoing + backlinks share this shape) ────── */

interface RelationsNodeListProps {
  icon: React.ReactNode;
  labelZh: string;
  labelEn: string;
  items: PageGraphNode[];
  context: WikiNavContext;
}

function RelationsNodeList({
  icon,
  labelZh,
  labelEn,
  items,
  context,
}: RelationsNodeListProps) {
  return (
    <div className="mb-5 last:mb-0">
      <h3 className="mb-2 flex items-center gap-1.5 text-[13px] font-medium text-[var(--color-muted-foreground)]">
        {icon}
        <span>
          {labelZh} · {labelEn} ({items.length})
        </span>
      </h3>
      <ul className="space-y-1">
        {items.map((item) => (
          <li key={item.slug}>
            <button
              type="button"
              onClick={() => navigateToWikiPage(item.slug, item.title, context)}
              className="text-[13px] text-[var(--color-primary)] underline decoration-dotted underline-offset-2 hover:decoration-solid"
            >
              {item.title}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/* ── Related list (title + reasons caption) ─────────────────────── */

function RelatedList({ items }: { items: RelatedPageHit[] }) {
  return (
    <div className="mb-0">
      <h3 className="mb-2 flex items-center gap-1.5 text-[13px] font-medium text-[var(--color-muted-foreground)]">
        <Sparkles className="inline size-3" aria-hidden />
        <span>相关页面 · Related ({items.length})</span>
      </h3>
      <ul className="space-y-2">
        {items.map((item) => (
          <li key={item.slug}>
            <button
              type="button"
              onClick={() =>
                navigateToWikiPage(item.slug, item.title, "wiki-related")
              }
              className="text-[13px] text-[var(--color-primary)] underline decoration-dotted underline-offset-2 hover:decoration-solid"
            >
              {item.title}
            </button>
            {item.reasons.length > 0 && (
              <div className="mt-0.5 text-[11px] text-[var(--color-muted-foreground)] opacity-70">
                {item.reasons.join(" · ")}
              </div>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

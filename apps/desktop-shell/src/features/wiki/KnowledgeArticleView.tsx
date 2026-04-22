/**
 * KnowledgeArticleView — DS1.2 breadcrumb + WikiArticle host.
 *
 * Mounted by `KnowledgeHubPage` when the URL matches `/wiki/<slug>`.
 * Keeps the existing `WikiArticle` component unchanged (including its
 * markdown renderer and relations panel) and just adds:
 *
 *   1. A DS-style breadcrumb:  「知识库 / {title}」
 *   2. A back link to the pages list
 *
 * Does NOT mount WikiTab / WikiTabBar / SkillProgressCard — those were
 * the source of the pre-DS1.2 "inner Wiki/Graph tab" visual that the
 * user asked us to remove.
 */

import { ChevronLeft } from "lucide-react";
import { Link } from "react-router-dom";
import { WikiArticle } from "./WikiArticle";

export function KnowledgeArticleView({ slug }: { slug: string }) {
  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div className="shrink-0 border-b border-border/40 px-6 py-3">
        <div className="ds-breadcrumb">
          <Link to="/wiki" className="inline-flex items-center gap-1">
            <ChevronLeft className="size-3.5" strokeWidth={1.5} />
            知识库
          </Link>
          <span className="ds-breadcrumb-sep">/</span>
          <span className="ds-breadcrumb-current truncate">{decodeURIComponent(slug)}</span>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <WikiArticle slug={slug} />
      </div>
    </div>
  );
}

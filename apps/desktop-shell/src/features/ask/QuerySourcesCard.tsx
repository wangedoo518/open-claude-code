/**
 * QuerySourcesCard — displays wiki page sources referenced in a /query answer.
 * Per 01-skill-engine.md §6.4.
 *
 * Each source shows: title + category badge + relevance score + snippet.
 * Click → switch to Wiki mode + open tab.
 */

import { BookOpen } from "lucide-react";
import type { QuerySource } from "@/api/wiki/types";
import { useWikiTabStore } from "@/state/wiki-tab-store";
import { useSettingsStore } from "@/state/settings-store";

interface QuerySourcesCardProps {
  sources: QuerySource[];
}

export function QuerySourcesCard({ sources }: QuerySourcesCardProps) {
  const openTab = useWikiTabStore((s) => s.openTab);
  const setAppMode = useSettingsStore((s) => s.setAppMode);

  if (sources.length === 0) return null;

  const handleClick = (source: QuerySource) => {
    // Switch to Wiki mode and open the source page tab.
    setAppMode("wiki");
    openTab({
      id: source.slug,
      kind: "article",
      slug: source.slug,
      title: source.title,
      closable: true,
    });
  };

  return (
    <div className="mt-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-card)] p-3">
      <div className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold text-[var(--color-muted-foreground)]">
        <BookOpen className="size-3.5" />
        引用来源 ({sources.length})
      </div>
      <div className="space-y-2">
        {sources.map((source) => (
          <button
            key={source.slug}
            onClick={() => handleClick(source)}
            className="flex w-full flex-col gap-0.5 rounded-md p-2 text-left transition-colors hover:bg-[var(--color-accent)]"
          >
            <div className="flex items-center gap-2">
              <span className="text-[12px] font-medium text-[var(--color-primary)]">
                {source.title}
              </span>
              <span className="rounded bg-[var(--color-primary)]/10 px-1.5 py-0.5 text-[9px] font-medium text-[var(--color-primary)]">
                {Math.round(source.relevance_score * 100)}%
              </span>
            </div>
            {source.snippet && (
              <span className="line-clamp-1 text-[11px] text-[var(--color-muted-foreground)]">
                {source.snippet}
              </span>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

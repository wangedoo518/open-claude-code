import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search, X } from "lucide-react";
import { listWikiPages } from "@/api/wiki/repository";
import type { WikiPageSummary } from "@/api/wiki/types";

/**
 * Slice E20.2 — pin wiki pages as source for a draft.
 *
 * Read-only list of available wiki pages with a search filter.
 * Selection is a slug array; parent owns the state and persists via
 * setDraftSources on every change.
 *
 * Strict no-input on the wiki pages themselves — this picker only
 * dispatches selection changes; never mutates page content.
 */
export function DraftSourcePicker({
  selected,
  onChange,
}: {
  selected: ReadonlyArray<string>;
  onChange: (next: string[]) => void;
}) {
  const [query, setQuery] = useState("");
  // Review J — fetch ALL categories (not just concepts) so users
  // can pin people / topic / compare / inspiration pages as
  // sources too. append_wiki_page_expressed_ref already supports
  // any category; the picker was the bottleneck. Distinct query
  // key from the regular pages list so the two caches don't
  // overwrite each other.
  const pagesQuery = useQuery({
    queryKey: ["wiki", "pages", "list", { includeAll: true }],
    queryFn: () => listWikiPages({ includeAll: true }),
  });
  const all: WikiPageSummary[] = pagesQuery.data?.pages ?? [];
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return all;
    return all.filter(
      (p) =>
        p.slug.toLowerCase().includes(q) ||
        p.title.toLowerCase().includes(q),
    );
  }, [all, query]);
  const selectedSet = useMemo(() => new Set(selected), [selected]);

  const toggle = (slug: string) => {
    if (selectedSet.has(slug)) {
      onChange(selected.filter((s) => s !== slug));
    } else {
      onChange([...selected, slug]);
    }
  };

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="text-[12px] font-medium text-muted-foreground">
        来源页面 ({selected.length})
      </div>
      {selected.length > 0 ? (
        <ul className="flex flex-wrap gap-1.5">
          {selected.map((slug) => (
            <li
              key={slug}
              className="inline-flex items-center gap-1 rounded-full border border-border bg-muted px-2 py-0.5 text-[11px]"
            >
              <span className="max-w-[120px] truncate">{slug}</span>
              <button
                type="button"
                onClick={() => toggle(slug)}
                className="text-muted-foreground hover:text-foreground"
                aria-label={`移除 ${slug}`}
              >
                <X className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="flex items-center gap-1.5 rounded-md border border-border bg-background px-2 py-1">
        <Search className="size-3 text-muted-foreground" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索 wiki 页面…"
          className="min-w-0 flex-1 bg-transparent text-[12px] outline-none"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-md border border-border bg-card">
        {pagesQuery.isLoading ? (
          <div className="p-3 text-[11px] text-muted-foreground">加载中…</div>
        ) : null}
        {pagesQuery.isSuccess && filtered.length === 0 ? (
          <div className="p-3 text-[11px] text-muted-foreground">
            没有匹配的页面
          </div>
        ) : null}
        <ul>
          {filtered.map((p) => (
            <li key={p.slug}>
              <button
                type="button"
                onClick={() => toggle(p.slug)}
                className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-[12px] hover:bg-muted/40"
                data-active={selectedSet.has(p.slug) || undefined}
              >
                <input
                  type="checkbox"
                  checked={selectedSet.has(p.slug)}
                  readOnly
                  className="pointer-events-none size-3"
                />
                <span className="min-w-0 flex-1 truncate">{p.title}</span>
                <span className="shrink-0 text-[10px] text-muted-foreground">
                  {p.category ?? ""}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

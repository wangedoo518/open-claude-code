/**
 * WikiPageSearchField — server-backed picker for selecting a Wiki page.
 *
 * Used by the Maintainer Workbench (UpdateExisting action) and any
 * other form surface where the user needs to point at an existing Wiki
 * page by slug. Built on shadcn's `Popover` + cmdk `Command` so it
 * inherits the theme tokens and keyboard navigation from the rest of
 * the shell.
 *
 * Design notes:
 *
 *   - `shouldFilter={false}` — the repo's shadcn wrapper already
 *     defaults to this (see `components/ui/command.tsx`). cmdk's
 *     Sørensen-Dice filter is useless for CJK, and the backend already
 *     does weighted scoring across slug/title/summary/body, so we just
 *     render whatever `GET /api/wiki/search` returns in order.
 *   - Minimum query length is 1 — this is fine for CJK (each char is
 *     already a signal) and is acceptable noise for ASCII given the
 *     300 ms debounce. The task brief explicitly allows "simple
 *     implementation: `>= 1` triggers".
 *   - The selected `slug` is the source of truth; the component keeps
 *     a short-lived title cache so re-opening the popover doesn't
 *     flash "(unknown)" before search results come back.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Check, ChevronsUpDown } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
  CommandLoading,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { searchWikiPages } from "@/api/wiki/repository";
import type { WikiSearchHit } from "@/api/wiki/types";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";

export interface WikiPageSearchFieldProps {
  /** Currently selected slug (controlled). Empty/undefined → unselected. */
  value?: string;
  /** Called when the user picks a hit. */
  onSelect: (slug: string, title: string) => void;
  /** Trigger-button placeholder when nothing is selected. */
  placeholder?: string;
  /** Disable the trigger button. */
  disabled?: boolean;
  /** Extra class on the trigger button. */
  className?: string;
}

const DEBOUNCE_MS = 300;
const MIN_QUERY = 1;
const RESULT_LIMIT = 20;

export function WikiPageSearchField({
  value,
  onSelect,
  placeholder = "搜索 Wiki 页…",
  disabled = false,
  className,
}: WikiPageSearchFieldProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const debouncedQuery = useDebouncedValue(query.trim(), DEBOUNCE_MS);

  // Remember titles we've seen so the trigger button can render a
  // friendly label for the currently selected slug without needing
  // its own fetch. Keyed by slug.
  const titleCacheRef = useRef<Map<string, string>>(new Map());

  const searchQuery = useQuery({
    queryKey: ["wiki-search", debouncedQuery],
    queryFn: () => searchWikiPages(debouncedQuery, RESULT_LIMIT),
    enabled: open && debouncedQuery.length >= MIN_QUERY,
    staleTime: 30_000,
  });

  const hits: WikiSearchHit[] = useMemo(
    () => searchQuery.data?.hits ?? [],
    [searchQuery.data],
  );

  // Refresh title cache from fresh hits so subsequent trigger renders
  // can show the proper title for the currently-selected slug.
  useEffect(() => {
    if (!hits.length) return;
    const cache = titleCacheRef.current;
    for (const h of hits) {
      cache.set(h.page.slug, h.page.title || h.page.slug);
    }
  }, [hits]);

  const selectedTitle = value
    ? titleCacheRef.current.get(value) ?? value
    : "";

  const handleSelect = useCallback(
    (slug: string, title: string) => {
      titleCacheRef.current.set(slug, title);
      onSelect(slug, title);
      setOpen(false);
      setQuery("");
    },
    [onSelect],
  );

  const showEmpty =
    open &&
    debouncedQuery.length >= MIN_QUERY &&
    !searchQuery.isPending &&
    !searchQuery.isFetching &&
    hits.length === 0;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={disabled}
          className={cn(
            "w-full justify-between font-normal",
            !value && "text-muted-foreground",
            className,
          )}
        >
          <span className="truncate">{value ? selectedTitle : placeholder}</span>
          <ChevronsUpDown className="ml-2 size-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        className="w-[var(--radix-popover-trigger-width)] p-0"
        align="start"
      >
        <Command shouldFilter={false}>
          <CommandInput
            placeholder={placeholder}
            value={query}
            onValueChange={setQuery}
          />
          <CommandList>
            {searchQuery.isFetching && debouncedQuery.length >= MIN_QUERY && (
              <CommandLoading>搜索中…</CommandLoading>
            )}
            {showEmpty && <CommandEmpty>未找到匹配页面</CommandEmpty>}
            {debouncedQuery.length < MIN_QUERY && !searchQuery.isFetching && (
              <div className="py-6 text-center text-sm text-muted-foreground">
                输入关键词开始搜索
              </div>
            )}
            {hits.map((hit) => {
              const { slug, title } = hit.page;
              const label = title || slug;
              const isSelected = value === slug;
              return (
                <CommandItem
                  key={slug}
                  value={slug}
                  onSelect={() => handleSelect(slug, label)}
                  className="flex items-center justify-between gap-2"
                >
                  <div className="flex min-w-0 flex-1 items-center gap-2">
                    <Check
                      className={cn(
                        "size-4 shrink-0",
                        isSelected ? "opacity-100" : "opacity-0",
                      )}
                    />
                    <span className="truncate">{label}</span>
                  </div>
                  <span className="ml-2 shrink-0 font-mono text-[10px] text-muted-foreground">
                    {slug}
                  </span>
                </CommandItem>
              );
            })}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FileSymlink, Loader2, RefreshCw, Scissors } from "lucide-react";
import { useMemo, useState } from "react";
import { breakdownWikiPage, listWikiPages } from "@/api/wiki/repository";
import type { BreakdownResponse } from "@/api/wiki/types";
import { Button } from "@/components/ui/button";

export function BreakdownPage() {
  const queryClient = useQueryClient();
  const [selectedSlug, setSelectedSlug] = useState("");
  const [latest, setLatest] = useState<BreakdownResponse | null>(null);

  const pagesQuery = useQuery({
    queryKey: ["wiki", "pages", "breakdown"],
    queryFn: () => listWikiPages(),
    staleTime: 30_000,
  });

  const pages = pagesQuery.data?.pages ?? [];
  const selectedPage = useMemo(
    () => pages.find((page) => page.slug === selectedSlug) ?? null,
    [pages, selectedSlug],
  );

  const breakdownMutation = useMutation({
    mutationFn: (apply: boolean) =>
      breakdownWikiPage(selectedSlug, { apply, maxTargets: 8 }),
    onSuccess: (data) => {
      setLatest(data);
      void queryClient.invalidateQueries({ queryKey: ["wiki", "pages"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "graph"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "stats"] });
    },
  });

  const isRunning = breakdownMutation.isPending;
  const canRun = selectedSlug.trim().length > 0 && !isRunning;

  return (
    <div className="ds-canvas min-h-full overflow-y-auto px-6 py-6">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-5">
        <header className="rounded-2xl border border-border/60 bg-background/75 p-5 shadow-sm">
          <div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
            <div>
              <div className="mb-2 inline-flex items-center gap-2 rounded-full border border-border/70 px-2.5 py-1 text-[11px] text-muted-foreground">
                <Scissors className="size-3.5" />
                Phase 4 breakdown
              </div>
              <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                Breakdown oversized pages
              </h1>
              <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
                Preview deterministic split targets for long or mixed-topic wiki
                pages. Applying writes new concept pages and keeps the source
                page intact for safe review.
              </p>
            </div>
            <div className="flex shrink-0 gap-2">
              <Button
                variant="outline"
                disabled={!canRun}
                onClick={() => breakdownMutation.mutate(false)}
              >
                {isRunning ? <Loader2 className="mr-2 size-4 animate-spin" /> : <RefreshCw className="mr-2 size-4" />}
                Preview
              </Button>
              <Button
                disabled={!canRun}
                onClick={() => breakdownMutation.mutate(true)}
              >
                {isRunning ? <Loader2 className="mr-2 size-4 animate-spin" /> : <FileSymlink className="mr-2 size-4" />}
                Write targets
              </Button>
            </div>
          </div>
        </header>

        <section className="rounded-2xl border border-border/60 bg-background/75 p-4 shadow-sm">
          <label className="mb-2 block text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Source page
          </label>
          <select
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground shadow-sm focus:outline-none focus:ring-1 focus:ring-ring"
            value={selectedSlug}
            onChange={(event) => {
              setSelectedSlug(event.target.value);
              setLatest(null);
            }}
            disabled={pagesQuery.isLoading}
          >
            <option value="">
              {pagesQuery.isLoading ? "Loading wiki pages..." : "Select a wiki page"}
            </option>
            {pages.map((page) => (
              <option key={page.slug} value={page.slug}>
                {page.title} ({page.slug})
              </option>
            ))}
          </select>
          {selectedPage && (
            <div className="mt-3 rounded-lg border border-border/60 bg-muted/10 px-3 py-2 text-xs text-muted-foreground">
              {selectedPage.summary || "No summary."}
            </div>
          )}
        </section>

        {breakdownMutation.error && (
          <div className="rounded-xl border border-red-500/30 bg-red-500/5 px-4 py-3 text-sm text-red-500">
            {breakdownMutation.error instanceof Error
              ? breakdownMutation.error.message
              : String(breakdownMutation.error)}
          </div>
        )}

        <section className="rounded-2xl border border-border/60 bg-background/75 p-4 shadow-sm">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div>
              <h2 className="text-sm font-semibold text-foreground">
                Split targets
              </h2>
              <p className="text-xs text-muted-foreground">
                {latest
                  ? `${latest.targets.length} target(s), ${latest.reason}`
                  : "Run preview to inspect generated targets."}
              </p>
            </div>
            {latest && (
              <span className="rounded-full border border-border/70 px-2.5 py-1 text-[11px] text-muted-foreground">
                {latest.applied ? "Written" : "Preview only"}
              </span>
            )}
          </div>

          {!latest ? (
            <div className="rounded-xl border border-dashed border-border/70 px-4 py-8 text-center text-sm text-muted-foreground">
              Choose a page, then preview. Nothing writes until you click
              Write targets.
            </div>
          ) : latest.targets.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border/70 px-4 py-8 text-center text-sm text-muted-foreground">
              No useful split points found for this page.
            </div>
          ) : (
            <div className="grid gap-3">
              {latest.targets.map((target) => (
                <article
                  key={target.slug}
                  className="rounded-xl border border-border/70 bg-muted/10 px-4 py-3"
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-semibold text-foreground">
                      {target.title}
                    </span>
                    <code className="rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">
                      {target.slug}
                    </code>
                    <span className="ml-auto text-[11px] text-muted-foreground">
                      {target.word_count} words
                    </span>
                  </div>
                  <p className="mt-2 text-sm text-muted-foreground">
                    {target.summary}
                  </p>
                  <pre className="mt-3 max-h-40 overflow-auto rounded-lg bg-background/80 p-3 text-[11px] leading-5 text-muted-foreground">
                    {target.body}
                  </pre>
                </article>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

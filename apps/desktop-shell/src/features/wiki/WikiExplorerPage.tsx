/**
 * Wiki Pages Explorer
 *
 * Canonical §10 layer contract: `~/.clawwiki/wiki/concepts/` is where
 * the `wiki_maintainer` agent writes its output. After commit
 * `bc3255b feat(wiki-maintainer): engram-style MVP proposes concept
 * pages`, these files really exist and the
 * `/api/wiki/pages{/:slug}` routes are live. This page is the read
 * surface for that data.
 *
 * ## Layout
 *
 *   ┌─────────────┬──────────────────────────────────────────┐
 *   │ Page head (56px · title + counters + refresh)          │
 *   ├─────────────┬──────────────────────────────────────────┤
 *   │ Left 280px  │ Right (flex-1)                           │
 *   │ — slug list │ — WikiPageDetail (serif title + body)    │
 *   │ — grouped   │ — "From raw #N" link                     │
 *   │   by kind    │ — "Planned layers" fallback              │
 *   ├─────────────┴──────────────────────────────────────────┤
 *   │ Planned layers category preview (scrolls with detail)  │
 *   └────────────────────────────────────────────────────────┘
 *
 * The split-pane mirrors `features/inbox/InboxPage.tsx` — same React
 * Query pattern, same `useMemo` sort-then-group, same left-right
 * selection state.
 *
 * ## What's still stubbed
 *
 *   1. Categories other than `concept` (`people` / `topic` / `compare`
 *      / `changelog`) are planned but not yet emitted by any
 *      maintainer pass. The `CATEGORIES` preview stays at the bottom
 *      as signposting.
 *   2. Backlinks aside (right sidebar "referenced by X, X, X") —
 *      needs a link_pages pass to populate.
 *   3. Diff history — needs `manifest.json` sha tracker.
 *
 * Those land in follow-up sprints; the list+detail view does NOT
 * block on them.
 */

import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import ReactMarkdown from "react-markdown";
import { Link } from "react-router-dom";
import {
  BookOpen,
  Brain,
  ArrowRight,
  Loader2,
  RefreshCw,
  FileText,
  Inbox,
  ListTree,
  ScrollText,
  Search,
  X as XIcon,
} from "lucide-react";
import {
  getWikiIndex,
  getWikiLog,
  getWikiPage,
  listWikiPages,
  searchWikiPages,
} from "@/features/ingest/persist";
import type { WikiPageSummary, WikiSearchHit } from "@/features/ingest/types";

/**
 * Selection state for the left pane. Concept pages are identified
 * by their slug; the two special top-level files (`wiki/index.md`
 * and `wiki/log.md`) get dedicated tags so the right pane can fetch
 * them via their dedicated routes instead of treating them like
 * concepts.
 */
type Selection =
  | { kind: "concept"; slug: string }
  | { kind: "index" }
  | { kind: "log" };

function selectionKey(sel: Selection): string {
  if (sel.kind === "concept") return `concept:${sel.slug}`;
  return sel.kind;
}

/* Categories removed — navigation is fully in left sidebar */

const wikiKeys = {
  list: () => ["wiki", "pages", "list"] as const,
  detail: (slug: string) => ["wiki", "pages", "detail", slug] as const,
  index: () => ["wiki", "index"] as const,
  log: () => ["wiki", "log"] as const,
  search: (q: string) => ["wiki", "search", q] as const,
};

/**
 * Debounce a string value. Used so the search query only fires a
 * request once the user has stopped typing for `delay` ms. Saves
 * one request per keystroke on short queries.
 */
function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(t);
  }, [value, delay]);
  return debounced;
}

// ── v2 Wiki Tab (Phase 1 Day 8-10) ─────────────────────────────
// The new multi-tab Wiki Explorer replaces the legacy split-pane layout.
// Once stable, the legacy code below will be removed.
import { WikiTab } from "./WikiTab";

export function WikiExplorerPage() {
  return <WikiTab />;
}

// @ts-expect-error Legacy v1 code preserved for rollback; will be removed after v2 stabilizes.
// eslint-disable-next-line @typescript-eslint/no-unused-vars
function _WikiExplorerPageLegacy() {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<Selection | null>(null);
  const [searchInput, setSearchInput] = useState("");
  const debouncedQuery = useDebouncedValue(searchInput.trim(), 250);

  const listQuery = useQuery({
    queryKey: wikiKeys.list(),
    queryFn: () => listWikiPages(),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });

  // Search fires on the debounced query. Empty query is a no-op
  // (returns an empty WikiSearchResponse from the backend).
  const searchQuery = useQuery({
    queryKey: wikiKeys.search(debouncedQuery),
    queryFn: () => searchWikiPages(debouncedQuery, 30),
    enabled: debouncedQuery.length > 0,
    staleTime: 5_000,
  });

  const isSearching = debouncedQuery.length > 0;
  const searchHits: WikiSearchHit[] = searchQuery.data?.hits ?? [];

  const pages: WikiPageSummary[] = useMemo(
    () => listQuery.data?.pages ?? [],
    [listQuery.data],
  );

  // Auto-select the index file on first load so the right pane is
  // never empty — the index is the canonical entry point per Karpathy.
  useEffect(() => {
    if (selected !== null) return;
    setSelected({ kind: "index" });
  }, [selected]);

  // Clear a stale concept selection when the underlying page gets
  // removed (e.g. user hand-deleted the file, or a future sprint
  // adds an Inbox "deprecate-with-delete" action). Special files
  // (index / log) are always present after F2 lands, so we never
  // clear those.
  useEffect(() => {
    if (selected === null || selected.kind !== "concept") return;
    if (listQuery.isLoading) return;
    if (!pages.some((p) => p.slug === selected.slug)) {
      setSelected({ kind: "index" });
    }
  }, [pages, selected, listQuery.isLoading]);

  const totalCount = listQuery.data?.total_count ?? pages.length;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Page head */}
      {/* Compact header: title + search inline */}
      <div className="flex shrink-0 items-center gap-3 border-b border-border/30 px-4 py-2.5">
        <h1 className="shrink-0 text-foreground" style={{ fontSize: 14, fontWeight: 600 }}>
          Wiki Pages
        </h1>
        <span className="text-muted-foreground/40" style={{ fontSize: 11 }}>
          {totalCount} 页
        </span>
        <div className="relative min-w-0 flex-1">
          <Search
            className="pointer-events-none absolute left-2 top-1/2 size-3 -translate-y-1/2 text-muted-foreground/40"
            aria-hidden="true"
          />
          <input
            type="search"
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            placeholder="搜索知识页面..."
            className="h-7 w-full rounded-md border border-border/30 bg-background pl-7 pr-7 text-foreground placeholder:text-muted-foreground/40 focus:border-primary focus:outline-none"
            style={{ fontSize: 12 }}
          />
          {searchInput && (
            <button
              type="button"
              onClick={() => setSearchInput("")}
              className="absolute right-1 top-1/2 flex size-5 -translate-y-1/2 items-center justify-center rounded text-muted-foreground hover:text-foreground"
              title="清除"
            >
              <XIcon className="size-3" />
            </button>
          )}
        </div>
        <button
          type="button"
          onClick={() => void queryClient.invalidateQueries({ queryKey: wikiKeys.list() })}
          className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground/50 transition-colors hover:text-foreground"
          title="刷新"
        >
          <RefreshCw className={"size-3 " + (listQuery.isFetching ? "animate-spin" : "")} />
        </button>
      </div>

      {/* Body: split pane. Pinned index/log are always visible even
          when there are no concept pages yet, because they give the
          user a way to see "nothing has been maintained" explicitly
          rather than staring at a generic empty state. When searching,
          the left pane swaps to search results ranked by score. */}
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[280px] shrink-0 flex-col overflow-hidden border-r border-border/30">
          {isSearching ? (
            <SearchResultsList
              query={debouncedQuery}
              hits={searchHits}
              isLoading={searchQuery.isLoading || searchQuery.isFetching}
              error={searchQuery.error as Error | null}
              selected={selected}
              onSelect={setSelected}
              totalMatches={searchQuery.data?.total_matches ?? 0}
            />
          ) : (
            <PageList
              pages={pages}
              isLoading={listQuery.isLoading}
              error={listQuery.error}
              selected={selected}
              onSelect={setSelected}
            />
          )}
        </aside>
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {selected === null ? (
            <PagePlaceholder />
          ) : selected.kind === "concept" ? (
            <PageDetail slug={selected.slug} />
          ) : selected.kind === "index" ? (
            <SpecialFilePanel kind="index" />
          ) : (
            <SpecialFilePanel kind="log" />
          )}
        </main>
      </div>

      {pages.length === 0 && !listQuery.isLoading ? (
        <EmptyHeroStrip />
      ) : null}

      {/* Categories integrated into left sidebar — no separate bottom strip */}
    </div>
  );
}

/* ─── Empty hero strip (shown under the split pane when 0 concepts) ── */

function EmptyHeroStrip() {
  return (
    <div className="shrink-0 border-t border-border/50 bg-muted/5 px-6 py-3">
      <div className="flex items-center gap-3 text-caption text-muted-foreground">
        <BookOpen
          className="size-4 shrink-0"
          style={{ color: "var(--claude-orange)" }}
        />
        <span className="flex-1">
          暂无概念页 — 在{" "}
          <Link to="/raw" className="text-primary hover:underline">
            素材库
          </Link>
          {" "}中粘贴一条 URL，然后在{" "}
          <Link to="/inbox" className="text-primary hover:underline">
            Inbox
          </Link>{" "}
          中点击「维护」来生成第一页。
        </span>
        <Link
          to="/inbox"
          className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-caption font-medium text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <Inbox className="size-3" />
          Inbox
          <ArrowRight className="size-3" />
        </Link>
      </div>
    </div>
  );
}

/* ─── Left list (pinned index/log + alphabetical concept list) ─── */

function PageList({
  pages,
  isLoading,
  error,
  selected,
  onSelect,
}: {
  pages: WikiPageSummary[];
  isLoading: boolean;
  error: Error | null;
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
}) {
  const selectedKey = selected ? selectionKey(selected) : null;

  // Sorted alphabetically by slug (backend already sorts, but we
  // re-sort defensively so the display order is deterministic even
  // if a future route returns un-sorted data).
  const sorted = useMemo(
    () => [...pages].sort((a, b) => a.slug.localeCompare(b.slug)),
    [pages],
  );

  return (
    <div className="flex-1 overflow-y-auto">
      {/* Pinned section: the two special files from Karpathy's
          "Indexing and logging" — always present, always at the top. */}
      <div className="border-b border-border/30 pb-2 pt-3">
        <div className="px-4 pb-1 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: "11px", fontWeight: 500 }}>
          置顶
        </div>
        <PinnedItem
          icon={ListTree}
          label="索引"
          hint="内容目录 · 自动生成"
          active={selectedKey === "index"}
          onClick={() => onSelect({ kind: "index" })}
        />
        <PinnedItem
          icon={ScrollText}
          label="日志"
          hint="追加式审计日志"
          active={selectedKey === "log"}
          onClick={() => onSelect({ kind: "log" })}
        />
      </div>

      {/* Concept pages */}
      <div className="flex items-center gap-2 px-4 pb-1 pt-4 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: "11px", fontWeight: 500 }}>
        <span>概念</span>
        {pages.length > 0 && (
          <span className="rounded-full bg-muted/30 px-1.5 text-muted-foreground/50" style={{ fontSize: "10px", fontWeight: 400 }}>
            {pages.length}
          </span>
        )}
      </div>
      {isLoading ? (
        <div className="px-3 py-6 text-center text-caption text-muted-foreground">
          <Loader2 className="mx-auto mb-1.5 size-4 animate-spin" />
          加载中…
        </div>
      ) : error ? (
        <div
          className="m-3 rounded-md border px-3 py-2 text-caption"
          style={{
            borderColor:
              "color-mix(in srgb, var(--color-error) 30%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--color-error) 5%, transparent)",
            color: "var(--color-error)",
          }}
        >
          加载失败：{error.message}
        </div>
      ) : sorted.length === 0 ? (
        <div className="px-4 py-3 text-caption text-muted-foreground/70">
          暂无内容。
        </div>
      ) : (
        <ul>
          {sorted.map((page) => {
            const isActive = selectedKey === `concept:${page.slug}`;
            return (
              <li key={page.slug}>
                <button
                  type="button"
                  onClick={() => onSelect({ kind: "concept", slug: page.slug })}
                  className={
                    "w-full py-2.5 text-left transition-colors " +
                    (isActive
                      ? "border-l-[3px] border-primary pl-[13px]"
                      : "border-l-[3px] border-transparent pl-[13px] hover:bg-accent/20")
                  }
                  style={{ paddingRight: "16px" }}
                >
                  <div className="flex items-center justify-between gap-2">
                    <Brain
                      className="size-3 shrink-0"
                      style={{ color: isActive ? "var(--primary)" : "var(--claude-orange)" }}
                    />
                    <span
                      className="flex-1 truncate text-foreground"
                      style={{
                        fontSize: "13px",
                        fontWeight: isActive ? 500 : 400,
                        lineHeight: "1.4",
                      }}
                    >
                      {page.title || page.slug}
                    </span>
                  </div>
                  <div className="mt-0.5 flex items-center gap-2 pl-5 text-muted-foreground/40" style={{ fontSize: "10px" }}>
                    {page.source_raw_id != null && (
                      <span className="font-mono">
                        raw #{String(page.source_raw_id).padStart(5, "0")}
                      </span>
                    )}
                    <span>{formatRelative(page.created_at)}</span>
                  </div>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/* ─── Search results list (replaces PageList when searching) ──── */

function SearchResultsList({
  query,
  hits,
  isLoading,
  error,
  selected,
  onSelect,
  totalMatches,
}: {
  query: string;
  hits: WikiSearchHit[];
  isLoading: boolean;
  error: Error | null;
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
  totalMatches: number;
}) {
  const selectedKey = selected ? selectionKey(selected) : null;

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="flex items-center gap-2 border-b border-border/30 px-4 py-2">
        <Search className="size-3 text-muted-foreground/50" />
        <span className="flex-1 truncate text-caption text-muted-foreground">
          {isLoading ? (
            "搜索中…"
          ) : (
            <>
              <span className="font-semibold text-foreground">{hits.length}</span>
              {totalMatches > hits.length ? (
                <>
                  {" "}/ {totalMatches}
                </>
              ) : null}{" "}
              条结果 &ldquo;{query}&rdquo;
            </>
          )}
        </span>
      </div>

      {error ? (
        <div
          className="m-3 rounded-md border px-3 py-2 text-caption"
          style={{
            borderColor:
              "color-mix(in srgb, var(--color-error) 30%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--color-error) 5%, transparent)",
            color: "var(--color-error)",
          }}
        >
          加载失败：{error.message}
        </div>
      ) : hits.length === 0 && !isLoading ? (
        <div className="px-4 py-6 text-center text-caption text-muted-foreground">
          <Search className="mx-auto mb-1.5 size-5 opacity-40" />
          <div className="text-body-sm text-foreground/80">暂无结果</div>
          <div className="mt-0.5 text-caption text-muted-foreground/70">
            标题、摘要、正文中没有匹配
            <br />
            &ldquo;{query}&rdquo; 的内容。
          </div>
        </div>
      ) : (
        <ul>
          {hits.map((hit) => {
            const isActive =
              selectedKey === `concept:${hit.page.slug}`;
            return (
              <li key={hit.page.slug}>
                <button
                  type="button"
                  onClick={() =>
                    onSelect({ kind: "concept", slug: hit.page.slug })
                  }
                  className={
                    "w-full py-2.5 text-left transition-colors " +
                    (isActive
                      ? "border-l-[3px] border-primary pl-[13px]"
                      : "border-l-[3px] border-transparent pl-[13px] hover:bg-accent/20")
                  }
                  style={{ paddingRight: "16px" }}
                >
                  <div className="flex items-center justify-between gap-2">
                    <Brain
                      className="size-3 shrink-0"
                      style={{ color: isActive ? "var(--primary)" : "var(--claude-orange)" }}
                    />
                    <span
                      className="flex-1 truncate text-foreground"
                      style={{ fontSize: "13px", fontWeight: isActive ? 500 : 400, lineHeight: "1.4" }}
                    >
                      {hit.page.title || hit.page.slug}
                    </span>
                    <span
                      className="shrink-0 rounded-sm px-1 font-mono text-muted-foreground/50"
                      style={{ fontSize: "10px" }}
                      title={`Relevance score: ${hit.score}`}
                    >
                      {hit.score}
                    </span>
                  </div>
                  {hit.snippet ? (
                    <div className="mt-0.5 line-clamp-2 pl-5 text-muted-foreground/70" style={{ fontSize: "11px" }}>
                      {hit.snippet}
                    </div>
                  ) : hit.page.summary ? (
                    <div className="mt-0.5 line-clamp-2 pl-5 text-muted-foreground/70" style={{ fontSize: "11px" }}>
                      {hit.page.summary}
                    </div>
                  ) : null}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function PinnedItem({
  icon: Icon,
  label,
  hint,
  active,
  onClick,
}: {
  icon: typeof ListTree;
  label: string;
  hint: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={
        "flex w-full items-center gap-2 py-2 text-left transition-colors " +
        (active ? "border-l-[3px] border-primary pl-[13px]" : "border-l-[3px] border-transparent pl-[13px] hover:bg-accent/20")
      }
      style={{ paddingRight: "16px" }}
    >
      <Icon
        className="size-3 shrink-0"
        style={{ color: active ? "var(--primary)" : "var(--claude-orange)" }}
      />
      <span
        className="text-foreground"
        style={{ fontSize: "13px", fontWeight: active ? 500 : 400, lineHeight: "1.4" }}
      >
        {label}
      </span>
      <span className="ml-auto truncate text-muted-foreground/50" style={{ fontSize: "11px" }}>
        {hint}
      </span>
    </button>
  );
}

/* ─── Right detail ───────────────────────────────────────────────── */

function PageDetail({ slug }: { slug: string }) {
  const detailQuery = useQuery({
    queryKey: wikiKeys.detail(slug),
    queryFn: () => getWikiPage(slug),
    staleTime: 30_000,
  });

  if (detailQuery.isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center text-caption text-muted-foreground">
        <Loader2 className="mr-1.5 size-3 animate-spin" />
        加载中…
      </div>
    );
  }

  if (detailQuery.error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
        <div
          className="rounded-md border px-4 py-3 text-body-sm"
          style={{
            borderColor:
              "color-mix(in srgb, var(--color-error) 30%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--color-error) 5%, transparent)",
            color: "var(--color-error)",
          }}
        >
          加载失败：{(detailQuery.error as Error).message}
        </div>
      </div>
    );
  }

  const data = detailQuery.data;
  if (!data) return null;
  const { summary, body } = data;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Head */}
      <div className="shrink-0 border-b border-border/30 px-6 py-5">
        <div className="flex items-center gap-2">
          <StatusPill label="concept" tint="var(--claude-orange)" />
          <StatusPill label="draft" tint="var(--color-warning)" />
        </div>
        <h2
          className="mt-2 text-foreground"
          style={{ fontFamily: "var(--font-serif, Lora, serif)", fontSize: "20px", fontWeight: 600, letterSpacing: "-0.01em", lineHeight: "1.3" }}
        >
          {summary.title || summary.slug}
        </h2>
        {summary.summary ? (
          <p className="mt-1.5 text-muted-foreground" style={{ fontSize: "13px", lineHeight: "1.5" }}>
            {summary.summary}
          </p>
        ) : null}
        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-muted-foreground/50" style={{ fontSize: "11px" }}>
          <span>{summary.created_at}</span>
          <span>{summary.byte_size} B</span>
          {summary.source_raw_id != null && (
            <Link
              to="/raw"
              className="inline-flex items-center gap-1 text-primary/70 hover:text-primary hover:underline"
            >
              <FileText className="size-3" />
              raw #{String(summary.source_raw_id).padStart(5, "0")}
              <ArrowRight className="size-3" />
            </Link>
          )}
        </div>
      </div>

      {/* Body: markdown-rendered */}
      <div className="flex-1 overflow-auto px-6 py-6">
        <article
          className="prose prose-sm max-w-none text-foreground/90"
          style={{
            fontFamily: "var(--font-serif, Lora, serif)",
            fontSize: "14px",
            lineHeight: "1.6",
          }}
        >
          <ReactMarkdown
            components={{
              h1: (props) => (
                <h1
                  className="mb-3 mt-0 text-foreground"
                  style={{ fontFamily: "var(--font-serif, Lora, serif)", fontSize: "18px", fontWeight: 600, letterSpacing: "-0.01em" }}
                  {...props}
                />
              ),
              h2: (props) => (
                <h2
                  className="mb-2 mt-6 uppercase tracking-wide text-foreground"
                  style={{ fontSize: "13px", fontWeight: 600 }}
                  {...props}
                />
              ),
              h3: (props) => (
                <h3
                  className="mb-1.5 mt-5 text-foreground"
                  style={{ fontSize: "13px", fontWeight: 600 }}
                  {...props}
                />
              ),
              p: (props) => (
                <p
                  className="my-2.5 text-foreground/90"
                  style={{ fontSize: "14px", lineHeight: "1.6" }}
                  {...props}
                />
              ),
              ul: (props) => (
                <ul
                  className="my-2.5 list-disc pl-6 text-foreground/90"
                  style={{ fontSize: "14px", lineHeight: "1.6" }}
                  {...props}
                />
              ),
              ol: (props) => (
                <ol
                  className="my-2.5 list-decimal pl-6 text-foreground/90"
                  style={{ fontSize: "14px", lineHeight: "1.6" }}
                  {...props}
                />
              ),
              code: ({ className, children, ...props }) => {
                // Inline code has no language tag.
                const isBlock = /language-/.test(className ?? "");
                if (isBlock) {
                  return (
                    <code
                      className="block overflow-auto rounded-md bg-muted/40 p-3 font-mono text-caption"
                      {...props}
                    >
                      {children}
                    </code>
                  );
                }
                return (
                  <code
                    className="rounded bg-muted/40 px-1 py-0.5 font-mono text-caption"
                    {...props}
                  >
                    {children}
                  </code>
                );
              },
              blockquote: (props) => (
                <blockquote
                  className="my-3 border-l-4 border-border pl-4 text-body italic text-foreground/80"
                  {...props}
                />
              ),
              a: ({ href, children, ...props }) => (
                <a
                  href={href}
                  className="text-primary underline hover:no-underline"
                  target="_blank"
                  rel="noopener noreferrer"
                  {...props}
                >
                  {children}
                </a>
              ),
            }}
          >
            {body}
          </ReactMarkdown>
        </article>
      </div>
    </div>
  );
}

function PagePlaceholder() {
  return (
    <div className="flex flex-1 items-center justify-center p-6 text-center">
      <div className="max-w-sm">
        <BookOpen className="mx-auto mb-2 size-8 opacity-30" />
        <p className="text-body text-muted-foreground">
          选择一项查看内容。
        </p>
      </div>
    </div>
  );
}

/**
 * Read-only viewer for the two special top-level files:
 * `wiki/index.md` (content catalog) and `wiki/log.md` (audit trail).
 * Both are auto-maintained by the desktop-server's
 * `approve-with-write` handler — the user never writes them directly.
 *
 * Both files are plain markdown, so we reuse the same `ReactMarkdown`
 * component stack as `PageDetail`. The one difference is the head
 * strip: special files don't have frontmatter-derived fields
 * (title/summary/source_raw_id), just a short hint + refresh button.
 */
function SpecialFilePanel({ kind }: { kind: "index" | "log" }) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: kind === "index" ? wikiKeys.index() : wikiKeys.log(),
    queryFn: kind === "index" ? getWikiIndex : getWikiLog,
    staleTime: 5_000,
  });

  const meta =
    kind === "index"
      ? {
          icon: ListTree,
          label: "索引",
          description:
            "内容目录 · 每次维护写入后自动重建。Canonical §10 + Karpathy llm-wiki §Indexing.",
        }
      : {
          icon: ScrollText,
          label: "日志",
          description:
            "追加式审计日志 · 维护与入库动作的时间线。Canonical §8 Triggers.",
        };

  if (query.isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center text-caption text-muted-foreground">
        <Loader2 className="mr-1.5 size-3 animate-spin" />
        加载中…
      </div>
    );
  }

  if (query.error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
        <div
          className="rounded-md border px-4 py-3 text-body-sm"
          style={{
            borderColor:
              "color-mix(in srgb, var(--color-error) 30%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--color-error) 5%, transparent)",
            color: "var(--color-error)",
          }}
        >
          加载失败：{(query.error as Error).message}
        </div>
      </div>
    );
  }

  const data = query.data;
  if (!data) return null;
  const Icon = meta.icon;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Head */}
      <div className="shrink-0 border-b border-border/30 border-l-[3px] border-l-primary px-6 py-5">
        <div className="flex items-center gap-2">
          <Icon
            className="size-4"
            style={{ color: "var(--primary)" }}
          />
          <h2
            className="text-foreground"
            style={{ fontFamily: "var(--font-serif, Lora, serif)", fontSize: "20px", fontWeight: 600, letterSpacing: "-0.01em" }}
          >
            {meta.label}
          </h2>
          <span className="ml-auto font-mono text-muted-foreground/50" style={{ fontSize: "11px" }}>
            {data.byte_size} B
          </span>
          <button
            type="button"
            onClick={() =>
              void queryClient.invalidateQueries({
                queryKey: kind === "index" ? wikiKeys.index() : wikiKeys.log(),
              })
            }
            className="flex items-center gap-1 rounded-md border border-border/40 bg-background px-1.5 py-0.5 text-muted-foreground transition-colors hover:bg-muted/30"
            style={{ fontSize: "11px" }}
            title="刷新"
          >
            <RefreshCw
              className={
                "size-3 " + (query.isFetching ? "animate-spin" : "")
              }
            />
          </button>
        </div>
        <p className="mt-1.5 text-muted-foreground" style={{ fontSize: "12px", lineHeight: "1.5" }}>
          {meta.description}
        </p>
        <div className="mt-1 font-mono text-muted-foreground/40" style={{ fontSize: "11px", wordBreak: "break-all" }}>
          {data.path}
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-auto px-6 py-6">
        {!data.exists || data.content.length === 0 ? (
          <div className="rounded-md border border-border/30 px-4 py-6 text-center">
            <BookOpen
              className="mx-auto mb-1.5 size-6 opacity-30"
              style={{ color: "var(--claude-orange)" }}
            />
            <div className="text-foreground/80" style={{ fontSize: "13px" }}>
              暂无内容。
            </div>
            <div className="mt-1 text-muted-foreground/60" style={{ fontSize: "11px" }}>
              {kind === "index"
                ? "在 Inbox 中审批维护提案后，目录会在此自动重建。"
                : "在 Inbox 中审批维护提案后，第一条日志会出现在这里。"}
            </div>
          </div>
        ) : (
          <article
            className="prose prose-sm max-w-none text-foreground/90"
            style={{ fontFamily: "var(--font-serif, Lora, serif)", fontSize: "14px", lineHeight: "1.6" }}
          >
            <ReactMarkdown
              components={{
                h1: (props) => (
                  <h1
                    className="mb-3 mt-0 text-foreground"
                    style={{
                      fontFamily: "var(--font-serif, Lora, serif)",
                      fontSize: "18px",
                      fontWeight: 600,
                      letterSpacing: "-0.01em",
                    }}
                    {...props}
                  />
                ),
                h2: (props) => (
                  <h2
                    className="mb-2 mt-6 uppercase tracking-wide text-foreground"
                    style={{ fontSize: "13px", fontWeight: 600 }}
                    {...props}
                  />
                ),
                p: (props) => (
                  <p
                    className="my-2.5 text-foreground/90"
                    style={{ fontSize: "14px", lineHeight: "1.6" }}
                    {...props}
                  />
                ),
                ul: (props) => (
                  <ul
                    className="my-2.5 list-disc pl-6 text-foreground/90"
                    style={{ fontSize: "14px", lineHeight: "1.6" }}
                    {...props}
                  />
                ),
                li: (props) => (
                  <li
                    className="my-0.5 text-foreground/90"
                    style={{ fontSize: "14px", lineHeight: "1.6" }}
                    {...props}
                  />
                ),
                code: ({ className, children, ...props }) => {
                  const isBlock = /language-/.test(className ?? "");
                  if (isBlock) {
                    return (
                      <code
                        className="block overflow-auto rounded-md bg-muted/40 p-3 font-mono text-caption"
                        style={{ wordBreak: "break-all" }}
                        {...props}
                      >
                        {children}
                      </code>
                    );
                  }
                  return (
                    <code
                      className="rounded bg-muted/40 px-1 py-0.5 font-mono text-caption"
                      style={{ wordBreak: "break-all" }}
                      {...props}
                    >
                      {children}
                    </code>
                  );
                },
                a: ({ href, children, ...props }) => (
                  <a
                    href={href}
                    className="text-primary underline hover:no-underline"
                    target="_blank"
                    rel="noopener noreferrer"
                    {...props}
                  >
                    {children}
                  </a>
                ),
                em: (props) => (
                  <em
                    className="italic text-muted-foreground"
                    {...props}
                  />
                ),
              }}
            >
              {data.content}
            </ReactMarkdown>
          </article>
        )}
      </div>
    </div>
  );
}

function StatusPill({ label, tint }: { label: string; tint: string }) {
  return (
    <span
      className="rounded-full border px-1.5 py-0.5 text-caption font-medium"
      style={{
        borderColor: `color-mix(in srgb, ${tint} 40%, transparent)`,
        color: tint,
      }}
    >
      {label}
    </span>
  );
}

/* ─── Time formatting (mirrors InboxPage) ────────────────────────── */

function formatRelative(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;
  const deltaSecs = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (deltaSecs < 60) return `${deltaSecs}s ago`;
  if (deltaSecs < 3600) return `${Math.floor(deltaSecs / 60)}m ago`;
  if (deltaSecs < 86_400) return `${Math.floor(deltaSecs / 3600)}h ago`;
  return `${Math.floor(deltaSecs / 86_400)}d ago`;
}

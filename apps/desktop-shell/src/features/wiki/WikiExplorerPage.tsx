/**
 * Wiki Pages Explorer (wireframes.html §04, §05)
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
  Users,
  Tag,
  GitCompare,
  History,
  ArrowRight,
  Loader2,
  RefreshCw,
  FileText,
  Inbox,
  ListTree,
  ScrollText,
} from "lucide-react";
import {
  getWikiIndex,
  getWikiLog,
  getWikiPage,
  listWikiPages,
} from "@/features/ingest/persist";
import type { WikiPageSummary } from "@/features/ingest/types";

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

const CATEGORIES = [
  {
    key: "concept",
    icon: Brain,
    label: "Concepts",
    description:
      "每个核心想法一页 · 由 raw 层的 WeChat 素材自动汇总成 canonical 状态",
    tint: "var(--claude-orange)",
    ready: true,
  },
  {
    key: "people",
    icon: Users,
    label: "People",
    description: "你引用过的作者 / 研究者 / 同事 · 自动汇总所有相关 raw",
    tint: "var(--claude-blue)",
    ready: false,
  },
  {
    key: "topic",
    icon: Tag,
    label: "Topics",
    description: "主题聚合页 · 跨多个 concept 汇总某一领域的结构化综述",
    tint: "var(--agent-purple)",
    ready: false,
  },
  {
    key: "compare",
    icon: GitCompare,
    label: "Compare",
    description: "A vs B 结构化对比 · 自动维护论据栏",
    tint: "var(--color-warning)",
    ready: false,
  },
  {
    key: "changelog",
    icon: History,
    label: "Changelog",
    description: "每天的维护动作日志 · append-only",
    tint: "var(--color-success)",
    ready: false,
  },
] as const;

const wikiKeys = {
  list: () => ["wiki", "pages", "list"] as const,
  detail: (slug: string) => ["wiki", "pages", "detail", slug] as const,
  index: () => ["wiki", "index"] as const,
  log: () => ["wiki", "log"] as const,
};

export function WikiExplorerPage() {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<Selection | null>(null);

  const listQuery = useQuery({
    queryKey: wikiKeys.list(),
    queryFn: () => listWikiPages(),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });

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
      <div className="flex shrink-0 items-start gap-3 border-b border-border/50 px-6 py-4">
        <div className="text-xl">📖</div>
        <div className="flex-1">
          <h1
            className="text-head font-semibold text-foreground"
            style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
          >
            Wiki Pages · LLM 主笔层
          </h1>
          <p className="mt-0.5 text-label text-muted-foreground">
            AI 帮我长出了什么 · concept pages 由 wiki_maintainer 从 raw 层自动维护 · Lora 衬线正文
          </p>
        </div>
        <div className="flex items-center gap-1.5 text-caption text-muted-foreground">
          <span
            className="rounded-md border border-border bg-background px-1.5 py-0.5"
            style={{ color: "var(--claude-orange)" }}
          >
            {totalCount} {totalCount === 1 ? "page" : "pages"}
          </span>
          <button
            type="button"
            onClick={() =>
              void queryClient.invalidateQueries({ queryKey: wikiKeys.list() })
            }
            className="flex items-center gap-1 rounded-md border border-border bg-background px-1.5 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-muted/30"
            title="Refresh"
          >
            <RefreshCw
              className={
                "size-3 " + (listQuery.isFetching ? "animate-spin" : "")
              }
            />
          </button>
        </div>
      </div>

      {/* Body: split pane. Pinned index/log are always visible even
          when there are no concept pages yet, because they give the
          user a way to see "nothing has been maintained" explicitly
          rather than staring at a generic empty state. */}
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[280px] shrink-0 flex-col overflow-hidden border-r border-border/50">
          <PageList
            pages={pages}
            isLoading={listQuery.isLoading}
            error={listQuery.error}
            selected={selected}
            onSelect={setSelected}
          />
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

      {/* Planned layers strip (always visible as signposting) */}
      <section className="shrink-0 border-t border-border/50 px-6 py-4">
        <h2 className="mb-2 text-caption font-semibold uppercase tracking-wide text-muted-foreground">
          Planned layers
        </h2>
        <ul className="grid gap-2 md:grid-cols-5">
          {CATEGORIES.map((cat) => {
            const Icon = cat.icon;
            const count = cat.key === "concept" ? totalCount : 0;
            return (
              <li
                key={cat.key}
                className={
                  "rounded-md border px-3 py-2 " +
                  (cat.ready
                    ? "border-border bg-background"
                    : "border-border/40 bg-muted/5")
                }
              >
                <div className="flex items-center gap-2">
                  <Icon className="size-3" style={{ color: cat.tint }} />
                  <span
                    className="text-caption font-semibold"
                    style={{ color: cat.tint }}
                  >
                    {cat.label}
                  </span>
                  <span className="ml-auto font-mono text-caption text-muted-foreground">
                    {count}
                  </span>
                </div>
                <p className="mt-0.5 truncate text-caption text-muted-foreground/80">
                  {cat.description}
                </p>
              </li>
            );
          })}
        </ul>
      </section>
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
          No concept pages yet — paste a URL in{" "}
          <Link to="/raw" className="text-primary hover:underline">
            Raw Library
          </Link>
          , then click <em>Maintain this</em> in{" "}
          <Link to="/inbox" className="text-primary hover:underline">
            Inbox
          </Link>{" "}
          to grow your first page.
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
      <div className="border-b border-border/40 bg-muted/10 pb-1 pt-2">
        <div className="px-4 py-1 text-caption font-semibold uppercase tracking-wide text-muted-foreground/70">
          Pinned
        </div>
        <PinnedItem
          icon={ListTree}
          label="Index"
          hint="content catalog · auto-rebuilt"
          active={selectedKey === "index"}
          onClick={() => onSelect({ kind: "index" })}
        />
        <PinnedItem
          icon={ScrollText}
          label="Log"
          hint="append-only audit trail"
          active={selectedKey === "log"}
          onClick={() => onSelect({ kind: "log" })}
        />
      </div>

      {/* Concept pages */}
      <div className="px-4 pb-1 pt-2 text-caption font-semibold uppercase tracking-wide text-muted-foreground/70">
        Concepts {pages.length > 0 ? `(${pages.length})` : ""}
      </div>
      {isLoading ? (
        <div className="px-3 py-6 text-center text-caption text-muted-foreground">
          <Loader2 className="mx-auto mb-1.5 size-4 animate-spin" />
          Loading…
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
          Failed to list wiki pages: {error.message}
        </div>
      ) : sorted.length === 0 ? (
        <div className="px-4 py-3 text-caption text-muted-foreground/70">
          No concept pages yet.
        </div>
      ) : (
        <ul className="divide-y divide-border/40">
          {sorted.map((page) => {
            const isActive = selectedKey === `concept:${page.slug}`;
            return (
              <li key={page.slug}>
                <button
                  type="button"
                  onClick={() => onSelect({ kind: "concept", slug: page.slug })}
                  className={
                    "w-full px-4 py-2.5 text-left transition-colors " +
                    (isActive ? "bg-primary/10" : "hover:bg-accent/40")
                  }
                >
                  <div className="flex items-center justify-between gap-2">
                    <Brain
                      className="size-3 shrink-0"
                      style={{ color: "var(--claude-orange)" }}
                    />
                    <span
                      className="flex-1 truncate text-body-sm font-medium text-foreground"
                      style={{
                        fontFamily: "var(--font-serif, Lora, serif)",
                      }}
                    >
                      {page.title || page.slug}
                    </span>
                  </div>
                  <div className="mt-0.5 truncate pl-5 font-mono text-caption text-muted-foreground/70">
                    {page.slug}
                  </div>
                  {page.summary ? (
                    <div className="mt-0.5 line-clamp-2 pl-5 text-caption text-muted-foreground/80">
                      {page.summary}
                    </div>
                  ) : null}
                  <div className="mt-0.5 flex items-center gap-2 pl-5 text-caption text-muted-foreground/60">
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
        "flex w-full items-center gap-2 px-4 py-1.5 text-left transition-colors " +
        (active ? "bg-primary/10" : "hover:bg-accent/40")
      }
    >
      <Icon
        className="size-3 shrink-0"
        style={{ color: "var(--claude-orange)" }}
      />
      <span
        className="text-body-sm font-semibold text-foreground"
        style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
      >
        {label}
      </span>
      <span className="ml-auto truncate text-caption text-muted-foreground/70">
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
        Loading {slug}…
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
          Failed to load page: {(detailQuery.error as Error).message}
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
      <div className="shrink-0 border-b border-border/50 bg-muted/10 px-6 py-4">
        <div className="flex items-center gap-2">
          <Brain
            className="size-3"
            style={{ color: "var(--claude-orange)" }}
          />
          <span className="font-mono text-caption text-muted-foreground">
            {summary.slug}
          </span>
          <StatusPill label="concept" tint="var(--claude-orange)" />
          <StatusPill label="draft" tint="var(--color-warning)" />
        </div>
        <h2
          className="mt-1.5 text-subhead font-semibold text-foreground"
          style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
        >
          {summary.title || summary.slug}
        </h2>
        {summary.summary ? (
          <p className="mt-1 text-caption text-muted-foreground">
            {summary.summary}
          </p>
        ) : null}
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-caption text-muted-foreground">
          <span>created: {summary.created_at}</span>
          <span>{summary.byte_size} B</span>
          {summary.source_raw_id != null && (
            <Link
              to="/raw"
              className="inline-flex items-center gap-1 text-primary hover:underline"
            >
              <FileText className="size-3" />
              from raw #{String(summary.source_raw_id).padStart(5, "0")}
              <ArrowRight className="size-3" />
            </Link>
          )}
        </div>
      </div>

      {/* Body: markdown-rendered */}
      <div className="flex-1 overflow-auto px-6 py-5">
        <article
          className="prose prose-sm max-w-none text-foreground/90"
          style={{
            fontFamily: "var(--font-serif, Lora, serif)",
          }}
        >
          <ReactMarkdown
            components={{
              h1: (props) => (
                <h1
                  className="mb-3 mt-0 text-head font-semibold text-foreground"
                  style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
                  {...props}
                />
              ),
              h2: (props) => (
                <h2
                  className="mb-2 mt-5 text-subhead font-semibold text-foreground"
                  style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
                  {...props}
                />
              ),
              h3: (props) => (
                <h3
                  className="mb-1.5 mt-4 text-body font-semibold text-foreground"
                  {...props}
                />
              ),
              p: (props) => (
                <p
                  className="my-2 text-body leading-relaxed text-foreground/90"
                  {...props}
                />
              ),
              ul: (props) => (
                <ul
                  className="my-2 list-disc pl-6 text-body text-foreground/90"
                  {...props}
                />
              ),
              ol: (props) => (
                <ol
                  className="my-2 list-decimal pl-6 text-body text-foreground/90"
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
          Select a page on the left to read it.
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
          label: "Index",
          description:
            "Content catalog auto-rebuilt after every maintainer write. Canonical §10 + Karpathy llm-wiki §Indexing.",
        }
      : {
          icon: ScrollText,
          label: "Log",
          description:
            "Append-only timeline of maintainer and ingest actions. Canonical §8 Triggers.",
        };

  if (query.isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center text-caption text-muted-foreground">
        <Loader2 className="mr-1.5 size-3 animate-spin" />
        Loading {meta.label.toLowerCase()}…
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
          Failed to load {meta.label.toLowerCase()}: {(query.error as Error).message}
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
      <div className="shrink-0 border-b border-border/50 bg-muted/10 px-6 py-4">
        <div className="flex items-center gap-2">
          <Icon
            className="size-4"
            style={{ color: "var(--claude-orange)" }}
          />
          <h2
            className="text-subhead font-semibold text-foreground"
            style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
          >
            {meta.label}
          </h2>
          <span className="ml-auto font-mono text-caption text-muted-foreground">
            {data.byte_size} B
          </span>
          <button
            type="button"
            onClick={() =>
              void queryClient.invalidateQueries({
                queryKey: kind === "index" ? wikiKeys.index() : wikiKeys.log(),
              })
            }
            className="flex items-center gap-1 rounded-md border border-border bg-background px-1.5 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-muted/30"
            title="Refresh"
          >
            <RefreshCw
              className={
                "size-3 " + (query.isFetching ? "animate-spin" : "")
              }
            />
          </button>
        </div>
        <p className="mt-1 text-caption text-muted-foreground">
          {meta.description}
        </p>
        <div className="mt-1 font-mono text-caption text-muted-foreground/60">
          {data.path}
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-auto px-6 py-5">
        {!data.exists || data.content.length === 0 ? (
          <div className="rounded-md border border-border/40 bg-muted/10 px-4 py-6 text-center text-caption text-muted-foreground">
            <BookOpen
              className="mx-auto mb-1.5 size-6 opacity-40"
              style={{ color: "var(--claude-orange)" }}
            />
            <div className="text-body-sm text-foreground/90">
              This {meta.label.toLowerCase()} is empty.
            </div>
            <div className="mt-1 text-caption text-muted-foreground/70">
              {kind === "index"
                ? "Approve a maintainer proposal in Inbox and the catalog will rebuild here automatically."
                : "Approve a maintainer proposal in Inbox and the first log entry will land here."}
            </div>
          </div>
        ) : (
          <article
            className="prose prose-sm max-w-none text-foreground/90"
            style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
          >
            <ReactMarkdown
              components={{
                h1: (props) => (
                  <h1
                    className="mb-3 mt-0 text-head font-semibold text-foreground"
                    style={{
                      fontFamily: "var(--font-serif, Lora, serif)",
                    }}
                    {...props}
                  />
                ),
                h2: (props) => (
                  <h2
                    className="mb-2 mt-5 text-subhead font-semibold text-foreground"
                    style={{
                      fontFamily: "var(--font-serif, Lora, serif)",
                    }}
                    {...props}
                  />
                ),
                p: (props) => (
                  <p
                    className="my-2 text-body leading-relaxed text-foreground/90"
                    {...props}
                  />
                ),
                ul: (props) => (
                  <ul
                    className="my-2 list-disc pl-6 text-body text-foreground/90"
                    {...props}
                  />
                ),
                li: (props) => (
                  <li
                    className="my-0.5 text-body text-foreground/90"
                    {...props}
                  />
                ),
                code: ({ className, children, ...props }) => {
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

import { useQuery } from "@tanstack/react-query";
import { BookOpen, ExternalLink, GitBranch, Loader2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { getWikiGraph, getWikiPage, listWikiPages } from "@/api/wiki/repository";
import { parseWikiHref, preprocessWikilinks } from "@/features/wiki/wiki-link-utils";

type ViewerMode =
  | { kind: "home" }
  | { kind: "wiki"; slug: string }
  | { kind: "graph" };

function parseViewerMode(pathname: string): ViewerMode {
  const wikiMatch = pathname.match(/^\/viewer\/wiki\/([^/]+)/);
  if (wikiMatch) {
    try {
      return { kind: "wiki", slug: decodeURIComponent(wikiMatch[1]) };
    } catch {
      return { kind: "wiki", slug: wikiMatch[1] };
    }
  }
  if (pathname === "/viewer/graph" || pathname.startsWith("/viewer/graph/")) {
    return { kind: "graph" };
  }
  return { kind: "home" };
}

export function WebViewerPage() {
  const location = useLocation();
  const mode = parseViewerMode(location.pathname);

  if (mode.kind === "wiki") {
    return <WikiReadOnlyView slug={mode.slug} />;
  }
  if (mode.kind === "graph") {
    return <GraphReadOnlyView />;
  }
  return <ViewerHome />;
}

function ViewerHome() {
  const pagesQuery = useQuery({
    queryKey: ["wiki", "pages", "viewer"],
    queryFn: () => listWikiPages(),
    staleTime: 30_000,
  });
  const pages = pagesQuery.data?.pages ?? [];

  return (
    <div className="ds-canvas min-h-full overflow-y-auto px-6 py-6">
      <main className="mx-auto max-w-4xl rounded-2xl border border-border/60 bg-background/80 p-6 shadow-sm">
        <div className="mb-5 inline-flex items-center gap-2 rounded-full border border-border/70 px-2.5 py-1 text-[11px] text-muted-foreground">
          <BookOpen className="size-3.5" />
          Read-only viewer
        </div>
        <h1 className="text-3xl font-semibold tracking-tight text-foreground">
          Web viewer
        </h1>
        <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
          Stable read-only entrypoints for wiki pages and graph snapshots. Use
          `/viewer/wiki/:slug` for a page and `/viewer/graph` for graph entry.
        </p>

        <div className="mt-6 grid gap-3 md:grid-cols-2">
          <Link
            to="/viewer/graph"
            className="rounded-xl border border-border/70 bg-muted/10 p-4 transition-colors hover:bg-foreground/5"
          >
            <GitBranch className="mb-3 size-5 text-muted-foreground" />
            <div className="text-sm font-semibold text-foreground">
              Graph entrypoint
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
              Inspect node and edge totals without editor affordances.
            </div>
          </Link>
          <Link
            to="/wiki"
            className="rounded-xl border border-border/70 bg-muted/10 p-4 transition-colors hover:bg-foreground/5"
          >
            <ExternalLink className="mb-3 size-5 text-muted-foreground" />
            <div className="text-sm font-semibold text-foreground">
              Open full Wiki
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
              Jump back to the editable desktop surface.
            </div>
          </Link>
        </div>

        <section className="mt-6">
          <h2 className="mb-3 text-sm font-semibold text-foreground">
            Recent wiki pages
          </h2>
          {pagesQuery.isLoading ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
              Loading pages...
            </div>
          ) : pages.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border/70 px-4 py-6 text-center text-sm text-muted-foreground">
              No wiki pages yet.
            </div>
          ) : (
            <div className="grid gap-2">
              {pages.slice(0, 12).map((page) => (
                <Link
                  key={page.slug}
                  to={`/viewer/wiki/${encodeURIComponent(page.slug)}`}
                  className="rounded-lg border border-border/60 px-3 py-2 transition-colors hover:bg-foreground/5"
                >
                  <div className="truncate text-sm font-medium text-foreground">
                    {page.title}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">
                    {page.summary || page.slug}
                  </div>
                </Link>
              ))}
            </div>
          )}
        </section>
      </main>
    </div>
  );
}

function WikiReadOnlyView({ slug }: { slug: string }) {
  const navigate = useNavigate();
  const pageQuery = useQuery({
    queryKey: ["wiki", "pages", "viewer", slug],
    queryFn: () => getWikiPage(slug),
    staleTime: 30_000,
  });
  const components = useViewerMarkdownComponents();

  if (pageQuery.isLoading) {
    return <CenteredStatus label="Loading wiki page..." />;
  }
  if (pageQuery.error || !pageQuery.data) {
    return (
      <CenteredStatus
        label={`Failed to load ${slug}: ${
          pageQuery.error instanceof Error ? pageQuery.error.message : "not found"
        }`}
      />
    );
  }

  const { summary, body } = pageQuery.data;
  const expandedBody = preprocessWikilinks(body);

  return (
    <div className="ds-canvas min-h-full overflow-y-auto px-6 py-6">
      <article className="mx-auto max-w-3xl rounded-2xl border border-border/60 bg-background/85 px-8 py-7 shadow-sm">
        <div className="mb-5 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <Link to="/viewer" className="hover:text-foreground">
            Viewer
          </Link>
          <span>/</span>
          <span>{summary.slug}</span>
          <button
            type="button"
            onClick={() => navigate(`/wiki/${encodeURIComponent(summary.slug)}`)}
            className="ml-auto rounded-md border border-border px-2 py-1 transition-colors hover:bg-foreground/10"
          >
            Open in Wiki
          </button>
        </div>
        <h1 className="text-3xl font-semibold tracking-tight text-foreground">
          {summary.title}
        </h1>
        {summary.summary && (
          <p className="mt-3 text-sm italic leading-6 text-muted-foreground">
            {summary.summary}
          </p>
        )}
        <div className="mt-6 wiki-article-body markdown-content">
          <ReactMarkdown components={components}>{expandedBody}</ReactMarkdown>
        </div>
      </article>
    </div>
  );
}

function GraphReadOnlyView() {
  const graphQuery = useQuery({
    queryKey: ["wiki", "graph", "viewer"],
    queryFn: getWikiGraph,
    staleTime: 30_000,
  });
  const graph = graphQuery.data;

  return (
    <div className="ds-canvas min-h-full overflow-y-auto px-6 py-6">
      <main className="mx-auto max-w-4xl rounded-2xl border border-border/60 bg-background/80 p-6 shadow-sm">
        <div className="mb-5 inline-flex items-center gap-2 rounded-full border border-border/70 px-2.5 py-1 text-[11px] text-muted-foreground">
          <GitBranch className="size-3.5" />
          Read-only graph
        </div>
        <h1 className="text-3xl font-semibold tracking-tight text-foreground">
          Graph entrypoint
        </h1>
        <p className="mt-3 text-sm leading-6 text-muted-foreground">
          A stable graph snapshot for web-style viewing. Open the full graph
          when you need physics navigation and drilldown.
        </p>

        {graphQuery.isLoading ? (
          <div className="mt-6 flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            Loading graph...
          </div>
        ) : graphQuery.error || !graph ? (
          <div className="mt-6 rounded-xl border border-red-500/30 bg-red-500/5 px-4 py-3 text-sm text-red-500">
            {graphQuery.error instanceof Error
              ? graphQuery.error.message
              : "Graph unavailable."}
          </div>
        ) : (
          <>
            <div className="mt-6 grid gap-3 md:grid-cols-3">
              <ViewerMetric label="Raw sources" value={graph.raw_count} />
              <ViewerMetric label="Wiki pages" value={graph.concept_count} />
              <ViewerMetric label="Edges" value={graph.edge_count} />
            </div>
            <div className="mt-6 rounded-xl border border-border/70">
              <div className="border-b border-border/70 px-4 py-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Recent nodes
              </div>
              <div className="divide-y divide-border/60">
                {graph.nodes.slice(0, 20).map((node) => (
                  <div
                    key={node.id}
                    className="flex items-center gap-3 px-4 py-2 text-sm"
                  >
                    <span className="rounded-full bg-foreground/10 px-2 py-0.5 text-[11px] text-muted-foreground">
                      {node.kind}
                    </span>
                    <span className="truncate text-foreground">{node.label}</span>
                    <span className="ml-auto text-xs text-muted-foreground">
                      {node.category}
                    </span>
                  </div>
                ))}
              </div>
            </div>
            <Link
              to="/graph"
              className="mt-5 inline-flex rounded-md border border-border px-3 py-2 text-sm text-foreground transition-colors hover:bg-foreground/10"
            >
              Open interactive graph
            </Link>
          </>
        )}
      </main>
    </div>
  );
}

function useViewerMarkdownComponents(): Components {
  const navigate = useNavigate();
  return {
    a: ({ href, children, ...rest }) => {
      const ref = href ? parseWikiHref(href) : null;
      if (ref) {
        return (
          <a
            href={`/viewer/wiki/${encodeURIComponent(ref.slug)}`}
            onClick={(event) => {
              event.preventDefault();
              navigate(`/viewer/wiki/${encodeURIComponent(ref.slug)}`);
            }}
            className="text-[var(--color-primary)] underline decoration-dotted underline-offset-[3px]"
            {...rest}
          >
            {children}
          </a>
        );
      }
      if (href?.startsWith("http://") || href?.startsWith("https://")) {
        return (
          <a
            href={href}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--color-primary)] underline underline-offset-2"
            {...rest}
          >
            {children}
          </a>
        );
      }
      return (
        <a
          href={href}
          className="text-[var(--color-primary)] underline underline-offset-2"
          {...rest}
        >
          {children}
        </a>
      );
    },
  };
}

function CenteredStatus({ label }: { label: string }) {
  return (
    <div className="ds-canvas flex h-full min-h-80 items-center justify-center px-6 py-6 text-sm text-muted-foreground">
      {label}
    </div>
  );
}

function ViewerMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-xl border border-border/60 bg-muted/10 px-4 py-3">
      <div className="text-[11px] uppercase tracking-wide text-muted-foreground">
        {label}
      </div>
      <div className="mt-1 text-2xl font-semibold text-foreground">
        {value}
      </div>
    </div>
  );
}

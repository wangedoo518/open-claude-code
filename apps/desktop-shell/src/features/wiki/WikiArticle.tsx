/**
 * WikiArticle — Markdown rendering area for a single wiki page.
 * Per component-spec.md §3 and 02-wiki-explorer.md §6.2.
 */

import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";

import { getWikiPage } from "@/features/ingest/persist";
import { useWikiTabStore } from "@/state/wiki-tab-store";
import type { WikiPageSummary } from "@/features/ingest/types";
import {
  preprocessWikilinks,
  useWikiLinkRenderer,
} from "./wiki-link-utils";

/* ── Reading time ──────────────────────────────────────────────── */
function estimateReadingTime(body: string): string {
  // CJK: 400 chars/min; ASCII: 200 words/min
  let cjkChars = 0;
  for (const ch of body) {
    if (ch.charCodeAt(0) > 0x2e7f) cjkChars++;
  }
  const asciiWords = body
    .split(/\s+/)
    .filter((w) => w.length > 0 && w.charCodeAt(0) <= 127).length;

  const minutes = Math.ceil(cjkChars / 400 + asciiWords / 200);
  return minutes <= 1 ? "1 min" : `${minutes} min`;
}

/* ── Category badge colors ─────────────────────────────────────── */
const CATEGORY_STYLES: Record<string, string> = {
  concept: "bg-[var(--color-primary)]/10 text-[var(--color-primary)]",
  people: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
  topic: "bg-purple-500/10 text-purple-600 dark:text-purple-400",
  compare: "bg-yellow-500/10 text-yellow-600 dark:text-yellow-400",
};

/* ── Backlinks section ─────────────────────────────────────────── */
function BacklinksSection({ slug }: { slug: string }) {
  const openTab = useWikiTabStore((s) => s.openTab);
  const { data } = useQuery({
    queryKey: ["wiki", "backlinks", slug],
    queryFn: async () => {
      const { fetchJson } = await import("@/lib/desktop/transport");
      return fetchJson<{ backlinks: Array<{ slug: string; title: string; category: string }> }>(
        `/api/wiki/pages/${encodeURIComponent(slug)}/backlinks`,
      );
    },
    staleTime: 60_000,
  });

  const backlinks = data?.backlinks ?? [];
  if (backlinks.length === 0) return null;

  return (
    <div className="mt-12 border-t border-[var(--color-border)] pt-4">
      <h4 className="mb-2 text-[13px] font-semibold text-[var(--color-muted-foreground)]">
        被引用
      </h4>
      <ul className="space-y-1">
        {backlinks.map((bl) => (
          <li key={bl.slug}>
            <button
              onClick={() =>
                openTab({
                  id: bl.slug,
                  kind: "article",
                  slug: bl.slug,
                  title: bl.title,
                  closable: true,
                })
              }
              className="text-[13px] text-[var(--color-primary)] underline decoration-dotted underline-offset-2 hover:decoration-solid"
            >
              {bl.title}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/* ── Markdown custom components ────────────────────────────────── */
/**
 * Article-page Markdown renderer. The `<a>` handler is shared with
 * the chat-side /query renderer via `useWikiLinkRenderer` — see
 * `wiki-link-utils.tsx` for why and how internal wiki refs are
 * intercepted (short version: raw relative `.md` paths were falling
 * through React Router to `/dashboard`).
 */
function useMarkdownComponents(): Components {
  const Anchor = useWikiLinkRenderer();

  return useMemo(
    (): Components => ({
      h1: ({ children }) => (
        <h1 className="mb-4 mt-6 text-[24px] font-semibold leading-[1.3] text-[var(--color-foreground)]" style={{ fontFamily: 'var(--font-family-dt-serif, "Lora", serif)' }}>
          {children}
        </h1>
      ),
      h2: ({ children }) => (
        <h2 className="mb-3 mt-5 text-[18px] font-semibold leading-[1.3] text-[var(--color-foreground)]" style={{ fontFamily: 'var(--font-family-dt-serif, "Lora", serif)' }}>
          {children}
        </h2>
      ),
      h3: ({ children }) => (
        <h3 className="mb-2 mt-4 text-[15px] font-semibold text-[var(--color-foreground)]">
          {children}
        </h3>
      ),
      p: ({ children }) => (
        <p className="my-4 text-[15px] leading-[1.7] text-[var(--color-foreground)]">
          {children}
        </p>
      ),
      ul: ({ children }) => <ul className="my-3 ml-5 list-disc space-y-1">{children}</ul>,
      ol: ({ children }) => <ol className="my-3 ml-5 list-decimal space-y-1">{children}</ol>,
      li: ({ children }) => (
        <li className="text-[15px] leading-[1.7] text-[var(--color-foreground)]">{children}</li>
      ),
      blockquote: ({ children }) => (
        <blockquote className="my-4 border-l-4 border-[var(--color-border)] pl-4 text-[var(--color-muted-foreground)] italic">
          {children}
        </blockquote>
      ),
      code: ({ className, children }) => {
        const isBlock = className?.includes("language-");
        if (isBlock) {
          return (
            <pre className="my-3 overflow-x-auto rounded-lg bg-[var(--color-secondary)] p-4 text-[13px] leading-[1.5] dark:bg-[var(--color-card)]">
              <code className={className} style={{ fontFamily: 'var(--font-family-dt-mono, monospace)' }}>
                {children}
              </code>
            </pre>
          );
        }
        return (
          <code className="rounded bg-[var(--color-muted)] px-1.5 py-0.5 text-[14px]" style={{ fontFamily: 'var(--font-family-dt-mono, monospace)' }}>
            {children}
          </code>
        );
      },
      a: Anchor,
    }),
    [Anchor],
  );
}

/* ── Main component ────────────────────────────────────────────── */
interface WikiArticleProps {
  slug: string;
}

export function WikiArticle({ slug }: WikiArticleProps) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["wiki", "pages", "detail", slug],
    queryFn: () => getWikiPage(slug),
    staleTime: 30_000,
  });

  const components = useMarkdownComponents();

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center text-[var(--color-muted-foreground)]">
        Loading...
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="flex h-64 items-center justify-center text-[var(--color-destructive)]">
        Failed to load page: {slug}
      </div>
    );
  }

  const { summary, body } = data;
  const category = (summary as WikiPageSummary & { category?: string }).category ?? "concept";
  const categoryStyle = CATEGORY_STYLES[category] ?? CATEGORY_STYLES.concept;
  const readingTime = estimateReadingTime(body);
  const expandedBody = preprocessWikilinks(body);

  return (
    <div className="mx-auto max-w-[720px] px-8 py-6">
      {/* Title — component-spec.md §3.2 */}
      <h1
        className="mb-2 text-[24px] leading-[1.3] text-[var(--color-foreground)]"
        style={{ fontFamily: 'var(--font-family-dt-serif, "Lora", serif)' }}
      >
        {summary.title}
      </h1>

      {/* Metadata row — component-spec.md §3.3 */}
      <div className="mb-6 flex items-center gap-2 text-[11px] text-muted-foreground">
        <span className={`rounded px-2 py-0.5 text-[10px] font-medium ${categoryStyle}`}>
          {category}
        </span>
        <span>&middot;</span>
        <span>{summary.created_at?.slice(0, 10) ?? "—"}</span>
        <span>&middot;</span>
        <span>{readingTime} read</span>
      </div>

      {/* Summary */}
      {summary.summary && (
        <p className="mb-6 text-[14px] italic text-[var(--color-muted-foreground)]">
          {summary.summary}
        </p>
      )}

      {/* Markdown body — component-spec.md §3.4 */}
      <div className="wiki-article-body markdown-content">
        <ReactMarkdown components={components}>{expandedBody}</ReactMarkdown>
      </div>

      {/* Backlinks — component-spec.md §3.7 */}
      <BacklinksSection slug={slug} />
    </div>
  );
}

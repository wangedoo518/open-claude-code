/**
 * Shared wiki-link rendering utilities for ReactMarkdown.
 *
 * Context for the 2026-04 fix: wiki page bodies and /query answers
 * both render user-authored markdown. Links inside that markdown come
 * in three flavours, none of which the default `<a>` renderer handles
 * correctly:
 *
 *   1. `wiki://<slug>`            — our canonical internal form
 *   2. `concepts/<slug>.md`       — relative path emitted by the LLM
 *      `people/<slug>.md`            when it cites a wiki page
 *      `topics/<slug>.md`
 *      `topic/<slug>.md`
 *      `compare/<slug>.md`
 *   3. `/wiki?page=<slug>`        — absolute app URL (rare, but exists)
 *      `#/wiki?page=<slug>`
 *
 * Without interception, those hrefs are passed to the browser verbatim
 * and — under HashRouter — resolve to nonexistent routes that React
 * Router's `*` catch-all redirects to `/dashboard`. The user sees
 * their wiki-internal link pop them back to the home page.
 *
 * This module provides:
 *   - `parseWikiHref(href)`            — classify a raw href
 *   - `preprocessWikilinks(md)`        — [[slug|Label]] → [Label](wiki://slug)
 *   - `useWikiLinkRenderer()`          — the `<a>` React component
 *                                        wired to the tab store, mode
 *                                        store, and dead-link check.
 */

import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

import { useWikiTabStore } from "@/state/wiki-tab-store";
import { useSettingsStore } from "@/state/settings-store";
import { fetchJson } from "@/lib/desktop/transport";

/* ─── URL → slug classification ─────────────────────────────────── */

/** A detected wiki-internal reference extracted from an anchor href. */
export interface WikiRef {
  slug: string;
}

/** Category folders that appear as path prefixes in relative .md links. */
const CATEGORY_PATH_PREFIX =
  /^(?:\.\.?\/)?(?:concepts?|people|topics?|compares?)\/([^/]+?)\.md$/i;

/** Matches `/wiki?page=<slug>` or `#/wiki?page=<slug>`. */
const WIKI_PAGE_QUERY = /(?:^|#\/?|\/)wiki\?page=([^&]+)/i;

/**
 * Classify an anchor href. Returns the slug if the href looks like a
 * wiki-internal link, otherwise `null` (meaning: pass through to the
 * default anchor handling, i.e. external link or app navigation).
 *
 * Slug validation is intentionally lax here — we accept anything that
 * looks path-shaped and let the downstream lookup fail loudly (toast)
 * rather than silently dropping weird slugs.
 */
export function parseWikiHref(href: string): WikiRef | null {
  const trimmed = href.trim();
  if (!trimmed) return null;

  // 1. wiki://slug — our canonical form.
  if (trimmed.startsWith("wiki://")) {
    const raw = trimmed.slice("wiki://".length);
    try {
      const slug = decodeURIComponent(raw).trim();
      if (slug) return { slug };
    } catch {
      /* malformed encoding → fall through */
    }
  }

  // 2. relative .md path with a category folder.
  const catMatch = trimmed.match(CATEGORY_PATH_PREFIX);
  if (catMatch) {
    try {
      return { slug: decodeURIComponent(catMatch[1]) };
    } catch {
      return { slug: catMatch[1] };
    }
  }

  // 3. /wiki?page=slug (absolute, with or without # prefix).
  const queryMatch = trimmed.match(WIKI_PAGE_QUERY);
  if (queryMatch) {
    try {
      return { slug: decodeURIComponent(queryMatch[1]) };
    } catch {
      return { slug: queryMatch[1] };
    }
  }

  return null;
}

/* ─── `[[slug|label]]` preprocessor ─────────────────────────────── */

const WIKILINK_RE = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;

/**
 * Rewrite Obsidian-style wikilinks `[[slug]]` / `[[slug|Label]]` into
 * standard markdown `[Label](wiki://slug)` so ReactMarkdown's default
 * link pipeline surfaces them to our custom `<a>` component.
 *
 * Idempotent: running twice is a no-op because the `[[...]]` pattern
 * no longer matches after the first pass.
 */
export function preprocessWikilinks(body: string): string {
  return body.replace(WIKILINK_RE, (_m, slug: string, label?: string) => {
    const display = (label ?? slug).trim();
    const slugTrimmed = slug.trim();
    return `[${display}](wiki://${slugTrimmed})`;
  });
}

/* ─── Link React component ──────────────────────────────────────── */

type AnchorProps = React.AnchorHTMLAttributes<HTMLAnchorElement> & {
  children?: React.ReactNode;
};

/**
 * Returns the `<a>` React component to plug into
 * `ReactMarkdown.components.a`. Handles three cases:
 *
 *   - **Internal wiki link** (matches `parseWikiHref`): intercepts the
 *     click, verifies the page exists, switches the shell to Wiki
 *     mode, and opens a Tab. Dead slugs → toast error.
 *   - **External http/https link**: open in new tab with safe rel.
 *   - **Everything else** (mailto, tel, anchor-only `#foo`): default
 *     anchor behaviour.
 */
export function useWikiLinkRenderer(): React.FC<AnchorProps> {
  const openTab = useWikiTabStore((s) => s.openTab);
  const setAppMode = useSettingsStore((s) => s.setAppMode);
  // DS1.2: internal wiki links now primarily navigate via react-router
  // so they land inside KnowledgeHubPage's article route. `openTab` is
  // still called for back-compat with any surface that still mounts
  // WikiTab directly; if WikiTab isn't in the tree the call is a no-op
  // on the visible UI.
  const navigate = useNavigate();

  const openInternal = useCallback(
    async (slug: string, title: string) => {
      // Dead-link guard. Keep it best-effort: a network glitch
      // shouldn't block a navigation the user clearly asked for, so we
      // only block on an explicit 404-shaped response, not on thrown
      // errors.
      try {
        await fetchJson<unknown>(`/api/wiki/pages/${encodeURIComponent(slug)}`);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        if (msg.includes("404") || msg.toLowerCase().includes("not found")) {
          toast.error(`页面不存在：${slug}`);
          return;
        }
        // Network / unknown error — let the navigation proceed; the
        // WikiArticle component surfaces its own load error.
      }
      setAppMode("wiki");
      openTab({
        id: slug,
        kind: "article",
        slug,
        title,
        closable: true,
      });
      // Primary navigation path for DS1.2+ shell.
      navigate(`/wiki/${encodeURIComponent(slug)}`);
    },
    [openTab, setAppMode, navigate],
  );

  return useCallback(
    (props: AnchorProps) => {
      const { href, children, ...rest } = props;
      const ref = href ? parseWikiHref(href) : null;

      if (ref) {
        return (
          <a
            href="#"
            onClick={(e) => {
              e.preventDefault();
              const title =
                typeof children === "string" ? children : ref.slug;
              void openInternal(ref.slug, title);
            }}
            className="cursor-pointer text-[var(--color-primary)] underline decoration-dotted underline-offset-[3px] hover:decoration-solid"
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
            className="text-[var(--color-primary)] underline decoration-solid underline-offset-2 hover:decoration-[3px]"
            {...rest}
          >
            {children}
          </a>
        );
      }

      // mailto, tel, plain anchors, etc. — default behaviour with our
      // colour palette applied.
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
    [openInternal],
  );
}

/**
 * WikiQueryMessage — renders a /query result as a special message block.
 * Displayed in the message list when a ?-prefixed question triggers wiki Q&A.
 *
 * Visual: 📚 icon (instead of Bot avatar) + green left border + streaming answer + sources card.
 *
 * The answer body is piped through `preprocessWikilinks` to expand
 * `[[slug|Label]]` syntax, then rendered with a custom `<a>` component
 * (`useWikiLinkRenderer`) that intercepts relative `.md` / `wiki://` /
 * `/wiki?page=` hrefs and routes them to the Wiki tab store instead of
 * letting the browser navigate them (which used to fall through
 * HashRouter to `/dashboard`).
 */

import { useMemo } from "react";
import { BookOpen, Loader2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import { QuerySourcesCard } from "./QuerySourcesCard";
import type { QuerySource } from "@/features/ingest/types";
import {
  preprocessWikilinks,
  useWikiLinkRenderer,
} from "@/features/wiki/wiki-link-utils";

interface WikiQueryMessageProps {
  question: string;
  answer: string;
  sources: QuerySource[];
  isStreaming: boolean;
  error: string | null;
}

export function WikiQueryMessage({
  question,
  answer,
  sources,
  isStreaming,
  error,
}: WikiQueryMessageProps) {
  const Anchor = useWikiLinkRenderer();
  const markdownComponents = useMemo<Components>(
    () => ({ a: Anchor }),
    [Anchor],
  );
  const processedAnswer = useMemo(
    () => preprocessWikilinks(answer),
    [answer],
  );

  return (
    <div className="flex gap-3 py-3">
      {/* Icon — 📚 instead of Bot avatar */}
      <div className="flex size-7 shrink-0 items-center justify-center rounded-full bg-[var(--deeptutor-ok,#3F8F5E)]/15 text-[var(--deeptutor-ok,#3F8F5E)]">
        <BookOpen className="size-4" />
      </div>

      <div className="min-w-0 flex-1">
        {/* Question echo */}
        <div className="mb-1 text-[11px] font-medium text-[var(--deeptutor-ok,#3F8F5E)]">
          知识库问答
        </div>
        <div className="mb-2 text-[13px] italic text-[var(--color-muted-foreground)]">
          ? {question}
        </div>

        {/* Answer body — green left border */}
        <div className="border-l-2 border-[var(--deeptutor-ok,#3F8F5E)]/40 pl-3">
          {answer ? (
            <div className="prose-sm text-[14px] leading-[1.6] text-[var(--color-foreground)]">
              <ReactMarkdown components={markdownComponents}>
                {processedAnswer}
              </ReactMarkdown>
            </div>
          ) : isStreaming ? (
            <div className="flex items-center gap-2 text-[13px] text-[var(--color-muted-foreground)]">
              <Loader2 className="size-3.5 animate-spin" />
              正在查询知识库...
            </div>
          ) : null}

          {error && (
            <div className="mt-1 text-[12px] text-[var(--color-destructive)]">
              查询失败: {error}
            </div>
          )}

          {/* Streaming indicator */}
          {isStreaming && answer && (
            <span className="inline-block size-2 animate-pulse rounded-full bg-[var(--deeptutor-ok,#3F8F5E)]" />
          )}
        </div>

        {/* Sources card — shown after streaming completes */}
        {!isStreaming && sources.length > 0 && (
          <QuerySourcesCard sources={sources} />
        )}
      </div>
    </div>
  );
}

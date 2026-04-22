/**
 * StreamingMessage — Claude-style streaming assistant turn (A5).
 *
 * Before A5 this component rendered a stripped-down ReactMarkdown
 * (only `p` / `code` / `pre`) which made the visual bounce between
 * streaming → final very jarring: lists became paragraphs, headings
 * lost hierarchy, fenced code lost its frame. A5 reuses the shared
 * `AskMarkdown` renderer so the element tree matches the final
 * `AssistantMessage`; the only cross-state difference is that fenced
 * code blocks render in their "still-writing" variant during
 * streaming (no Prism → no flicker on half-tokens) and smoothly
 * promote to highlighted when the turn completes.
 *
 * Phased loading states (when `content === ""` and no thinking
 * payload has arrived yet) stay as a local UI affordance:
 *   0-5s:   思考中…
 *   5-15s:  深度思考中…
 *   15s+:   准备回复…
 *
 * Thinking plumbing: the backend's current SSE vocabulary (snapshot
 * / message / text_delta / permission_request) does NOT carry a
 * reasoning channel, so the `thinkingContent` prop here is a forward
 * compatibility hook. When Worker A introduces a `thinking_delta`
 * event (post-A5) the store can feed the accumulated summary through
 * this prop and the `ThinkingBlock` below will render a collapsible
 * summary. Until then, we never synthesize fake reasoning text —
 * the phase shimmer is the only "thinking" signal.
 */

import { memo, useState, useEffect, useRef } from "react";
import { Brain, ChevronRight, ChevronDown, Bot } from "lucide-react";
import { AskMarkdown } from "./AskMarkdown";
import { Shimmer } from "./Shimmer";

interface StreamingMessageProps {
  content: string;
  thinkingContent?: string;
  isComplete?: boolean;
}

export const StreamingMessage = memo(function StreamingMessage({
  content,
  thinkingContent,
  isComplete = false,
}: StreamingMessageProps) {
  const [elapsed, setElapsed] = useState(0);
  const startRef = useRef(Date.now());

  useEffect(() => {
    if (isComplete) return;
    startRef.current = Date.now();
    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startRef.current) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [isComplete]);

  // No content yet → phased shimmer, Claude-style "subtle waiting".
  if (!content && !thinkingContent) {
    return (
      <div className="flex items-start gap-2.5 pb-3">
        <AssistantAvatar />
        <div className="flex items-center gap-3 py-2">
          <Shimmer className="text-sm font-medium">
            {elapsed < 5
              ? "思考中…"
              : elapsed < 15
                ? "深度思考中…"
                : "准备回复…"}
          </Shimmer>
          {elapsed >= 2 && (
            <span className="tabular-nums text-[11px] text-muted-foreground/40">
              {elapsed}s
            </span>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-start gap-2.5 pb-3">
      <AssistantAvatar />

      <div className="min-w-0 flex-1">
        {/* Thinking summary — only rendered when the backend has
            provided a reasoning payload. No fake summaries. */}
        {thinkingContent && (
          <ThinkingBlock content={thinkingContent} isStreaming={!isComplete} />
        )}

        {/* Streaming body reuses AskMarkdown so the element tree
            matches the final AssistantMessage; code blocks render
            in their streaming variant until the turn completes. */}
        {content && (
          <div
            className="ask-stream-fade overflow-hidden text-[15px] leading-[1.8] text-foreground"
            style={{ overflowWrap: "break-word", wordBreak: "break-word" }}
          >
            <AskMarkdown content={content} streaming={!isComplete} />
            {!isComplete && (
              <span
                className="ask-blink-cursor ml-0.5 inline-block h-[1.1em] w-[2px] translate-y-[2px] bg-foreground/70"
                aria-hidden
              />
            )}
          </div>
        )}

        {/* Quiet elapsed timer, hidden for very short turns. */}
        {!isComplete && elapsed >= 2 && (
          <div className="mt-1 text-right">
            <span className="tabular-nums text-[11px] text-muted-foreground/40">
              {elapsed >= 60
                ? `${Math.floor(elapsed / 60)}m ${elapsed % 60}s`
                : `${elapsed}s`}
            </span>
          </div>
        )}
      </div>
    </div>
  );
});

function AssistantAvatar() {
  return (
    <div
      className="flex size-7 shrink-0 items-center justify-center rounded-full"
      style={{
        backgroundColor: "var(--deeptutor-primary, var(--claude-orange))",
      }}
    >
      <Bot className="size-3.5 text-white" />
    </div>
  );
}

/* ─── Thinking block (collapsible) ──────────────────────────────── */

function ThinkingBlock({
  content,
  isStreaming,
}: {
  content: string;
  isStreaming: boolean;
}) {
  const [expanded, setExpanded] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const startRef = useRef(Date.now());

  useEffect(() => {
    if (!isStreaming) return;
    startRef.current = Date.now();
    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startRef.current) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [isStreaming]);

  useEffect(() => {
    if (!isStreaming && elapsed === 0) {
      setElapsed(Math.max(1, Math.ceil(content.length / 50)));
    }
  }, [isStreaming, elapsed, content.length]);

  // Extract a short summary: first **bold** or leading heading. This
  // is intentionally conservative — we never render the raw reasoning
  // unless the user explicitly expands the block.
  const summary = (() => {
    const boldMatch = content.match(/\*\*(.+?)\*\*/);
    if (boldMatch) return boldMatch[1];
    const headingMatch = content.match(/^#{1,4}\s+(.+)$/m);
    if (headingMatch) return headingMatch[1];
    return isStreaming ? "思考中" : "思考过程";
  })();

  return (
    <div className="mb-2">
      <button
        type="button"
        className="flex w-full items-center gap-1.5 rounded px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted/30"
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? (
          <ChevronDown className="size-3" />
        ) : (
          <ChevronRight className="size-3" />
        )}
        <Brain
          className="size-3"
          style={{ color: "var(--deeptutor-purple, var(--agent-purple))" }}
        />
        {isStreaming ? (
          <Shimmer className="font-medium">
            {summary} · {elapsed}s
          </Shimmer>
        ) : (
          <span className="font-medium text-muted-foreground/60">
            {summary} · {elapsed}s
          </span>
        )}
      </button>
      {expanded && (
        <div className="ml-5 mt-1 border-l-2 border-border/30 pl-3">
          <pre className="whitespace-pre-wrap text-xs italic leading-relaxed text-muted-foreground">
            {content}
          </pre>
        </div>
      )}
    </div>
  );
}

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
import { Brain, ChevronRight, ChevronDown } from "lucide-react";
import { AskMarkdown } from "./AskMarkdown";
import { Shimmer } from "./Shimmer";
import type { ConversationTurnStatus } from "./useConversationTurnState";

interface StreamingMessageProps {
  content: string;
  thinkingContent?: string;
  isComplete?: boolean;
  turnStatus?: ConversationTurnStatus;
}

function getRevealStep(remaining: number): number {
  // Large SSE chunks should feel like text is being laid down, not pasted.
  // Keep the step adaptive so tiny chunks stay responsive while 10k+ bursts
  // catch up in under a second without a single-frame flash.
  const adaptiveStep = Math.ceil(remaining * 0.055);
  return Math.min(220, Math.max(28, adaptiveStep));
}

export const StreamingMessage = memo(function StreamingMessage({
  content,
  thinkingContent,
  isComplete = false,
  turnStatus,
}: StreamingMessageProps) {
  const [elapsed, setElapsed] = useState(0);
  const [displayContent, setDisplayContent] = useState(() =>
    isComplete ? content : "",
  );
  const startRef = useRef(Date.now());
  const targetContentRef = useRef(content);
  const displayContentRef = useRef(isComplete ? content : "");
  const revealRafRef = useRef<number | null>(null);

  useEffect(() => {
    if (isComplete) return;
    startRef.current = Date.now();
    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startRef.current) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [isComplete]);

  useEffect(() => {
    targetContentRef.current = content;

    const cancelReveal = () => {
      if (revealRafRef.current !== null) {
        cancelAnimationFrame(revealRafRef.current);
        revealRafRef.current = null;
      }
    };

    if (isComplete) {
      cancelReveal();
      displayContentRef.current = content;
      setDisplayContent(content);
      return cancelReveal;
    }

    const current = displayContentRef.current;
    if (!content.startsWith(current) || content.length < current.length) {
      displayContentRef.current = content;
      setDisplayContent(content);
      return cancelReveal;
    }

    const revealNextFrame = () => {
      if (revealRafRef.current !== null) return;
      revealRafRef.current = requestAnimationFrame(() => {
        revealRafRef.current = null;
        const target = targetContentRef.current;
        const prev = displayContentRef.current;

        if (!target.startsWith(prev) || target.length <= prev.length) {
          displayContentRef.current = target;
          setDisplayContent(target);
          return;
        }

        const remaining = target.length - prev.length;
        const step = Math.min(remaining, getRevealStep(remaining));
        const next = target.slice(0, prev.length + step);
        displayContentRef.current = next;
        setDisplayContent(next);

        if (next.length < target.length) {
          revealNextFrame();
        }
      });
    };

    revealNextFrame();
    return cancelReveal;
  }, [content, isComplete]);

  const statusText =
    turnStatus?.label ??
    (content
      ? "正在写入回答"
      : thinkingContent
        ? "正在整理思路"
        : elapsed < 5
          ? "正在分析上下文"
          : elapsed < 15
            ? "正在调用工具与检索"
            : "正在组织回答");

  // No content yet → phased work-log shimmer, not a chat bubble.
  if (!content && !thinkingContent) {
    return (
      <div className="ask-turn ask-turn-assistant">
        <div className="ask-transcript-shell ask-transcript-shell--streaming">
          <div className="ask-transcript-rail ask-transcript-rail--active" aria-hidden />
          <div className="ask-stream-status">
            <span className="ask-stream-dot" aria-hidden />
            <Shimmer className="text-sm font-medium">{statusText}…</Shimmer>
            {elapsed >= 2 && (
              <span className="tabular-nums text-[11px] text-muted-foreground/45">
                {elapsed}s
              </span>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="ask-turn ask-turn-assistant">
      <div className="ask-transcript-shell ask-transcript-shell--streaming">
        <div className="ask-transcript-rail ask-transcript-rail--active" aria-hidden />
        <div className="min-w-0 flex-1">
          <div className="ask-stream-status">
            <span className="ask-stream-dot" aria-hidden />
            <Shimmer className="text-sm font-medium">{statusText}</Shimmer>
            {turnStatus?.detail && (
              <span className="hidden text-[11px] text-muted-foreground/45 md:inline">
                {turnStatus.detail}
              </span>
            )}
            {!isComplete && elapsed >= 2 && (
              <span className="tabular-nums text-[11px] text-muted-foreground/45">
                {elapsed >= 60
                  ? `${Math.floor(elapsed / 60)}m ${elapsed % 60}s`
                  : `${elapsed}s`}
              </span>
            )}
          </div>

          {/* Thinking summary — only rendered when the backend has
              provided a reasoning payload. No fake summaries. */}
          {thinkingContent && (
            <ThinkingBlock content={thinkingContent} isStreaming={!isComplete} />
          )}

          {/* Streaming body reuses AskMarkdown so the element tree
              matches the final AssistantMessage; code blocks render
              in their streaming variant until the turn completes. */}
          {displayContent && (
            <div
              className={`ask-assistant-prose ask-stream-body ask-stream-fade overflow-hidden text-foreground ${
                !isComplete && displayContent.length < content.length
                  ? "ask-stream-body--catching-up"
                  : ""
              }`}
              style={{ overflowWrap: "break-word", wordBreak: "break-word" }}
            >
              <AskMarkdown content={displayContent} streaming={!isComplete} />
              {!isComplete && (
                <span
                  className="ask-blink-cursor ml-0.5 inline-block h-[1.1em] w-[2px] translate-y-[2px] bg-foreground/70"
                  aria-hidden
                />
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
});

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
    <div className="ask-thinking-block">
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
        <div className="ml-5 mt-1 border-l border-border/40 pl-3">
          <pre className="whitespace-pre-wrap text-xs italic leading-relaxed text-muted-foreground">
            {content}
          </pre>
        </div>
      )}
    </div>
  );
}

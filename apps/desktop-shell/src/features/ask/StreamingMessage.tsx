/**
 * StreamingMessage — Claude Code 风格的流式消息组件。
 *
 * 分阶段状态：
 *   0-5s:  "思考中..." (shimmer)
 *   5-15s: "深度思考中..." (shimmer)
 *   15s+:  "准备回复..." (shimmer)
 *   有内容: 显示 markdown + 闪烁光标
 *   思考内容: 折叠块 + 耗时
 */

import { memo, useState, useEffect, useRef } from "react";
import ReactMarkdown from "react-markdown";
import { Brain, ChevronRight, ChevronDown, Bot } from "lucide-react";
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

  // Global elapsed timer
  useEffect(() => {
    if (isComplete) return;
    startRef.current = Date.now();
    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startRef.current) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [isComplete]);

  // No content yet — show phased thinking indicator
  if (!content && !thinkingContent) {
    return (
      <div className="flex items-start gap-2.5 pb-3">
        {/* Avatar */}
        <div
          className="flex size-7 shrink-0 items-center justify-center rounded-full"
          style={{ backgroundColor: "var(--deeptutor-primary, var(--claude-orange))" }}
        >
          <Bot className="size-3.5 text-white" />
        </div>
        <div className="flex items-center gap-3 py-2">
          <Shimmer className="text-sm font-medium">
            {elapsed < 5 ? "思考中..." : elapsed < 15 ? "深度思考中..." : "准备回复..."}
          </Shimmer>
          <span className="tabular-nums text-[11px] text-muted-foreground/40">
            {elapsed}s
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-start gap-2.5 pb-3">
      {/* Avatar */}
      <div
        className="flex size-7 shrink-0 items-center justify-center rounded-full"
        style={{ backgroundColor: "var(--deeptutor-primary, var(--claude-orange))" }}
      >
        <Bot className="size-3.5 text-white" />
      </div>

      <div className="min-w-0 flex-1">
        {/* Thinking content (collapsible) */}
        {thinkingContent && (
          <ThinkingBlock content={thinkingContent} isStreaming={!isComplete} />
        )}

        {/* Streaming text */}
        {content && (
          <div className="text-[15px] leading-[1.8] text-foreground">
            <ReactMarkdown
              components={{
                p({ children }) {
                  return <p className="mb-2 last:mb-0">{children}</p>;
                },
                code({ className, children, ...props }) {
                  return (
                    <code
                      className={className ?? "rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[13px] text-foreground"}
                      {...props}
                    >
                      {children}
                    </code>
                  );
                },
                pre({ children }) {
                  return <>{children}</>;
                },
              }}
            >
              {content}
            </ReactMarkdown>
            {/* Blinking block cursor */}
            {!isComplete && (
              <span className="ask-blink-cursor ml-0.5 inline-block h-[1.1em] w-[2px] translate-y-[2px] bg-foreground/70" />
            )}
          </div>
        )}

        {/* Elapsed timer at bottom right */}
        {!isComplete && (
          <div className="mt-1 text-right">
            <span className="tabular-nums text-[11px] text-muted-foreground/40">
              {elapsed >= 60 ? `${Math.floor(elapsed / 60)}m ${elapsed % 60}s` : `${elapsed}s`}
            </span>
          </div>
        )}
      </div>
    </div>
  );
});

/* ─── Thinking block (collapsible) ──────────────────────────────── */

function ThinkingBlock({ content, isStreaming }: { content: string; isStreaming: boolean }) {
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

  // Extract summary from first **bold** or # heading
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
        {expanded ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        <Brain className="size-3" style={{ color: "var(--deeptutor-purple, var(--agent-purple))" }} />
        {isStreaming ? (
          <Shimmer className="font-medium">{summary} {elapsed}s</Shimmer>
        ) : (
          <span className="font-medium text-muted-foreground/60">{summary} · {elapsed}s</span>
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

/**
 * S0.3 extraction target: single chat message bubble (CCD soul ②).
 *
 * Original: features/session-workbench/MessageItem.tsx. 100% verbatim
 * copy with only the types import path swapped to
 * `@/features/common/message-types`. All subcomponents (MarkdownContent,
 * CodeBlock, UserMessage, AssistantMessage, ToolUseMessage, etc.) are
 * preserved so the Ask page can mount this component unchanged once S3
 * wires it to the ask_runtime stream.
 */

import { memo, useState, useMemo, useRef, useEffect } from "react";

/** Timestamp when the MessageList module first loaded. Messages rendered
 *  within 3s of this are "old" (page load / conversation switch) and
 *  skip the sentence reveal animation. Messages arriving later are "new". */
let moduleLoadTime = Date.now();
/** Reset on HMR to avoid stale timestamps */
if (import.meta.hot) {
  import.meta.hot.accept(() => { moduleLoadTime = Date.now(); });
}
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import {
  ChevronDown,
  ChevronRight,
  AlertCircle,
  CheckCircle2,
  Copy,
  Check,
  Brain,
  User,
  Bot,
  File,
  Folder,
  Globe,
  ExternalLink,
} from "lucide-react";
import { getToolMeta } from "./tool-meta";
import { cn } from "@/lib/utils";
import type { ConversationMessage } from "@/features/common/message-types";

interface MessageProps {
  message: ConversationMessage;
}

export const Message = memo(function Message({ message }: MessageProps) {
  switch (message.type) {
    case "text":
      return message.role === "user" ? (
        <UserMessage content={message.content} />
      ) : message.role === "system" ? (
        <SystemMessage content={message.content} />
      ) : (
        <AssistantMessage content={message.content} usage={message.usage} />
      );
    case "tool_use":
      return <ToolUseMessage message={message} />;
    case "tool_result":
      if (isTodoToolResult(message)) {
        return <TodoMessage message={message} />;
      }
      return <ToolResultMessage message={message} />;
    case "error":
      return <ErrorMessage content={message.content} />;
    default:
      return <AssistantMessage content={message.content} />;
  }
});

/* ─── Markdown renderer ──────────────────────────────────────────── */

function MarkdownContent({ content }: { content: string }) {
  return (
    <ReactMarkdown
      components={{
        code({ className, children, ...props }) {
          const match = /language-(\w+)/.exec(className || "");
          const codeString = String(children).replace(/\n$/, "");

          if (match) {
            return (
              <CodeBlock language={match[1]} code={codeString} />
            );
          }

          return (
            <code
              className="rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[13px] text-foreground"
              {...props}
            >
              {children}
            </code>
          );
        },
        pre({ children }) {
          return <>{children}</>;
        },
        p({ children }) {
          return <p className="mb-2 last:mb-0">{children}</p>;
        },
        ul({ children }) {
          return <ul className="mb-2 list-disc pl-5 last:mb-0">{children}</ul>;
        },
        ol({ children }) {
          return <ol className="mb-2 list-decimal pl-5 last:mb-0">{children}</ol>;
        },
        li({ children }) {
          return <li className="mb-0.5">{children}</li>;
        },
        h1({ children }) {
          return <h1 className="mb-2 mt-3 text-base font-bold first:mt-0">{children}</h1>;
        },
        h2({ children }) {
          return <h2 className="mb-2 mt-3 text-head font-bold first:mt-0">{children}</h2>;
        },
        h3({ children }) {
          return <h3 className="mb-1.5 mt-2.5 text-sm font-semibold first:mt-0">{children}</h3>;
        },
        blockquote({ children }) {
          return (
            <blockquote className="mb-2 border-l-[3px] border-muted-foreground/30 pl-3 italic text-muted-foreground last:mb-0">
              {children}
            </blockquote>
          );
        },
        table({ children }) {
          return (
            <div className="mb-2 overflow-x-auto last:mb-0">
              <table className="w-full border-collapse text-body-sm">{children}</table>
            </div>
          );
        },
        th({ children }) {
          return (
            <th className="border border-border/50 bg-muted/50 px-2.5 py-1.5 text-left font-semibold">
              {children}
            </th>
          );
        },
        td({ children }) {
          return (
            <td className="border border-border/50 px-2.5 py-1.5">{children}</td>
          );
        },
        hr() {
          return <hr className="my-3 border-border/50" />;
        },
        a({ href, children }) {
          return (
            <a
              href={href}
              className="text-[color:var(--color-label-you,rgb(37,99,235))] underline decoration-[color:var(--color-label-you,rgb(37,99,235))]/30 hover:decoration-[color:var(--color-label-you,rgb(37,99,235))]"
              target="_blank"
              rel="noopener noreferrer"
            >
              {children}
            </a>
          );
        },
      }}
    >
      {content}
    </ReactMarkdown>
  );
}

/* ─── Code block with copy button ────────────────────────────────── */

function CodeBlock({ language, code }: { language: string; code: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    void navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="group/code my-3 overflow-hidden rounded-lg border border-border/50 bg-[#1f2937]">
      {/* Language header bar */}
      <div className="flex items-center justify-between border-b border-white/10 px-3 py-2">
        <span className="text-[11px] font-medium uppercase tracking-wider text-[#9ca3af]">
          {language}
        </span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-[#9ca3af] opacity-0 transition-opacity hover:text-white group-hover/code:opacity-100"
        >
          {copied ? (
            <>
              <Check className="size-3" /> 已复制
            </>
          ) : (
            <>
              <Copy className="size-3" /> 复制
            </>
          )}
        </button>
      </div>
      <SyntaxHighlighter
        language={language}
        style={oneDark}
        customStyle={{
          margin: 0,
          padding: "1rem",
          fontSize: "13px",
          lineHeight: "1.7",
          background: "#1f2937",
          borderRadius: 0,
        }}
        codeTagProps={{
          style: { fontFamily: "var(--font-family-dt-mono, 'JetBrains Mono', 'Cascadia Code', 'Fira Code', monospace)" },
        }}
      >
        {code}
      </SyntaxHighlighter>
    </div>
  );
}

/* ─── User message ───────────────────────────────────────────────── */

function UserMessage({ content }: { content: string }) {
  const [copied, setCopied] = useState(false);

  return (
    <div className="group/user flex items-start justify-end gap-2">
      <div className="flex max-w-[75%] flex-col items-end gap-1">
        <div className="whitespace-pre-wrap rounded-2xl bg-foreground px-4 py-2.5 text-[14px] leading-relaxed text-background">
          {content}
        </div>
        <div className="flex items-center gap-1 opacity-0 transition-opacity group-hover/user:opacity-100">
          <button
            type="button"
            onClick={() => {
              void navigator.clipboard.writeText(content);
              setCopied(true);
              setTimeout(() => setCopied(false), 2000);
            }}
            className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground/50 transition-colors hover:text-foreground"
          >
            {copied ? "已复制" : "复制"}
          </button>
        </div>
      </div>
      {/* User avatar */}
      <div className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted">
        <User className="size-3.5 text-muted-foreground" />
      </div>
    </div>
  );
}

/* ─── Assistant message with markdown ────────────────────────────── */

function AssistantMessage({ content, usage }: { content: string; usage?: { inputTokens: number; outputTokens: number } }) {
  const [copied, setCopied] = useState(false);

  // Sentence-by-sentence reveal for NEW messages only.
  // "New" = rendered more than 3s after module load (not a page refresh).
  const mountTimeRef = useRef(Date.now());
  const isNew = mountTimeRef.current - moduleLoadTime > 3000;
  const sentences = useMemo(() => {
    if (!isNew) return null;
    // Split by Chinese/English sentence endings, keeping delimiters
    return content.split(/(?<=[。！？.!?\n])/);
  }, [content, isNew]);

  const [visibleCount, setVisibleCount] = useState(isNew ? 1 : (sentences?.length ?? 0));

  useEffect(() => {
    if (!sentences || visibleCount >= sentences.length) return;
    const timer = setTimeout(() => {
      setVisibleCount((c) => Math.min(c + 1, sentences.length));
    }, 80); // ~80ms per sentence
    return () => clearTimeout(timer);
  }, [visibleCount, sentences]);

  const displayContent = sentences
    ? sentences.slice(0, visibleCount).join("")
    : content;
  const isRevealing = sentences ? visibleCount < sentences.length : false;

  const handleCopy = () => {
    void navigator.clipboard.writeText(content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const fmt = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);

  return (
    <div className="flex items-start gap-2.5 pb-3 animate-fade-in">
      {/* Claw avatar */}
      <div
        className="flex size-7 shrink-0 items-center justify-center rounded-full"
        style={{ backgroundColor: "var(--deeptutor-primary, var(--claude-orange))" }}
      >
        <Bot className="size-3.5 text-white" />
      </div>

      <div className="min-w-0 flex-1">
        {/* Input tokens label — next to avatar, like "Assistant · 输入 7.3k" */}
        {usage && (
          <div className="mb-1 text-[11px] text-muted-foreground/40">
            Assistant · 输入 {fmt(usage.inputTokens)}
          </div>
        )}

        {/* Message body — 15px, generous line height */}
        <div className="text-[15px] leading-[1.8] text-foreground">
          <MarkdownContent content={displayContent} />
          {isRevealing && (
            <span className="ask-blink-cursor ml-0.5 inline-block h-[1.1em] w-[2px] translate-y-[2px] bg-foreground/70" />
          )}
        </div>

        {/* Action row: Copy + output tokens */}
        <div className="mt-2 flex items-center gap-3">
          <button
            type="button"
            onClick={handleCopy}
            className="flex items-center gap-1 text-[12px] text-muted-foreground/50 transition-colors hover:text-foreground"
          >
            {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
            {copied ? "已复制" : "复制"}
          </button>

          {usage && usage.outputTokens > 0 && (
            <div className="ml-auto text-[11px] text-muted-foreground/40">
              输出 {fmt(usage.outputTokens)}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/* ─── System message ─────────────────────────────────────────────── */

function SystemMessage({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const timeStr = new Date().toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });

  return (
    <div className="flex justify-center">
      <button
        className="flex items-center gap-2 rounded-full border border-border/20 px-3 py-1 text-[11px] text-muted-foreground/60 transition-colors hover:bg-muted/20"
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? <ChevronDown className="size-2.5" /> : <ChevronRight className="size-2.5" />}
        <span>── 系统 · {timeStr} ──</span>
        {!expanded && (
          <span className="flex-1 truncate text-left opacity-60">
            {content.slice(0, 80)}
          </span>
        )}
      </button>
      {expanded && (
        <div className="mx-auto mt-1 max-w-lg rounded-lg border border-border/20 bg-muted/10 p-3">
          <pre className="whitespace-pre-wrap text-center font-mono text-[11px] text-muted-foreground/60">
            {content}
          </pre>
        </div>
      )}
    </div>
  );
}

/* ─── Tool status badge ─────────────────────────────────────────── */

function ToolStatusBadge({ status }: { status: "pending" | "running" | "completed" | "error" }) {
  const styles = {
    pending: "border-[color:var(--deeptutor-warn,rgb(200,139,26))]/30 bg-[color:var(--deeptutor-warn,rgb(200,139,26))]/10 text-[color:var(--deeptutor-warn,rgb(200,139,26))]",
    running: "border-[color:var(--deeptutor-primary,rgb(195,90,44))]/30 bg-[color:var(--deeptutor-primary,rgb(195,90,44))]/10 text-[color:var(--deeptutor-primary,rgb(195,90,44))]",
    completed: "border-[color:var(--color-success)]/30 bg-[color:var(--color-success)]/10 text-[color:var(--color-success)]",
    error: "border-[color:var(--color-error)]/30 bg-[color:var(--color-error)]/10 text-[color:var(--color-error)]",
  };
  const labels = { pending: "等待中", running: "执行中", completed: "完成", error: "出错" };

  return (
    <span className={cn("inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 text-[10px] font-medium", styles[status])}>
      {status === "running" && (
        <span className="inline-block size-2.5 animate-spin rounded-full border border-current border-t-transparent" />
      )}
      {labels[status]}
    </span>
  );
}

/* ─── Tool use message ───────────────────────────────────────────── */

function ToolUseMessage({ message }: { message: ConversationMessage }) {
  const [expanded, setExpanded] = useState(false);
  const toolName = message.toolUse?.toolName ?? "Tool";
  const toolInput = message.toolUse?.toolInput ?? "";

  const { icon: ToolIcon, label, color } = getToolMeta(toolName);

  // Try to parse tool input for structured display
  const parsedInput = useMemo(() => {
    try {
      return JSON.parse(toolInput) as Record<string, unknown>;
    } catch {
      return null;
    }
  }, [toolInput]);

  const inputPreview = useMemo(() => {
    if (parsedInput) {
      // Tool-specific previews
      if ("command" in parsedInput) return String(parsedInput.command);
      if ("file_path" in parsedInput) return String(parsedInput.file_path);
      if ("pattern" in parsedInput) {
        const path = parsedInput.path ? ` in ${parsedInput.path}` : "";
        return `${parsedInput.pattern}${path}`;
      }
      if ("description" in parsedInput && "prompt" in parsedInput) {
        // Agent tool
        const type = parsedInput.subagent_type ? `[${parsedInput.subagent_type}] ` : "";
        return `${type}${parsedInput.description}`;
      }
      if ("query" in parsedInput) return String(parsedInput.query);
      if ("url" in parsedInput) return String(parsedInput.url);
      if ("old_string" in parsedInput && "new_string" in parsedInput) {
        return `${String(parsedInput.old_string).slice(0, 30)} → ${String(parsedInput.new_string).slice(0, 30)}`;
      }
      if ("content" in parsedInput && "file_path" in parsedInput) {
        return String(parsedInput.file_path);
      }
      if ("content" in parsedInput) return String(parsedInput.content).slice(0, 60);
      if ("skill" in parsedInput) return String(parsedInput.skill);
    }
    return toolInput.slice(0, 100);
  }, [parsedInput, toolInput]);

  return (
    <div>
      <button
        className="flex w-full items-center gap-2 rounded-lg border border-border/40 px-3 py-2 text-body-sm transition-colors hover:bg-accent/50"
        style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-secondary))" }}
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3 shrink-0 text-muted-foreground" />
        )}
        <ToolIcon className="size-3.5 shrink-0" style={{ color }} />
        <span className="font-medium" style={{ color }}>{label}</span>
        <ToolStatusBadge status="running" />
        {!expanded && inputPreview && (
          <span className="flex-1 truncate text-left font-mono text-[11px] text-muted-foreground">
            {inputPreview}
          </span>
        )}
      </button>
      {/* Running output placeholder for Bash tools */}
      {!expanded && toolName.toLowerCase() === "bash" && parsedInput && "command" in parsedInput && (
        <div className="mt-0.5 rounded-b-lg border border-t-0 border-border/40 bg-muted/30 px-3 py-2 font-mono text-[11px] text-muted-foreground/60">
          <div className="text-foreground/50">$ {String(parsedInput.command).slice(0, 80)}</div>
          <div className="mt-1 flex items-center gap-1.5">
            <span className="inline-block size-2 animate-spin rounded-full border border-current border-t-transparent" />
            <span>执行中…</span>
          </div>
        </div>
      )}
      {expanded && (
        <div
          className="mt-0.5 overflow-hidden rounded-b-lg border border-t-0 border-border/40"
          style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-secondary))" }}
        >
          {parsedInput ? (
            <StructuredToolInput params={parsedInput} />
          ) : (
            <pre className="overflow-x-auto whitespace-pre-wrap p-3 font-mono text-label text-foreground/80">
              {toolInput}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

/* ─── Structured tool input display ──────────────────────────────── */

function StructuredToolInput({ params }: { params: Record<string, unknown> }) {
  return (
    <div className="divide-y divide-border/30">
      {Object.entries(params).map(([key, value]) => {
        const strValue = typeof value === "string" ? value : JSON.stringify(value, null, 2);
        const isLong = strValue.length > 120;

        return (
          <div key={key} className="flex gap-2 px-3 py-1.5">
            <span className="shrink-0 font-mono text-caption font-semibold uppercase tracking-wider text-muted-foreground">
              {key}
            </span>
            {isLong ? (
              <pre className="flex-1 overflow-x-auto whitespace-pre-wrap font-mono text-label text-foreground/80">
                {strValue}
              </pre>
            ) : (
              <span className="flex-1 truncate font-mono text-label text-foreground/80">
                {strValue}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ─── Tool result message ────────────────────────────────────────── */

function ToolResultMessage({ message }: { message: ConversationMessage }) {
  const [expanded, setExpanded] = useState(false);
  const toolName = message.toolResult?.toolName ?? "Result";
  const output = message.toolResult?.output ?? message.content;
  const isError = message.toolResult?.isError ?? false;

  const lines = output.split("\n");
  const lineCount = lines.length;
  const isLong = lineCount > 3;

  // Detect if output looks like a diff
  const isDiff = output.includes("@@") && (output.includes("---") || output.includes("+++"));

  const { icon: ToolIcon, color } = getToolMeta(toolName);

  return (
    <div>
      <button
        className={cn(
          "flex w-full items-center gap-2 rounded-lg border px-3 py-1.5 text-body-sm transition-colors",
          isError
            ? "border-[color:var(--color-error)]/30 bg-[color:var(--color-error)]/5 hover:bg-[color:var(--color-error)]/10"
            : "border-[color:var(--color-success)]/20 bg-[color:var(--color-success)]/5 hover:bg-[color:var(--color-success)]/10"
        )}
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0" />
        ) : (
          <ChevronRight className="size-3 shrink-0" />
        )}
        {isError ? (
          <AlertCircle className="size-3.5 shrink-0" style={{ color: "var(--color-error)" }} />
        ) : (
          <ToolIcon className="size-3.5 shrink-0" style={{ color }} />
        )}
        <span className="font-medium">{toolName}</span>
        <ToolStatusBadge status={isError ? "error" : "completed"} />
        {!expanded && (
          <>
            {isLong && (
              <span className="rounded bg-muted/50 px-1 py-0.5 text-[10px] text-muted-foreground">
                {lineCount} 行
              </span>
            )}
            <span className="flex-1 truncate text-left font-mono text-[11px] text-muted-foreground">
              {lines[0]?.slice(0, 80)}
            </span>
          </>
        )}
      </button>
      {expanded && (
        <div
          className="mt-0.5 max-h-[400px] overflow-auto rounded-b-lg border border-t-0 border-border/40"
          style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-secondary))" }}
        >
          <ToolResultContent toolName={toolName} output={output} isDiff={isDiff} isError={isError} />
        </div>
      )}
    </div>
  );
}

/* ─── Tool-specific result content ──────────────────────────────── */

function ToolResultContent({
  toolName,
  output,
  isDiff,
  isError,
}: {
  toolName: string;
  output: string;
  isDiff: boolean;
  isError: boolean;
}) {
  const lower = toolName.toLowerCase();

  // Glob — render as file list
  if (lower === "glob" || lower === "glob_search") {
    return <GlobResult output={output} />;
  }

  // Grep — render with search context
  if (lower === "grep" || lower === "grep_search") {
    return <GrepResult output={output} />;
  }

  // Write / Edit — show file operation result
  if (lower === "write" || lower === "writefile" || lower === "write_file"
    || lower === "edit" || lower === "editfile" || lower === "edit_file") {
    if (isDiff) return <DiffDisplay content={output} />;
    return <FileOpResult output={output} isError={isError} />;
  }

  // Agent — show subagent result with branding
  if (lower === "agent") {
    return <AgentResult output={output} />;
  }

  // WebFetch / WebSearch — show web result
  if (lower.includes("webfetch") || lower.includes("web_fetch") || lower.includes("websearch") || lower.includes("web_search")) {
    return <WebResult output={output} isSearch={lower.includes("search")} />;
  }

  // Diff detection fallback
  if (isDiff) return <DiffDisplay content={output} />;

  // Default
  return (
    <pre className="whitespace-pre-wrap p-3 font-mono text-label leading-[1.6] text-foreground/80">
      {output}
    </pre>
  );
}

/* ─── Glob result — file list ───────────────────────────────────── */

function GlobResult({ output }: { output: string }) {
  const files = output.split("\n").filter((l) => l.trim());

  return (
    <div className="divide-y divide-border/20 py-1">
      {files.map((file, i) => {
        const isDir = file.endsWith("/");
        const name = file.split("/").pop() ?? file;
        const dir = file.slice(0, file.length - name.length);

        return (
          <div
            key={i}
            className="flex items-center gap-2 px-3 py-1"
          >
            {isDir ? (
              <Folder className="size-3 shrink-0" style={{ color: "var(--color-warning)" }} />
            ) : (
              <File className="size-3 shrink-0 text-muted-foreground" />
            )}
            <span className="font-mono text-label text-muted-foreground/60">
              {dir}
            </span>
            <span className="font-mono text-label text-foreground/80">
              {name}
            </span>
          </div>
        );
      })}
      <div className="px-3 py-1 text-caption text-muted-foreground">
        {files.length} 个文件匹配
      </div>
    </div>
  );
}

/* ─── Grep result — search matches with context ───────────────────── */

function GrepResult({ output }: { output: string }) {
  const lines = output.split("\n").filter((l) => l.trim());

  return (
    <pre className="p-3 font-mono text-label leading-[1.6]">
      {lines.map((line, i) => {
        // File path headers (e.g. "src/main.rs:42:")
        const hasFilePrefix = /^[^\s].*:\d+[:-]/.test(line);
        // Separator lines (e.g. "--")
        const isSeparator = line === "--";

        let lineClass = "text-foreground/80";
        if (isSeparator) {
          lineClass = "text-muted-foreground/40";
        } else if (hasFilePrefix) {
          // Highlight the file:line prefix
          const colonIdx = line.indexOf(":");
          if (colonIdx > 0) {
            return (
              <div key={i} className="px-1">
                <span className="text-[color:var(--claude-blue)]">{line.slice(0, colonIdx)}</span>
                <span className="text-muted-foreground">{line.slice(colonIdx, line.indexOf(":", colonIdx + 1) + 1)}</span>
                <span className="text-foreground/80">{line.slice(line.indexOf(":", colonIdx + 1) + 1)}</span>
              </div>
            );
          }
        }

        return (
          <div key={i} className={cn("px-1", lineClass)}>
            {line || " "}
          </div>
        );
      })}
      <div className="mt-1 border-t border-border/20 pt-1 text-caption text-muted-foreground">
        {lines.length} 行
      </div>
    </pre>
  );
}

/* ─── File operation result ─────────────────────────────────────── */

function FileOpResult({ output, isError }: { output: string; isError: boolean }) {
  return (
    <div className="p-3">
      <div className="flex items-center gap-2 text-label">
        {isError ? (
          <AlertCircle className="size-3.5" style={{ color: "var(--color-error)" }} />
        ) : (
          <CheckCircle2 className="size-3.5" style={{ color: "var(--color-success)" }} />
        )}
        <pre className="whitespace-pre-wrap font-mono text-label leading-[1.6] text-foreground/80">
          {output}
        </pre>
      </div>
    </div>
  );
}

/* ─── Agent result ──────────────────────────────────────────────── */

function AgentResult({ output }: { output: string }) {
  return (
    <div className="p-3">
      <div className="mb-2 flex items-center gap-1.5 text-caption font-medium" style={{ color: "var(--agent-purple)" }}>
        <Brain className="size-3" />
        <span>子代理结果</span>
      </div>
      <pre className="whitespace-pre-wrap font-mono text-label leading-[1.6] text-foreground/80">
        {output}
      </pre>
    </div>
  );
}

/* ─── Web result ────────────────────────────────────────────────── */

function WebResult({ output, isSearch }: { output: string; isSearch: boolean }) {
  // Try to extract URL from first line
  const lines = output.split("\n");
  const urlLine = lines.find((l) => l.startsWith("http"));

  return (
    <div className="p-3">
      {urlLine && (
        <div className="mb-2 flex items-center gap-1.5 rounded-md bg-muted/30 px-2 py-1">
          <Globe className="size-3 shrink-0" style={{ color: "var(--claude-blue)" }} />
          <span className="flex-1 truncate font-mono text-label text-foreground/70">
            {urlLine}
          </span>
          <ExternalLink className="size-3 shrink-0 text-muted-foreground" />
        </div>
      )}
      <div className="mb-1 text-caption font-medium text-muted-foreground">
        {isSearch ? "搜索结果" : "抓取内容"}
      </div>
      <pre className="whitespace-pre-wrap font-mono text-label leading-[1.6] text-foreground/80">
        {urlLine ? lines.filter((l) => l !== urlLine).join("\n") : output}
      </pre>
    </div>
  );
}

/* ─── Inline diff display ────────────────────────────────────────── */

function DiffDisplay({ content }: { content: string }) {
  return (
    <pre className="p-3 font-mono text-label leading-[1.6]">
      {content.split("\n").map((line, i) => {
        let lineClass = "text-foreground/80";
        if (line.startsWith("+") && !line.startsWith("+++")) {
          lineClass = "text-[color:var(--color-diff-added-word,rgb(47,157,68))] bg-[color:var(--color-diff-added,rgb(105,219,124))]/15";
        } else if (line.startsWith("-") && !line.startsWith("---")) {
          lineClass = "text-[color:var(--color-diff-removed-word,rgb(209,69,75))] bg-[color:var(--color-diff-removed,rgb(255,168,180))]/15";
        } else if (line.startsWith("@@")) {
          lineClass = "text-[color:var(--claude-blue,rgb(87,105,247))]";
        } else if (line.startsWith("---") || line.startsWith("+++")) {
          lineClass = "text-muted-foreground font-semibold";
        }

        return (
          <div key={i} className={cn("px-1", lineClass)}>
            {line || " "}
          </div>
        );
      })}
    </pre>
  );
}

/* ─── Error message ──────────────────────────────────────────────── */

function ErrorMessage({ content }: { content: string }) {
  return (
    <div
      className="flex items-start gap-2 rounded-lg border px-3 py-2"
      style={{
        borderColor: "var(--deeptutor-danger, var(--color-error))",
        backgroundColor: "var(--deeptutor-danger-soft, color-mix(in srgb, var(--color-error) 5%, transparent))",
      }}
    >
      <AlertCircle className="mt-0.5 size-3.5 shrink-0" style={{ color: "var(--color-error)" }} />
      <div className="font-mono text-body-sm" style={{ color: "var(--color-error)" }}>
        {content}
      </div>
    </div>
  );
}

/* ─── Tool metadata — imported from tool-meta.ts ───────────────── */

/* ─── TodoWrite message ─────────────────────────────────────────── */

interface TodoItem {
  content: string;
  status: "pending" | "in_progress" | "completed";
  activeForm?: string;
}

function isTodoToolResult(message: ConversationMessage): boolean {
  const name = message.toolResult?.toolName?.toLowerCase() ?? "";
  return name === "todowrite" || name === "todo_write";
}

function parseTodoOutput(output: string): TodoItem[] {
  try {
    const parsed = JSON.parse(output) as unknown;
    if (Array.isArray(parsed)) return parsed as TodoItem[];
    if (typeof parsed === "object" && parsed !== null && "todos" in parsed) {
      return (parsed as { todos: TodoItem[] }).todos;
    }
  } catch {
    // Not JSON — try to extract from text.
  }
  return [];
}

function TodoMessage({ message }: { message: ConversationMessage }) {
  const output = message.toolResult?.output ?? message.content;
  const todos = parseTodoOutput(output);

  if (todos.length === 0) {
    return <ToolResultMessage message={message} />;
  }

  return (
    <div className="rounded-lg border border-[color:var(--color-terminal-tool)]/20 bg-[color:var(--color-terminal-tool)]/5 p-3">
      <div className="mb-2 flex items-center gap-2 text-body font-medium">
        <CheckCircle2 className="size-4" style={{ color: "var(--color-terminal-tool)" }} />
        <span>任务列表</span>
        <span className="ml-auto rounded bg-muted/50 px-1.5 py-0.5 text-caption text-muted-foreground">
          {todos.filter((t) => t.status === "completed").length}/{todos.length}
        </span>
      </div>
      <div className="space-y-1">
        {todos.map((todo, i) => (
          <div key={`${todo.content}-${i}`} className="flex items-start gap-2 py-0.5">
            <span className="mt-0.5 text-body-sm">
              {todo.status === "completed" && (
                <CheckCircle2 className="size-3.5 text-[color:var(--color-success)]" />
              )}
              {todo.status === "in_progress" && (
                <span className="inline-block size-3.5 animate-spin rounded-full border-2 border-[color:var(--color-warning)] border-t-transparent" />
              )}
              {todo.status === "pending" && (
                <span className="inline-block size-3.5 rounded-full border-2 border-muted-foreground/40" />
              )}
            </span>
            <span
              className={cn(
                "text-body-sm",
                todo.status === "completed" && "text-muted-foreground line-through",
                todo.status === "in_progress" && "font-medium",
              )}
            >
              {todo.status === "in_progress" && todo.activeForm
                ? todo.activeForm
                : todo.content}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

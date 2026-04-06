import { memo, useState, useMemo } from "react";
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import {
  ChevronDown,
  ChevronRight,
  Terminal as TerminalIcon,
  Search,
  Globe,
  AlertCircle,
  CheckCircle2,
  Eye,
  FileText,
  FolderSearch,
  Copy,
  Check,
  Brain,
  Pencil,
  FileCode,
  BookOpen,
  File,
  Folder,
  ExternalLink,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { ConversationMessage } from "./types";

interface MessageItemProps {
  message: ConversationMessage;
}

export const MessageItem = memo(function MessageItem({ message }: MessageItemProps) {
  switch (message.type) {
    case "text":
      return message.role === "user" ? (
        <UserMessage content={message.content} />
      ) : message.role === "system" ? (
        <SystemMessage content={message.content} />
      ) : (
        <AssistantMessage content={message.content} />
      );
    case "tool_use":
      return <ToolUseMessage message={message} />;
    case "tool_result":
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
              className="rounded-[3px] bg-muted px-1.5 py-0.5 font-mono text-body-sm text-foreground"
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
    <div className="group/code relative my-2 overflow-hidden rounded-lg border border-border/50">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border/50 bg-muted/40 px-3 py-1">
        <span className="font-mono text-caption uppercase tracking-wider text-muted-foreground">
          {language}
        </span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-caption text-muted-foreground opacity-0 transition-opacity hover:bg-accent group-hover/code:opacity-100"
        >
          {copied ? (
            <>
              <Check className="size-3" /> Copied
            </>
          ) : (
            <>
              <Copy className="size-3" /> Copy
            </>
          )}
        </button>
      </div>
      <SyntaxHighlighter
        language={language}
        style={oneDark}
        customStyle={{
          margin: 0,
          padding: "12px 14px",
          fontSize: "12px",
          lineHeight: "1.5",
          background: "var(--color-msg-bash-bg, var(--color-muted))",
          borderRadius: 0,
        }}
        codeTagProps={{
          style: { fontFamily: "'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace" },
        }}
      >
        {code}
      </SyntaxHighlighter>
    </div>
  );
}

/* ─── User message ───────────────────────────────────────────────── */

function UserMessage({ content }: { content: string }) {
  return (
    <div className="mx-4 my-2">
      <div
        className="relative overflow-hidden rounded-lg border border-border/50"
        style={{ backgroundColor: "var(--color-msg-user-bg, var(--color-accent))" }}
      >
        <div
          className="absolute left-0 top-0 h-full w-[3px]"
          style={{ backgroundColor: "var(--color-label-you)" }}
        />
        <div className="py-2.5 pl-5 pr-4">
          <div
            className="mb-1 text-caption font-semibold uppercase tracking-wider"
            style={{ color: "var(--color-label-you)" }}
          >
            You
          </div>
          <div className="whitespace-pre-wrap text-body leading-relaxed text-foreground">
            {content}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ─── Assistant message with markdown ────────────────────────────── */

function AssistantMessage({ content }: { content: string }) {
  return (
    <div className="mx-4 my-2">
      <div
        className="relative overflow-hidden rounded-lg border border-border/50"
        style={{ backgroundColor: "var(--color-msg-assistant-bg, var(--color-background))" }}
      >
        <div
          className="absolute left-0 top-0 h-full w-[3px]"
          style={{ backgroundColor: "var(--color-label-claude)" }}
        />
        <div className="py-2.5 pl-5 pr-4">
          <div
            className="mb-1 text-caption font-semibold uppercase tracking-wider"
            style={{ color: "var(--color-label-claude)" }}
          >
            Assistant
          </div>
          <div className="text-body leading-relaxed text-foreground">
            <MarkdownContent content={content} />
          </div>
        </div>
      </div>
    </div>
  );
}

/* ─── System message ─────────────────────────────────────────────── */

function SystemMessage({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="mx-4 my-1">
      <button
        className="flex w-full items-center gap-2 rounded-lg border border-border/30 bg-muted/20 px-3 py-1.5 text-label text-muted-foreground transition-colors hover:bg-muted/30"
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        <Brain className="size-3" />
        <span className="font-medium">System</span>
        {!expanded && (
          <span className="flex-1 truncate text-left opacity-60">
            {content.slice(0, 80)}
          </span>
        )}
      </button>
      {expanded && (
        <div className="mt-1 rounded-b-lg border border-t-0 border-border/30 bg-muted/10 p-3">
          <pre className="whitespace-pre-wrap font-mono text-label text-muted-foreground">
            {content}
          </pre>
        </div>
      )}
    </div>
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
    <div className="mx-4 my-1">
      <button
        className="flex w-full items-center gap-2 rounded-lg border border-border/40 px-3 py-2 text-body-sm transition-colors hover:bg-muted/30"
        style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-muted))" }}
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3 shrink-0 text-muted-foreground" />
        )}
        <ToolIcon className="size-3.5 shrink-0" style={{ color }} />
        <span className="font-medium" style={{ color }}>{label}</span>
        {!expanded && inputPreview && (
          <span className="flex-1 truncate text-left font-mono text-label text-muted-foreground">
            {inputPreview}
          </span>
        )}
      </button>
      {expanded && (
        <div
          className="mt-0.5 overflow-hidden rounded-b-lg border border-t-0 border-border/40"
          style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-muted))" }}
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
    <div className="mx-4 my-1">
      <button
        className={cn(
          "flex w-full items-center gap-2 rounded-lg border px-3 py-1.5 text-body-sm transition-colors",
          isError
            ? "border-[color:var(--color-error,rgb(171,43,63))]/30 bg-[color:var(--color-error,rgb(171,43,63))]/5 hover:bg-[color:var(--color-error,rgb(171,43,63))]/10"
            : "border-[color:var(--color-success,rgb(44,122,57))]/20 bg-[color:var(--color-success,rgb(44,122,57))]/5 hover:bg-[color:var(--color-success,rgb(44,122,57))]/10"
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
        {!expanded && (
          <>
            {isLong && (
              <span className="rounded bg-muted/50 px-1 py-0.5 text-caption text-muted-foreground">
                {lineCount} lines
              </span>
            )}
            <span className="flex-1 truncate text-left font-mono text-label text-muted-foreground">
              {lines[0]?.slice(0, 80)}
            </span>
          </>
        )}
      </button>
      {expanded && (
        <div
          className="mt-0.5 max-h-[400px] overflow-auto rounded-b-lg border border-t-0 border-border/40"
          style={{ backgroundColor: "var(--color-msg-bash-bg, var(--color-muted))" }}
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
  if (lower === "glob") {
    return <GlobResult output={output} />;
  }

  // Write / Edit — show file operation result
  if (lower === "write" || lower === "writefile" || lower === "edit" || lower === "editfile") {
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
        {files.length} file{files.length !== 1 ? "s" : ""} matched
      </div>
    </div>
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
        <span>Subagent Result</span>
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
        {isSearch ? "Search Results" : "Fetched Content"}
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
    <div className="mx-4 my-2">
      <div
        className="flex items-start gap-2 rounded-lg border px-3 py-2"
        style={{
          borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
        }}
      >
        <AlertCircle className="mt-0.5 size-3.5 shrink-0" style={{ color: "var(--color-error)" }} />
        <div className="font-mono text-body-sm" style={{ color: "var(--color-error)" }}>
          {content}
        </div>
      </div>
    </div>
  );
}

/* ─── Tool metadata helper ───────────────────────────────────────── */

function getToolMeta(toolName: string): {
  icon: typeof TerminalIcon;
  label: string;
  color: string;
} {
  const lower = toolName.toLowerCase();

  if (lower === "bash" || lower.includes("shell"))
    return { icon: TerminalIcon, label: "Bash", color: "var(--color-terminal-tool)" };
  if (lower === "read" || lower === "readfile")
    return { icon: Eye, label: "Read", color: "var(--claude-blue)" };
  if (lower === "edit" || lower === "editfile")
    return { icon: Pencil, label: "Edit", color: "var(--claude-orange)" };
  if (lower === "write" || lower === "writefile")
    return { icon: FileCode, label: "Write", color: "var(--claude-orange)" };
  if (lower === "glob")
    return { icon: FolderSearch, label: "Glob", color: "var(--color-terminal-tool)" };
  if (lower === "grep")
    return { icon: Search, label: "Grep", color: "var(--color-terminal-tool)" };
  if (lower.includes("webfetch") || lower.includes("web_fetch"))
    return { icon: Globe, label: "WebFetch", color: "var(--claude-blue)" };
  if (lower.includes("websearch") || lower.includes("web_search"))
    return { icon: Globe, label: "WebSearch", color: "var(--claude-blue)" };
  if (lower === "agent")
    return { icon: Brain, label: "Agent", color: "var(--agent-purple)" };
  if (lower.includes("notebook"))
    return { icon: BookOpen, label: "Notebook", color: "var(--claude-blue)" };
  if (lower.includes("todowrite") || lower.includes("todo"))
    return { icon: CheckCircle2, label: "TodoWrite", color: "var(--color-terminal-tool)" };
  if (lower.includes("skill"))
    return { icon: FileText, label: "Skill", color: "var(--agent-cyan)" };

  // Fallback
  return { icon: TerminalIcon, label: toolName, color: "var(--color-terminal-tool)" };
}

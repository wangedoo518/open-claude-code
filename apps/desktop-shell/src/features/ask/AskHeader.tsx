// S0.3 extraction target: Ask page's top content-header strip.
//
// Original: features/session-workbench/ContentHeader.tsx. Verbatim port
// with one behavior change: the `title` default drops from "Warwolf" to
// "Ask" because this header is mounted inside the Ask page only.
// Everything else (streaming indicator, model / environment badges,
// agent-panel toggle, export dropdown) is unchanged so S3 can swap in
// the ask_runtime-backed props with no template rework.

import { useState, useRef, useEffect } from "react";
import { Brain, Download, FileJson, FileText } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

interface AskHeaderProps {
  title?: string;
  projectPath?: string;
  modelLabel?: string;
  environmentLabel?: string;
  isStreaming?: boolean;
  agentCount?: number;
  showAgentPanel?: boolean;
  onToggleAgentPanel?: () => void;
  onExportMarkdown?: () => void;
  onExportJson?: () => void;
}

export function AskHeader({
  title = "Ask",
  projectPath,
  modelLabel = "Codex GPT-5.4",
  environmentLabel = "via internal broker",
  isStreaming = false,
  agentCount = 0,
  showAgentPanel = false,
  onToggleAgentPanel,
  onExportMarkdown,
  onExportJson,
}: AskHeaderProps) {
  return (
    <div className="flex items-start justify-between px-4 pb-1.5 pt-2.5">
      {/* Left: title + project path */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <h1 className="text-body font-semibold text-foreground">{title}</h1>
          {isStreaming && (
            <span className="flex items-center gap-1 text-caption" style={{ color: "var(--claude-orange)" }}>
              <span
                className="inline-block size-1.5 animate-pulse rounded-full"
                style={{ backgroundColor: "var(--claude-orange)" }}
              />
              Streaming
            </span>
          )}
        </div>
        {projectPath && (
          <p className="mt-0.5 truncate text-label text-muted-foreground">
            {projectPath}
          </p>
        )}
      </div>

      {/* Right: badges + agent toggle */}
      <div className="flex shrink-0 items-center gap-1.5 pt-0.5">
        <Badge
          variant="secondary"
          className="h-[18px] rounded-md px-1.5 text-caption font-medium"
        >
          {modelLabel}
        </Badge>
        <Badge
          variant="outline"
          className="h-[18px] rounded-md px-1.5 text-caption font-medium"
        >
          {environmentLabel}
        </Badge>
        {onToggleAgentPanel && (
          <button
            className={cn(
              "relative flex h-[18px] items-center gap-1 rounded-md border px-1.5 text-caption font-medium transition-colors",
              showAgentPanel
                ? "border-[color:var(--agent-purple,rgb(147,51,234))]/30 bg-[color:var(--agent-purple,rgb(147,51,234))]/10 text-[color:var(--agent-purple,rgb(147,51,234))]"
                : "border-border/50 text-muted-foreground hover:bg-accent hover:text-foreground"
            )}
            onClick={onToggleAgentPanel}
          >
            <Brain className="size-3" />
            {agentCount > 0 && (
              <span>{agentCount}</span>
            )}
          </button>
        )}
        {(onExportMarkdown || onExportJson) && (
          <ExportDropdown
            onExportMarkdown={onExportMarkdown}
            onExportJson={onExportJson}
          />
        )}
      </div>
    </div>
  );
}

/* ─── Export Dropdown ─────────────────────────────────────────── */

function ExportDropdown({
  onExportMarkdown,
  onExportJson,
}: {
  onExportMarkdown?: () => void;
  onExportJson?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        className="flex h-[18px] items-center gap-1 rounded-md border border-border/50 px-1.5 text-caption font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onClick={() => setOpen((v) => !v)}
        title="Export session"
      >
        <Download className="size-3" />
      </button>
      {open && (
        <div className="absolute right-0 top-full z-50 mt-1 min-w-[140px] rounded-md border border-border bg-popover py-1 shadow-md">
          {onExportMarkdown && (
            <button
              className="flex w-full items-center gap-2 px-3 py-1.5 text-label text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                onExportMarkdown();
                setOpen(false);
              }}
            >
              <FileText className="size-3" />
              Export as Markdown
            </button>
          )}
          {onExportJson && (
            <button
              className="flex w-full items-center gap-2 px-3 py-1.5 text-label text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                onExportJson();
                setOpen(false);
              }}
            >
              <FileJson className="size-3" />
              Export as JSON
            </button>
          )}
        </div>
      )}
    </div>
  );
}

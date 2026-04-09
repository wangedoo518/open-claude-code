/**
 * Schema Editor · Maintainer 的纪律 (wireframes.html §09)
 *
 * S6 MVP implementation. Canonical §10 says `schema/` is human-owned
 * and the maintainer agent may only PROPOSE changes via the Inbox —
 * never write directly. This page is therefore READ-ONLY by contract:
 * it renders `schema/CLAUDE.md` with a friendly markdown preview plus
 * the resolved disk path so users can see exactly which file drives
 * the maintainer's behavior.
 *
 * What's NOT in S6 MVP:
 *   - Proposal diff view (needs the maintainer LLM to propose anything,
 *     which needs codex_broker::chat_completion to be wired)
 *   - Left-pane file tree of AGENTS.md / templates/ / policies/
 *     (S6 only reads CLAUDE.md; future sprints can add endpoints for
 *     the other schema files)
 *   - Inline editing (contradicts "human-owned" rule; if users want
 *     to edit they open the file in their OS editor)
 */

import { useQuery } from "@tanstack/react-query";
import { Loader2, Ruler, FileText, ShieldAlert } from "lucide-react";
import { getWikiSchema } from "@/features/ingest/persist";

export function SchemaEditorPage() {
  const schemaQuery = useQuery({
    queryKey: ["wiki", "schema"] as const,
    queryFn: () => getWikiSchema(),
    staleTime: 30_000,
  });

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Hero */}
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <div className="flex items-baseline gap-3">
          <span className="text-xl">📐</span>
          <h1
            className="text-head font-semibold text-foreground"
            style={{ fontFamily: "var(--font-serif, Lora, serif)" }}
          >
            Schema Editor · Maintainer 的纪律
          </h1>
        </div>
        <p className="mt-1 text-label text-muted-foreground">
          AI 的纪律是什么 — <code>schema/CLAUDE.md</code> 是维护 agent 的唯一行为契约 · 人写优先 · AI 只能通过 Inbox 提议修改
        </p>
      </div>

      {/* Body */}
      <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
        {schemaQuery.isLoading ? (
          <div className="flex items-center gap-2 text-caption text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            Loading schema…
          </div>
        ) : schemaQuery.error ? (
          <div
            className="rounded-md border px-3 py-2 text-caption"
            style={{
              borderColor:
                "color-mix(in srgb, var(--color-error) 30%, transparent)",
              backgroundColor:
                "color-mix(in srgb, var(--color-error) 5%, transparent)",
              color: "var(--color-error)",
            }}
          >
            Failed to load schema: {(schemaQuery.error as Error).message}
          </div>
        ) : schemaQuery.data ? (
          <SchemaBody
            content={schemaQuery.data.content}
            path={schemaQuery.data.path}
            source={schemaQuery.data.source}
            byteSize={schemaQuery.data.byte_size}
          />
        ) : null}
      </div>
    </div>
  );
}

function SchemaBody({
  content,
  path,
  source,
  byteSize,
}: {
  content: string;
  path: string;
  source: "disk";
  byteSize: number;
}) {
  return (
    <div className="mx-auto max-w-4xl space-y-4">
      {/* Path card */}
      <div className="rounded-md border border-border bg-muted/10 px-4 py-3">
        <div className="mb-1 flex items-center gap-2 text-caption uppercase tracking-wide text-muted-foreground">
          <FileText className="size-3" />
          Source
        </div>
        <div className="flex items-center justify-between gap-3">
          <code className="break-all font-mono text-body-sm text-foreground">
            {path}
          </code>
          <div className="shrink-0 text-caption text-muted-foreground">
            {byteSize} bytes · {source === "disk" ? "on disk" : source}
          </div>
        </div>
      </div>

      {/* Read-only notice */}
      <div
        className="flex items-start gap-2 rounded-md border px-4 py-3"
        style={{
          borderColor: "color-mix(in srgb, var(--color-warning) 30%, transparent)",
          backgroundColor:
            "color-mix(in srgb, var(--color-warning) 5%, transparent)",
        }}
      >
        <ShieldAlert
          className="mt-0.5 size-4 shrink-0"
          style={{ color: "var(--color-warning)" }}
        />
        <div className="text-caption text-foreground/90">
          <div className="mb-0.5 font-semibold">Human-owned file</div>
          <div className="text-muted-foreground">
            Per canonical §10 the <code>schema/</code> directory is
            human-only. The maintainer agent may PROPOSE changes to this
            file via the{" "}
            <a href="#/inbox" className="text-primary hover:underline">
              Inbox
            </a>
            , but never writes directly. This page is read-only. To edit
            the rules, open <code>{path}</code> in your OS editor.
          </div>
        </div>
      </div>

      {/* Content pane */}
      <div className="rounded-md border border-border bg-background">
        <div className="flex items-center gap-2 border-b border-border/50 bg-muted/5 px-4 py-2">
          <Ruler
            className="size-3.5"
            style={{ color: "var(--claude-orange)" }}
          />
          <span className="font-mono text-caption text-muted-foreground">
            CLAUDE.md
          </span>
        </div>
        <pre
          className="overflow-x-auto whitespace-pre-wrap px-5 py-4 font-mono text-body-sm leading-relaxed text-foreground/90"
          style={{ fontFamily: "var(--font-mono, 'JetBrains Mono', monospace)" }}
        >
          {content}
        </pre>
      </div>
    </div>
  );
}

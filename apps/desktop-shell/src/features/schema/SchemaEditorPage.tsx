/**
 * Schema Editor · Maintainer 的纪律
 *
 * S6 MVP shipped a read-only viewer; feat(M) adds write mode:
 * canonical §8 says "schema/ is human-curated", so the HUMAN write
 * path is a direct edit-and-save. (The maintainer agent's PROPOSE
 * path through Inbox is a separate, future feature — see Tier 3 R.)
 *
 * Layout:
 *   - Hero header
 *   - Source path + size card
 *   - Read-only notice toggles to "Editing" notice when in edit mode
 *   - Content pane is either a <pre> (view) or <textarea> (edit)
 *   - Action bar at the bottom: Edit / Save / Cancel
 *
 * Save flow:
 *   1. User clicks Edit → enter edit mode, copy server content into draft
 *   2. User edits draft, clicks Save → PUT /api/wiki/schema
 *   3. On success → exit edit mode, refetch schema, show "Saved" toast
 *   4. On failure → stay in edit mode, show error inline
 *
 * What's STILL not in:
 *   - Markdown rendered preview (raw monospace is fine for a rules file)
 *   - Diff view (no proposal source to diff against yet)
 *   - Left-pane file tree of AGENTS.md / templates/ / policies/
 */

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Loader2,
  FileText,
  ShieldAlert,
  Pencil,
  Save,
  X,
  CheckCircle2,
  Bot,
  FileCode2,
} from "lucide-react";
import { getSchemaTemplates, getWikiSchema, putWikiSchema } from "@/api/wiki/repository";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";

export function SchemaEditorPage() {
  const queryClient = useQueryClient();
  const schemaQuery = useQuery({
    queryKey: ["wiki", "schema"] as const,
    queryFn: () => getWikiSchema(),
    staleTime: 30_000,
  });
  const templatesQuery = useQuery({
    queryKey: ["wiki", "schema", "templates"] as const,
    queryFn: () => getSchemaTemplates(),
    staleTime: 60_000,
  });

  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [savedAt, setSavedAt] = useState<number | null>(null);

  // Reset draft whenever fresh server data arrives and we're not
  // mid-edit (so Save+refetch ends up showing the new content
  // rather than reverting to the old draft).
  useEffect(() => {
    if (!isEditing && schemaQuery.data) {
      setDraft(schemaQuery.data.content);
    }
  }, [schemaQuery.data, isEditing]);

  const saveMutation = useMutation({
    mutationFn: (content: string) => putWikiSchema(content),
    onSuccess: () => {
      setIsEditing(false);
      setSavedAt(Date.now());
      void queryClient.invalidateQueries({ queryKey: ["wiki", "schema"] });
    },
  });

  const handleEdit = () => {
    if (schemaQuery.data) {
      setDraft(schemaQuery.data.content);
      setIsEditing(true);
      setSavedAt(null);
    }
  };

  const handleCancel = () => {
    if (schemaQuery.data) {
      setDraft(schemaQuery.data.content);
    }
    setIsEditing(false);
    saveMutation.reset();
  };

  const handleSave = () => {
    if (draft.trim().length === 0) return;
    saveMutation.mutate(draft);
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Hero */}
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <h1 className="text-lg text-foreground">
          Rules Studio
        </h1>
        <p className="mt-1 text-muted-foreground/60" style={{ fontSize: 11 }}>
          用户教外脑如何整理：Types、Templates、Policies、Guidance 与 Validation 收束在一个工作区。
        </p>
      </div>

      {/* Body */}
      <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
        {schemaQuery.isLoading ? (
          <div className="flex items-center gap-2 text-caption text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            加载整理规则…
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
            加载 Schema 失败：{(schemaQuery.error as Error).message}
          </div>
        ) : schemaQuery.data ? (
          <SchemaBody
            content={schemaQuery.data.content}
            path={schemaQuery.data.path}
            source={schemaQuery.data.source}
            byteSize={schemaQuery.data.byte_size}
            templateCount={templatesQuery.data?.length ?? 0}
            isEditing={isEditing}
            draft={draft}
            onDraftChange={setDraft}
            onEdit={handleEdit}
            onCancel={handleCancel}
            onSave={handleSave}
            saveError={(saveMutation.error as Error | null)?.message ?? null}
            isSaving={saveMutation.isPending}
            savedAt={savedAt}
          />
        ) : null}
      </div>
    </div>
  );
}

interface SchemaBodyProps {
  content: string;
  path: string;
  source: "disk";
  byteSize: number;
  templateCount: number;
  isEditing: boolean;
  draft: string;
  onDraftChange: (next: string) => void;
  onEdit: () => void;
  onCancel: () => void;
  onSave: () => void;
  saveError: string | null;
  isSaving: boolean;
  savedAt: number | null;
}

function SchemaBody({
  content,
  path,
  source,
  byteSize,
  templateCount,
  isEditing,
  draft,
  onDraftChange,
  onEdit,
  onCancel,
  onSave,
  saveError,
  isSaving,
  savedAt,
}: SchemaBodyProps) {
  const justSaved = savedAt != null && Date.now() - savedAt < 4000;

  return (
    <div className="mx-auto max-w-4xl space-y-4">
      <div className="grid gap-2 md:grid-cols-5">
        {[
          ["Types", "字段与类型"],
          ["Templates", `${templateCount} 个模板`],
          ["Policies", "维护策略"],
          ["Guidance", "AGENTS / CLAUDE"],
          ["Validation", "巡检结果"],
        ].map(([title, desc]) => (
          <div key={title} className="rounded-md border border-border/50 bg-card px-3 py-3">
            <div className="text-[12px] font-medium text-foreground">{title}</div>
            <div className="mt-1 text-[11px] text-muted-foreground">{desc}</div>
          </div>
        ))}
      </div>

      <div className="rounded-md border border-border/50 bg-card px-4 py-3">
        <div className="flex items-start gap-2">
          <Bot className="mt-0.5 size-4 text-primary" />
          <div className="text-[12px] leading-5 text-muted-foreground">
            外部 AI 首期允许受控写入 <code>wiki/</code>、
            <code>schema/templates</code> 与 root guidance；自动写入分为本次会话有效和永久规则。
          </div>
        </div>
      </div>

      {/* Path card */}
      <div className="rounded-md border border-border/40 px-4 py-3">
        <div className="mb-1.5 flex items-center gap-2 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
          <FileText className="size-3" />
          Source
        </div>
        <div className="flex items-center justify-between gap-3">
          <code className="break-all font-mono text-foreground/80" style={{ fontSize: 12 }}>
            {path}
          </code>
          <div className="shrink-0 text-muted-foreground/40" style={{ fontSize: 11 }}>
            {byteSize} bytes · {source === "disk" ? "on disk" : source}
          </div>
        </div>
      </div>

      {/* Mode notice */}
      {isEditing ? (
        <div
          className="flex items-start gap-2 rounded-md border px-4 py-3"
          style={{
            borderColor:
              "color-mix(in srgb, var(--claude-orange) 40%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--claude-orange) 6%, transparent)",
          }}
        >
          <Pencil
            className="mt-0.5 size-4 shrink-0"
            style={{ color: "var(--claude-orange)" }}
          />
          <div className="text-caption text-foreground/90">
            <div className="mb-0.5 font-semibold">编辑中</div>
            <div className="text-muted-foreground">
              点击保存会直接写入磁盘，整理 AI 会在下一次处理新素材时读取到新规则。取消则丢弃本次修改。
            </div>
          </div>
        </div>
      ) : (
        <div
          className="flex items-start gap-2 rounded-md border px-4 py-3"
          style={{
            borderColor:
              "color-mix(in srgb, var(--color-warning) 30%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--color-warning) 5%, transparent)",
          }}
        >
          <ShieldAlert
            className="mt-0.5 size-4 shrink-0"
            style={{ color: "var(--color-warning)" }}
          />
          <div className="text-caption text-foreground/90">
            <div className="mb-0.5 font-semibold">仅人工编辑</div>
            <div className="text-muted-foreground">
              <code>schema/</code> 目录只允许你手动修改。点「编辑」
              可以改写整理 AI 的规则；AI 自己不会直接写这里，如需调整它会把修改提案丢到
              {" "}
              <a href="#/inbox" className="text-primary hover:underline">
                待整理
              </a>
              。
            </div>
          </div>
        </div>
      )}

      {/* Content pane */}
      <details className="rounded-md border border-border bg-background" open={isEditing}>
        <summary className="flex cursor-pointer list-none items-center gap-2 border-b border-border/40 px-4 py-2">
          <FileCode2
            className="size-3.5"
            style={{ color: "var(--claude-orange)" }}
          />
          <span className="font-mono text-muted-foreground/70" style={{ fontSize: 11 }}>
            Advanced YAML / CodeMirror · CLAUDE.md
          </span>
          {justSaved ? (
            <span
              className="ml-auto inline-flex items-center gap-1 text-caption"
              style={{ color: "var(--color-success)" }}
            >
              <CheckCircle2 className="size-3" />
              Saved
            </span>
          ) : null}
        </summary>
        {isEditing ? (
          <Textarea
            value={draft}
            onChange={(e) => onDraftChange(e.target.value)}
            spellCheck={false}
            className="block min-h-[400px] w-full resize-y border-0 bg-background px-5 py-4 font-mono text-body-sm leading-relaxed text-foreground/90 shadow-none focus-visible:ring-0"
            style={{
              fontFamily: "var(--font-mono, 'JetBrains Mono', monospace)",
            }}
          />
        ) : (
          <pre
            className="overflow-x-auto whitespace-pre-wrap px-5 py-4 font-mono text-body-sm leading-relaxed text-foreground/90"
            style={{
              fontFamily: "var(--font-mono, 'JetBrains Mono', monospace)",
            }}
          >
            {content}
          </pre>
        )}
      </details>

      {/* Action bar */}
      <div className="flex items-center justify-end gap-2">
        {saveError ? (
          <span
            className="mr-auto text-caption"
            style={{ color: "var(--color-error)" }}
          >
            {saveError}
          </span>
        ) : null}
        {isEditing ? (
          <>
            <Button
              variant="outline"
              size="sm"
              onClick={onCancel}
              disabled={isSaving}
            >
              <X className="size-3" />
              Cancel
            </Button>
            <Button
              variant="default"
              size="sm"
              onClick={onSave}
              disabled={isSaving || draft.trim().length === 0}
            >
              {isSaving ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Save className="size-3" />
              )}
              保存
            </Button>
          </>
        ) : (
          <Button variant="default" size="sm" onClick={onEdit}>
            <Pencil className="size-3" />
            Edit
          </Button>
        )}
      </div>
    </div>
  );
}

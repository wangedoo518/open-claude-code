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

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Loader2,
  FileText,
  ShieldAlert,
  Pencil,
  RefreshCw,
  Save,
  X,
  CheckCircle2,
  Bot,
  FileCode2,
  GitBranch,
} from "lucide-react";
import {
  getGuidanceFiles,
  getPatrolReport,
  getPolicyFiles,
  getRulesFile,
  getSchemaTemplates,
  getVaultGitStatus,
  getWikiSchema,
  putRulesFile,
  putWikiSchema,
  triggerPatrol,
} from "@/api/wiki/repository";
import type {
  GuidanceFileInfo,
  PatrolReport,
  PolicyFileInfo,
  RulesFileContent,
  SchemaTemplate,
} from "@/api/wiki/types";
import { Button } from "@/components/ui/button";
import { CodeMirrorEditor } from "@/components/CodeMirrorEditor";

function rulesGitStatusLabel(
  git:
    | {
        git_available: boolean;
        initialized: boolean;
        dirty: boolean;
        changed_count: number;
      }
    | undefined,
  hasError: boolean,
) {
  if (hasError) return "Git 状态不可用";
  if (!git) return "检查中";
  if (!git.git_available) return "未安装 Git";
  if (!git.initialized) return "Git 未启用";
  if (git.dirty) return `${git.changed_count} 改动待 checkpoint`;
  return "当前 clean";
}

interface RuleFileOption {
  path: string;
  label: string;
}

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
  const guidanceQuery = useQuery({
    queryKey: ["wiki", "guidance"] as const,
    queryFn: () => getGuidanceFiles(),
    staleTime: 60_000,
  });
  const policiesQuery = useQuery({
    queryKey: ["wiki", "policies"] as const,
    queryFn: () => getPolicyFiles(),
    staleTime: 60_000,
  });
  const gitQuery = useQuery({
    queryKey: ["wiki", "git", "rules"],
    queryFn: () => getVaultGitStatus(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const patrolQuery = useQuery({
    queryKey: ["wiki", "patrol", "rules"],
    queryFn: () => getPatrolReport(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });

  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [selectedRulesPath, setSelectedRulesPath] = useState("schema/CLAUDE.md");
  const [isRulesFileEditing, setIsRulesFileEditing] = useState(false);
  const [rulesFileDraft, setRulesFileDraft] = useState("");
  const [savedAt, setSavedAt] = useState<number | null>(null);

  const ruleFileOptions = useMemo<RuleFileOption[]>(() => {
    const options = new Map<string, string>();
    for (const file of guidanceQuery.data?.files ?? []) {
      options.set(file.relative_path, file.label);
    }
    for (const template of templatesQuery.data ?? []) {
      const path = `schema/templates/${template.category}.md`;
      options.set(path, `Template · ${template.display_name}`);
    }
    for (const file of policiesQuery.data?.files ?? []) {
      options.set(file.relative_path, `Policy · ${file.label}`);
    }
    if (!options.has("schema/CLAUDE.md")) {
      options.set("schema/CLAUDE.md", "Schema CLAUDE.md");
    }
    return Array.from(options, ([path, label]) => ({ path, label })).sort((a, b) =>
      a.path.localeCompare(b.path),
    );
  }, [guidanceQuery.data?.files, policiesQuery.data?.files, templatesQuery.data]);

  const rulesFileQuery = useQuery({
    queryKey: ["wiki", "rules", "file", selectedRulesPath] as const,
    queryFn: () => getRulesFile(selectedRulesPath),
    staleTime: 30_000,
  });

  // Reset draft whenever fresh server data arrives and we're not
  // mid-edit (so Save+refetch ends up showing the new content
  // rather than reverting to the old draft).
  useEffect(() => {
    if (!isEditing && schemaQuery.data) {
      setDraft(schemaQuery.data.content);
    }
  }, [schemaQuery.data, isEditing]);

  useEffect(() => {
    if (!isRulesFileEditing && rulesFileQuery.data) {
      setRulesFileDraft(rulesFileQuery.data.content);
    }
  }, [rulesFileQuery.data, isRulesFileEditing]);

  const saveMutation = useMutation({
    mutationFn: (content: string) => putWikiSchema(content),
    onSuccess: () => {
      setIsEditing(false);
      setSavedAt(Date.now());
      void queryClient.invalidateQueries({ queryKey: ["wiki", "schema"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
    },
  });

  const saveRulesFileMutation = useMutation({
    mutationFn: () => putRulesFile(selectedRulesPath, rulesFileDraft),
    onSuccess: () => {
      setIsRulesFileEditing(false);
      setSavedAt(Date.now());
      void queryClient.invalidateQueries({ queryKey: ["wiki", "rules", "file"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "schema"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "schema", "templates"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "guidance"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "policies"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "git"] });
    },
  });
  const patrolMutation = useMutation({
    mutationFn: () => triggerPatrol(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["wiki", "patrol"] });
      void queryClient.invalidateQueries({ queryKey: ["wiki", "inbox"] });
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

  const handleSelectedRulesPathChange = (path: string) => {
    setSelectedRulesPath(path);
    setIsRulesFileEditing(false);
    saveRulesFileMutation.reset();
  };

  const handleEditRulesFile = () => {
    if (rulesFileQuery.data) {
      setRulesFileDraft(rulesFileQuery.data.content);
      setIsRulesFileEditing(true);
      setSavedAt(null);
      saveRulesFileMutation.reset();
    }
  };

  const handleCancelRulesFile = () => {
    if (rulesFileQuery.data) {
      setRulesFileDraft(rulesFileQuery.data.content);
    }
    setIsRulesFileEditing(false);
    saveRulesFileMutation.reset();
  };

  const handleSaveRulesFile = () => {
    if (rulesFileDraft.trim().length === 0) return;
    saveRulesFileMutation.mutate();
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
            templates={templatesQuery.data ?? []}
            guidanceFiles={guidanceQuery.data?.files ?? []}
            policyFiles={policiesQuery.data?.files ?? []}
            ruleFileOptions={ruleFileOptions}
            selectedRulesPath={selectedRulesPath}
            selectedRulesFile={rulesFileQuery.data ?? null}
            isRulesFileLoading={rulesFileQuery.isLoading}
            isRulesFileEditing={isRulesFileEditing}
            rulesFileDraft={rulesFileDraft}
            onSelectedRulesPathChange={handleSelectedRulesPathChange}
            onEditRulesFile={handleEditRulesFile}
            onCancelRulesFile={handleCancelRulesFile}
            onSaveRulesFile={handleSaveRulesFile}
            onRulesFileDraftChange={setRulesFileDraft}
            rulesFileSaveError={(saveRulesFileMutation.error as Error | null)?.message ?? null}
            isRulesFileSaving={saveRulesFileMutation.isPending}
            gitStatus={rulesGitStatusLabel(gitQuery.data, Boolean(gitQuery.error))}
            patrolReport={patrolMutation.data ?? patrolQuery.data ?? null}
            patrolLoading={patrolQuery.isLoading || patrolMutation.isPending}
            patrolError={
              ((patrolMutation.error ?? patrolQuery.error) as Error | null)?.message ?? null
            }
            onRunPatrol={() => patrolMutation.mutate()}
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
  templates: SchemaTemplate[];
  guidanceFiles: GuidanceFileInfo[];
  policyFiles: PolicyFileInfo[];
  ruleFileOptions: RuleFileOption[];
  selectedRulesPath: string;
  selectedRulesFile: RulesFileContent | null;
  isRulesFileLoading: boolean;
  isRulesFileEditing: boolean;
  rulesFileDraft: string;
  onSelectedRulesPathChange: (path: string) => void;
  onEditRulesFile: () => void;
  onCancelRulesFile: () => void;
  onSaveRulesFile: () => void;
  onRulesFileDraftChange: (content: string) => void;
  rulesFileSaveError: string | null;
  isRulesFileSaving: boolean;
  gitStatus: string;
  patrolReport: PatrolReport | null;
  patrolLoading: boolean;
  patrolError: string | null;
  onRunPatrol: () => void;
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
  templates,
  guidanceFiles,
  policyFiles,
  ruleFileOptions,
  selectedRulesPath,
  selectedRulesFile,
  isRulesFileLoading,
  isRulesFileEditing,
  rulesFileDraft,
  onSelectedRulesPathChange,
  onEditRulesFile,
  onCancelRulesFile,
  onSaveRulesFile,
  onRulesFileDraftChange,
  rulesFileSaveError,
  isRulesFileSaving,
  gitStatus,
  patrolReport,
  patrolLoading,
  patrolError,
  onRunPatrol,
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

      <div className="rounded-md border border-border/50 bg-card px-4 py-4">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="text-[14px] font-medium text-foreground">Templates</h2>
            <p className="mt-1 text-[12px] text-muted-foreground">
              schema/templates 是外脑写入 Wiki 时会参考的页面骨架。
            </p>
          </div>
          <span className="rounded bg-muted px-2 py-1 text-[11px] text-muted-foreground">
            {templateCount} files
          </span>
        </div>
        <div className="mt-4 grid gap-2 md:grid-cols-2">
          {templates.map((template) => (
            <TemplateSummaryCard key={template.category} template={template} />
          ))}
          {templates.length === 0 ? (
            <div className="rounded-md border border-border/50 bg-background px-3 py-3 text-[12px] text-muted-foreground">
              暂无 schema/templates 模板。
            </div>
          ) : null}
        </div>
      </div>

      <div className="rounded-md border border-border/50 bg-card px-4 py-3">
        <div className="flex items-start gap-2">
          <GitBranch className="mt-0.5 size-4 text-primary" />
          <div className="text-[12px] leading-5 text-muted-foreground">
            <div className="font-medium text-foreground">Git checkpoint</div>
            <div>
              Rules 保存会产生普通 Buddy Vault diff；当前状态：
              <span className="ml-1 text-foreground">{gitStatus}</span>
            </div>
          </div>
        </div>
      </div>

      <ValidationSnapshotCard
        report={patrolReport}
        isLoading={patrolLoading}
        error={patrolError}
        onRunPatrol={onRunPatrol}
      />

      <div className="rounded-md border border-border/50 bg-card px-4 py-4">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="text-[14px] font-medium text-foreground">Guidance</h2>
            <p className="mt-1 text-[12px] text-muted-foreground">
              root shims 让外部 AI 和 CLI agent 先读正确的 Buddy Vault 写入边界。
            </p>
          </div>
          <span className="rounded bg-muted px-2 py-1 text-[11px] text-muted-foreground">
            {guidanceFiles.filter((file) => file.exists).length}/{guidanceFiles.length || 4}
          </span>
        </div>
        <div className="mt-4 grid gap-2 md:grid-cols-2">
          {guidanceFiles.map((file) => (
            <GuidanceFileCard key={file.id} file={file} />
          ))}
          {guidanceFiles.length === 0 ? (
            <div className="rounded-md border border-border/50 bg-background px-3 py-3 text-[12px] text-muted-foreground">
              正在读取 root guidance 文件。
            </div>
          ) : null}
        </div>
      </div>

      <div className="rounded-md border border-border/50 bg-card px-4 py-4">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="text-[14px] font-medium text-foreground">Policies</h2>
            <p className="mt-1 text-[12px] text-muted-foreground">
              schema/policies 约束维护流程如何命名、冲突、废弃和复盘。
            </p>
          </div>
          <span className="rounded bg-muted px-2 py-1 text-[11px] text-muted-foreground">
            {policyFiles.length} files
          </span>
        </div>
        <div className="mt-4 grid gap-2 md:grid-cols-2">
          {policyFiles.map((file) => (
            <PolicyFileCard key={file.id} file={file} />
          ))}
          {policyFiles.length === 0 ? (
            <div className="rounded-md border border-border/50 bg-background px-3 py-3 text-[12px] text-muted-foreground">
              暂无 schema/policies 文件。
            </div>
          ) : null}
        </div>
      </div>

      <div className="rounded-md border border-border/50 bg-card px-4 py-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-[14px] font-medium text-foreground">Rule file editor</h2>
            <p className="mt-1 text-[12px] text-muted-foreground">
              只允许编辑 root guidance、schema guidance、templates 与 policies 白名单文件。
            </p>
          </div>
          <div className="flex items-center gap-2">
            <select
              value={selectedRulesPath}
              onChange={(event) => onSelectedRulesPathChange(event.target.value)}
              disabled={isRulesFileEditing || isRulesFileSaving}
              className="h-8 rounded-md border border-border bg-background px-2 text-[12px] text-foreground"
              aria-label="Select Rules Studio file"
            >
              {ruleFileOptions.map((option) => (
                <option key={option.path} value={option.path}>
                  {option.path}
                </option>
              ))}
            </select>
            {isRulesFileEditing ? (
              <>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onCancelRulesFile}
                  disabled={isRulesFileSaving}
                >
                  <X className="size-3" />
                  Cancel
                </Button>
                <Button
                  variant="default"
                  size="sm"
                  onClick={onSaveRulesFile}
                  disabled={isRulesFileSaving || rulesFileDraft.trim().length === 0}
                >
                  {isRulesFileSaving ? (
                    <Loader2 className="size-3 animate-spin" />
                  ) : (
                    <Save className="size-3" />
                  )}
                  保存
                </Button>
              </>
            ) : (
              <Button
                variant="default"
                size="sm"
                onClick={onEditRulesFile}
                disabled={isRulesFileLoading || !selectedRulesFile}
              >
                <Pencil className="size-3" />
                编辑选中文件
              </Button>
            )}
          </div>
        </div>
        <div className="mt-4 overflow-hidden rounded-md border border-border bg-background">
          <div className="flex items-center justify-between gap-3 border-b border-border/40 px-3 py-2">
            <span className="font-mono text-[11px] text-muted-foreground">
              {selectedRulesFile?.relative_path ?? selectedRulesPath}
            </span>
            <span className="text-[11px] text-muted-foreground">
              {selectedRulesFile ? `${selectedRulesFile.byte_size} bytes` : "loading"}
            </span>
          </div>
          {isRulesFileEditing ? (
            <CodeMirrorEditor
              value={rulesFileDraft}
              onChange={onRulesFileDraftChange}
              language={selectedRulesPath.endsWith(".yml") ? "yaml" : "markdown"}
              minHeight="360px"
              ariaLabel="Rules Studio selected file CodeMirror editor"
            />
          ) : (
            <pre className="max-h-[360px] overflow-auto whitespace-pre-wrap px-4 py-3 font-mono text-[12px] leading-5 text-foreground/90">
              {isRulesFileLoading ? "Loading selected rules file..." : selectedRulesFile?.content}
            </pre>
          )}
        </div>
        {rulesFileSaveError ? (
          <div className="mt-3 rounded-md border border-[var(--color-error)]/30 bg-[var(--color-error)]/5 px-3 py-2 text-[12px] text-[var(--color-error)]">
            {rulesFileSaveError}
          </div>
        ) : null}
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
          <CodeMirrorEditor
            value={draft}
            onChange={onDraftChange}
            language="markdown"
            minHeight="420px"
            ariaLabel="Rules Studio advanced CodeMirror editor"
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

function ValidationSnapshotCard({
  report,
  isLoading,
  error,
  onRunPatrol,
}: {
  report: PatrolReport | null;
  isLoading: boolean;
  error: string | null;
  onRunPatrol: () => void;
}) {
  const summaryItems = report
    ? [
        ["Schema", report.summary.schema_violations],
        ["Orphans", report.summary.orphans],
        ["Stale", report.summary.stale],
        ["Stubs", report.summary.stubs],
        ["Oversized", report.summary.oversized],
        ["Confidence", report.summary.confidence_decay],
      ]
    : [];
  const checkedAt = report?.checked_at
    ? report.checked_at.slice(0, 19).replace("T", " ")
    : null;

  return (
    <div className="rounded-md border border-border/50 bg-card px-4 py-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-[14px] font-medium text-foreground">Validation snapshot</h2>
          <p className="mt-1 text-[12px] text-muted-foreground">
            巡检结果把 Rules 与 Wiki 的健康信号放在同一个默认工作区。
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={onRunPatrol} disabled={isLoading}>
          <RefreshCw className={`size-3 ${isLoading ? "animate-spin" : ""}`} />
          运行巡检
        </Button>
      </div>

      {error ? (
        <div className="mt-3 rounded-md border border-[var(--color-error)]/30 bg-[var(--color-error)]/5 px-3 py-2 text-[12px] text-[var(--color-error)]">
          {error}
        </div>
      ) : null}

      {report ? (
        <>
          <div className="mt-4 grid gap-2 sm:grid-cols-3 lg:grid-cols-6">
            {summaryItems.map(([label, value]) => (
              <div key={label} className="rounded-md border border-border/50 bg-background px-3 py-2">
                <div className="text-[11px] text-muted-foreground">{label}</div>
                <div className="mt-1 text-[16px] font-medium text-foreground">{value}</div>
              </div>
            ))}
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
            <span>{report.issues.length} issues</span>
            {checkedAt ? <span>checked {checkedAt}</span> : null}
          </div>
        </>
      ) : (
        <div className="mt-4 rounded-md border border-dashed border-border/70 bg-background px-3 py-3 text-[12px] text-muted-foreground">
          {isLoading ? "正在读取巡检报告..." : "暂无巡检报告，运行一次巡检即可生成当前规则健康快照。"}
        </div>
      )}
    </div>
  );
}

function TemplateSummaryCard({ template }: { template: SchemaTemplate }) {
  const requiredCount = template.fields.filter((field) => field.required).length;
  const bodyHint = template.body_hint.trim().split(/\r?\n/)[0] || "正文模板";
  return (
    <div className="rounded-md border border-border/50 bg-background px-3 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-[13px] font-medium text-foreground">
            {template.display_name}
          </div>
          <div className="mt-1 font-mono text-[11px] text-muted-foreground">
            schema/templates/{template.category}.md
          </div>
        </div>
        <span className="shrink-0 rounded bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
          {requiredCount} fields
        </span>
      </div>
      <p className="mt-3 line-clamp-2 text-[12px] leading-5 text-muted-foreground">
        {bodyHint}
      </p>
    </div>
  );
}

function GuidanceFileCard({ file }: { file: GuidanceFileInfo }) {
  return (
    <div className="rounded-md border border-border/50 bg-background px-3 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-[13px] font-medium text-foreground">
            {file.label}
          </div>
          <div className="mt-1 font-mono text-[11px] text-muted-foreground">
            {file.relative_path}
          </div>
        </div>
        <span
          className={
            "shrink-0 rounded px-2 py-0.5 text-[11px] " +
            (file.exists
              ? "bg-[var(--color-success)]/10 text-[var(--color-success)]"
              : "bg-[var(--color-warning)]/10 text-[var(--color-warning)]")
          }
        >
          {file.exists ? `${file.byte_size} bytes` : "missing"}
        </span>
      </div>
      <p className="mt-3 truncate text-[12px] text-muted-foreground">
        {file.first_heading ?? "未找到标题"}
      </p>
    </div>
  );
}

function PolicyFileCard({ file }: { file: PolicyFileInfo }) {
  return (
    <div className="rounded-md border border-border/50 bg-background px-3 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-[13px] font-medium text-foreground">
            {file.label}
          </div>
          <div className="mt-1 font-mono text-[11px] text-muted-foreground">
            {file.relative_path}
          </div>
        </div>
        <span className="shrink-0 rounded bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
          {file.byte_size} bytes
        </span>
      </div>
      <p className="mt-3 truncate text-[12px] text-muted-foreground">
        {file.first_heading ?? "未找到标题"}
      </p>
    </div>
  );
}

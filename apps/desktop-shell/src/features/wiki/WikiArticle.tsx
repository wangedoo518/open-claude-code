/**
 * WikiArticle — Markdown rendering area for a single wiki page.
 * Per component-spec.md §3 and 02-wiki-explorer.md §6.2.
 */

import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import {
  CheckCircle2,
  GitBranch,
  Link2,
  Loader2,
  MessageCircleQuestion,
  Pencil,
  X,
} from "lucide-react";

import { getVaultGitStatus, getWikiPage, putWikiPage } from "@/api/wiki/repository";
import type { WikiPageSummary } from "@/api/wiki/types";
import { CodeMirrorEditor } from "@/components/CodeMirrorEditor";
import {
  preprocessWikilinks,
  useWikiLinkRenderer,
} from "./wiki-link-utils";
import { WikiArticleRelationsPanel } from "./WikiArticleRelationsPanel";
import { ConfidenceBadge } from "./components/ConfidenceBadge";
import {
  isValidPurposeLens,
  purposeLensLabel,
} from "@/features/purpose/purpose-lenses";

/* ── Reading time ──────────────────────────────────────────────── */
function estimateReadingMinutes(body: string): number {
  // CJK: 400 chars/min; ASCII: 200 words/min
  let cjkChars = 0;
  for (const ch of body) {
    if (ch.charCodeAt(0) > 0x2e7f) cjkChars++;
  }
  const asciiWords = body
    .split(/\s+/)
    .filter((w) => w.length > 0 && w.charCodeAt(0) <= 127).length;

  return Math.max(1, Math.ceil(cjkChars / 400 + asciiWords / 200));
}

function localizeReadingTime(minutes: number): string {
  if (minutes < 1) return "1 分钟内阅读";
  return `${Math.round(minutes)} 分钟阅读`;
}

/* ── Category badge colors ─────────────────────────────────────── */
const CATEGORY_STYLES: Record<string, string> = {
  concept: "bg-[var(--color-primary)]/10 text-[var(--color-primary)]",
  people: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
  topic: "bg-purple-500/10 text-purple-600 dark:text-purple-400",
  compare: "bg-yellow-500/10 text-yellow-600 dark:text-yellow-400",
};

const ALLOWED_PAGE_TYPES = new Set([
  "concept",
  "evergreen",
  "people",
  "topic",
  "compare",
  "personal",
  "research",
]);

const ALLOWED_PAGE_STATUSES = new Set([
  "active",
  "draft",
  "canonical",
  "stale",
  "deprecated",
  "ingested",
  "archived",
]);

const FRONTMATTER_REF_PATTERN = /^[A-Za-z0-9:_./-]+$/;

function formatShortDate(value?: string | null): string | null {
  if (!value) return null;
  return value.slice(0, 10);
}

function formatVerifiedDate(value?: string | null): string | null {
  const date = formatShortDate(value);
  return date ? `已于 ${date} 验证` : null;
}

function localizePageKind(kind: string): string {
  const map: Record<string, string> = {
    concept: "概念",
    people: "人物",
    topic: "主题",
    compare: "对比",
  };
  return map[kind] ?? kind;
}

function buildEditableMarkdown(summary: WikiPageSummary, body: string): string {
  const purpose = summary.purpose?.length ? summary.purpose : ["learning"];
  const purposeBlock = purpose.map((lens) => `  - ${lens}`).join("\n");
  const expressedIn = summary.expressed_in ?? [];
  const expressedInBlock = expressedIn.length
    ? `expressed_in:\n${expressedIn.map((reference) => `  - ${reference}`).join("\n")}\n`
    : "expressed_in: []\n";
  const sourceRefs = summary.source_refs ?? [];
  const sourceRefsBlock = sourceRefs.length
    ? `source_refs:\n${sourceRefs.map((reference) => `  - ${reference}`).join("\n")}\n`
    : "source_refs: []\n";
  const sourceRaw =
    typeof summary.source_raw_id === "number"
      ? `source_raw_id: ${summary.source_raw_id}\n`
      : "";
  return `---\ntype: ${summary.category ?? "concept"}\nstatus: active\nowner: human\nschema: v1\ntitle: ${summary.title || summary.slug}\nsummary: ${summary.summary ?? ""}\npurpose:\n${purposeBlock}\n${expressedInBlock}${sourceRefsBlock}${sourceRaw}created_at: ${summary.created_at || new Date().toISOString()}\n---\n\n${body}`;
}

interface DraftValidation {
  ok: boolean;
  errors: string[];
  warnings: string[];
}

function stripYamlScalar(value: string): string {
  return value.trim().replace(/^['"]|['"]$/g, "");
}

function parseInlineListValues(raw: string): string[] {
  const trimmed = raw.trim();
  if (!trimmed) return [];
  const withoutBrackets = trimmed.replace(/^\[/, "").replace(/\]$/, "").trim();
  if (!withoutBrackets) return [];
  return withoutBrackets
    .split(",")
    .map(stripYamlScalar)
    .filter(Boolean);
}

function frontmatterScalar(frontmatter: string[], key: string): string | null {
  const prefix = `${key}:`;
  const line = frontmatter.find(
    (item) =>
      !item.startsWith(" ") &&
      !item.startsWith("\t") &&
      item.startsWith(prefix),
  );
  return line ? stripYamlScalar(line.slice(prefix.length)) : null;
}

function collectFrontmatterListValues(
  frontmatter: string[],
  key: string,
): string[] {
  const values: string[] = [];
  const prefix = `${key}:`;
  for (let index = 0; index < frontmatter.length; index += 1) {
    const line = frontmatter[index];
    if (line.startsWith(" ") || line.startsWith("\t") || !line.startsWith(prefix)) {
      continue;
    }
    values.push(...parseInlineListValues(line.slice(prefix.length)));
    for (let next = index + 1; next < frontmatter.length; next += 1) {
      const candidate = frontmatter[next];
      if (!candidate.startsWith(" ") && !candidate.startsWith("\t")) break;
      const item = candidate.trim().replace(/^- /, "").trim();
      if (item) values.push(stripYamlScalar(item));
    }
  }
  return values;
}

function validateReferenceValues(
  field: "expressed_in" | "source_refs",
  values: string[],
  errors: string[],
) {
  for (const value of values) {
    if (!FRONTMATTER_REF_PATTERN.test(value)) {
      errors.push(`${field} 值不可用：${value}`);
    }
  }
}

function validateWikiDraft(content: string): DraftValidation {
  const errors: string[] = [];
  const warnings: string[] = [];
  const lines = content.split(/\r?\n/);
  if (lines[0] !== "---") {
    errors.push("缺少开头 frontmatter 分隔符");
  }
  const closingIndex = lines.findIndex((line, index) => index > 0 && line === "---");
  if (closingIndex < 0) {
    errors.push("缺少结尾 frontmatter 分隔符");
    return { ok: false, errors, warnings };
  }
  const frontmatter = lines.slice(1, closingIndex);
  const required = ["type", "status", "title"];
  for (const key of required) {
    const value = frontmatterScalar(frontmatter, key);
    if (!value) {
      errors.push(`frontmatter 缺少 ${key}`);
    }
  }

  const pageType = frontmatterScalar(frontmatter, "type")?.toLowerCase();
  if (pageType && !ALLOWED_PAGE_TYPES.has(pageType)) {
    errors.push(`type 值不可用：${pageType}`);
  }
  const status = frontmatterScalar(frontmatter, "status")?.toLowerCase();
  if (status && !ALLOWED_PAGE_STATUSES.has(status)) {
    errors.push(`status 值不可用：${status}`);
  }
  const schema = frontmatterScalar(frontmatter, "schema")?.toLowerCase();
  if (schema && schema !== "v1") {
    errors.push(`schema 暂不支持：${schema}`);
  }
  const sourceRawId = frontmatterScalar(frontmatter, "source_raw_id");
  if (sourceRawId && !/^\d+$/.test(sourceRawId)) {
    errors.push(`source_raw_id 必须是数字：${sourceRawId}`);
  }

  const purposeValues = collectFrontmatterListValues(frontmatter, "purpose");
  for (const lens of purposeValues) {
    if (!isValidPurposeLens(lens)) {
      errors.push(`purpose 值不可用：${lens}`);
    }
  }
  validateReferenceValues(
    "expressed_in",
    collectFrontmatterListValues(frontmatter, "expressed_in"),
    errors,
  );
  validateReferenceValues(
    "source_refs",
    collectFrontmatterListValues(frontmatter, "source_refs"),
    errors,
  );
  if (purposeValues.length === 0) {
    warnings.push("建议至少保留一个 purpose lens");
  }
  const body = lines.slice(closingIndex + 1).join("\n").trim();
  if (body.length === 0) {
    warnings.push("正文为空");
  }
  return { ok: errors.length === 0, errors, warnings };
}

function wikiEditGitLabel(
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
  if (hasError) return "状态不可用";
  if (!git) return "检查中";
  if (!git.git_available) return "未安装 Git";
  if (!git.initialized) return "Git 未启用";
  if (git.dirty) return `${git.changed_count} 改动待 checkpoint`;
  return "当前 clean";
}

/* ── Markdown custom components ────────────────────────────────── */
/**
 * Article-page Markdown renderer. The `<a>` handler is shared with
 * the chat-side /query renderer via `useWikiLinkRenderer` — see
 * `wiki-link-utils.tsx` for why and how internal wiki refs are
 * intercepted (short version: raw relative `.md` paths were falling
 * through React Router to `/dashboard`).
 */
function useMarkdownComponents(): Components {
  const Anchor = useWikiLinkRenderer();

  // Heading / body / list / code / blockquote styling is handled by
  // the .markdown-content class on the parent <div>. The ONLY custom
  // component we keep is the wiki-link interceptor — it turns relative
  // .md paths and wiki:// hrefs into tab-store navigations instead of
  // letting the browser fall through to React Router's catch-all.
  return useMemo((): Components => ({ a: Anchor }), [Anchor]);
}

function EditGitFact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 items-center justify-between gap-2 rounded bg-muted/50 px-2 py-1.5">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate text-right text-foreground">{value}</span>
    </div>
  );
}

/* ── Main component ────────────────────────────────────────────── */
interface WikiArticleProps {
  slug: string;
}

export function WikiArticle({ slug }: WikiArticleProps) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const { data, isLoading, error } = useQuery({
    queryKey: ["wiki", "pages", "detail", slug],
    queryFn: () => getWikiPage(slug),
    staleTime: 30_000,
  });
  const gitQuery = useQuery({
    queryKey: ["wiki", "git", "wiki-edit", slug],
    queryFn: () => getVaultGitStatus(),
    staleTime: 10_000,
    refetchInterval: isEditing ? 20_000 : false,
  });

  const components = useMarkdownComponents();
  const saveMutation = useMutation({
    mutationFn: (content: string) => putWikiPage(slug, content),
    onSuccess: async () => {
      setIsEditing(false);
      setSavedAt(Date.now());
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["wiki", "pages", "detail", slug] }),
        queryClient.invalidateQueries({ queryKey: ["wiki", "pages", "list"] }),
        queryClient.invalidateQueries({ queryKey: ["wiki", "git"] }),
      ]);
    },
  });

  const handleAsk = () => {
    const params = new URLSearchParams();
    params.set("bind", `wiki:${slug}`);
    params.set("title", data?.summary.title ?? slug);
    navigate(`/ask?${params.toString()}`);
  };

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center text-[var(--color-muted-foreground)]">
        Loading...
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="flex h-64 items-center justify-center text-[var(--color-destructive)]">
        Failed to load page: {slug}
      </div>
    );
  }

  const { summary, body } = data;
  const validation = validateWikiDraft(draft);
  const category = summary.category ?? "concept";
  const categoryStyle = CATEGORY_STYLES[category] ?? CATEGORY_STYLES.concept;
  const readingTime = localizeReadingTime(estimateReadingMinutes(body));
  const expandedBody = preprocessWikilinks(body);
  const lastVerified = formatVerifiedDate(summary.last_verified);
  const editableMarkdown = data.content ?? buildEditableMarkdown(summary, body);
  const purpose = summary.purpose ?? [];
  const expressedIn = summary.expressed_in ?? [];
  const sourceRefs = summary.source_refs ?? [];

  const handleEdit = () => {
    setDraft(editableMarkdown);
    setIsEditing(true);
    setSavedAt(null);
    saveMutation.reset();
  };

  const handleCancelEdit = () => {
    setDraft(editableMarkdown);
    setIsEditing(false);
    saveMutation.reset();
  };

  const handleSave = () => {
    if (!validation.ok) return;
    saveMutation.mutate(draft);
  };

  if (isEditing) {
    return (
      <div className="mx-auto max-w-[960px] px-8 py-6">
        <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div>
            <h1 className="text-[22px] leading-[1.3] text-[var(--color-foreground)]">
              {summary.title}
            </h1>
            <p className="mt-1 text-[12px] text-muted-foreground">
              正在编辑 wiki/{summary.slug}.md
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleCancelEdit}
              disabled={saveMutation.isPending}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-3 text-[12px] text-muted-foreground transition-colors hover:bg-muted"
            >
              <X className="size-3.5" />
              取消
            </button>
            <button
              type="button"
              onClick={handleSave}
              disabled={!validation.ok || saveMutation.isPending}
              className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-[12px] text-primary-foreground transition-colors disabled:cursor-not-allowed disabled:opacity-50"
            >
              {saveMutation.isPending ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <CheckCircle2 className="size-3.5" />
              )}
              保存
            </button>
          </div>
        </div>

        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_260px]">
          <CodeMirrorEditor
            value={draft}
            onChange={setDraft}
            language="markdown"
            minHeight="620px"
            ariaLabel="Wiki page Markdown and YAML CodeMirror editor"
          />
          <aside className="space-y-3">
            <div className="rounded-md border border-border bg-card px-3 py-3 text-[12px]">
              <div className="mb-2 font-medium text-foreground">保存前检查</div>
              {validation.errors.length === 0 ? (
                <div className="text-[var(--color-success)]">必填字段正常</div>
              ) : (
                <ul className="space-y-1 text-[var(--color-error)]">
                  {validation.errors.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              )}
              {validation.warnings.length > 0 && (
                <ul className="mt-2 space-y-1 text-muted-foreground">
                  {validation.warnings.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              )}
            </div>
            <div className="rounded-md border border-border bg-card px-3 py-3 text-[12px] text-muted-foreground">
              <div className="mb-2 flex items-center gap-1.5 font-medium text-foreground">
                <GitBranch className="size-3.5 text-primary" />
                <span>Git / Lineage</span>
              </div>
              保存会直接写入磁盘，并由后端记录
              <code className="mx-1 rounded bg-muted px-1">human-edit-wiki-page</code>
              日志；Git diff 会出现在 Vault 版本历史里。
              <div className="mt-3 space-y-2">
                <EditGitFact
                  label="Vault diff"
                  value={wikiEditGitLabel(gitQuery.data, Boolean(gitQuery.error))}
                />
                <EditGitFact
                  label="Checkpoint"
                  value={gitQuery.data?.initialized ? "保存后去 Connections 提交" : "建议启用 Git"}
                />
              </div>
            </div>
            {saveMutation.error && (
              <div className="rounded-md border border-[var(--color-error)]/30 bg-[var(--color-error)]/5 px-3 py-2 text-[12px] text-[var(--color-error)]">
                {saveMutation.error instanceof Error
                  ? saveMutation.error.message
                  : String(saveMutation.error)}
              </div>
            )}
          </aside>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[720px] px-8 py-6">
      {/* Title — component-spec.md §3.2 */}
      <h1 className="mb-2 text-[24px] leading-[1.3] text-[var(--color-foreground)]">
        {summary.title}
      </h1>

      {/* Metadata row — component-spec.md §3.3 */}
      <div
        className={`${sourceRefs.length > 0 ? "mb-3" : "mb-6"} flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground`}
      >
        <span className={`rounded px-2 py-0.5 text-[10px] font-medium ${categoryStyle}`}>
          {localizePageKind(category)}
        </span>
        <span>&middot;</span>
        <span>{summary.created_at?.slice(0, 10) ?? "—"}</span>
        <span>&middot;</span>
        <span>{readingTime}</span>
        {summary.confidence != null && (
          <>
            <span>&middot;</span>
            <ConfidenceBadge confidence={summary.confidence} />
          </>
        )}
        {lastVerified && (
          <>
            <span>&middot;</span>
            <span title="由知识维护流程记录的最近验证时间">
              {lastVerified}
            </span>
          </>
        )}
        {purpose.map((lens) => (
          <span
            key={lens}
            className="rounded bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground"
          >
            {purposeLensLabel(lens)}
          </span>
        ))}
        {expressedIn.length ? (
          <span className="rounded bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
            已表达 {expressedIn.length}
          </span>
        ) : null}
        {savedAt && Date.now() - savedAt < 5000 && (
          <>
            <span>&middot;</span>
            <span className="text-[var(--color-success)]">已保存</span>
          </>
        )}
        <button
          type="button"
          onClick={handleAsk}
          className="ml-auto flex items-center gap-1 rounded-md border border-border px-2 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-primary/10 hover:text-primary"
          title="用此页提问"
          aria-label="Ask with this page"
        >
          <MessageCircleQuestion className="size-3" />
          用此页提问
        </button>
        <button
          type="button"
          onClick={handleEdit}
          className="flex items-center gap-1 rounded-md border border-border px-2 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-primary/10 hover:text-primary"
          title="编辑此页"
          aria-label="编辑此页"
        >
          <Pencil className="size-3" />
          编辑
        </button>
      </div>

      {sourceRefs.length > 0 && (
        <div
          className="mb-6 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground"
          aria-label="来源引用"
        >
          <Link2 className="size-3.5 shrink-0 text-primary" />
          <span className="shrink-0 font-medium text-foreground/80">来源</span>
          {sourceRefs.slice(0, 5).map((reference, index) => (
            <span
              key={`${reference}-${index}`}
              className="max-w-[220px] truncate rounded bg-muted px-2 py-0.5 font-mono text-[10px] text-muted-foreground"
              title={reference}
            >
              {reference}
            </span>
          ))}
          {sourceRefs.length > 5 && (
            <span className="rounded bg-muted px-2 py-0.5 text-[10px] font-medium">
              +{sourceRefs.length - 5}
            </span>
          )}
        </div>
      )}

      {/* Summary */}
      {summary.summary && (
        <p className="mb-6 text-[14px] italic text-[var(--color-muted-foreground)]">
          {summary.summary}
        </p>
      )}

      {/* Markdown body — component-spec.md §3.4 */}
      <div className="wiki-article-body markdown-content">
        <ReactMarkdown components={components}>{expandedBody}</ReactMarkdown>
      </div>

      {/* Relations (outgoing / backlinks / related) — G1 sprint.
          Replaces the legacy single-list BacklinksSection. */}
      <WikiArticleRelationsPanel slug={slug} />
    </div>
  );
}

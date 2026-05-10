import { useCallback, useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { ArrowLeft, Check, Loader2, Save, Send } from "lucide-react";
import {
  DRAFT_TARGETS,
  exportDraft,
  getDraft,
  putDraft,
  setDraftSources,
  type DraftTarget,
} from "@/api/wiki/repository";
import { CodeMirrorEditor } from "@/components/CodeMirrorEditor";
import { DraftSourcePicker } from "./DraftSourcePicker";
import { labelForTarget } from "./DraftsPage";

/**
 * Slice E20.1 — DraftEditor.
 *
 * Title + target picker + CodeMirror markdown body. Save is manual
 * (no autosave) — explicit "保存" button so users have a clear "I
 * committed this revision" mental model. Source picker + export
 * button arrive in E20.2.
 */
export function DraftEditor({ slug }: { slug: string }) {
  const queryClient = useQueryClient();
  const draftQuery = useQuery({
    queryKey: ["wiki", "drafts", "detail", slug],
    queryFn: () => getDraft(slug),
  });

  const [title, setTitle] = useState("");
  const [target, setTarget] = useState<DraftTarget>("xhs");
  const [body, setBody] = useState("");
  const [sources, setSources] = useState<string[]>([]);
  const [dirty, setDirty] = useState(false);
  const [hydratedFor, setHydratedFor] = useState<string | null>(null);

  // Review I4 (E20.1) — reset hydration state immediately on slug
  // change so a quick navigation A → B doesn't leave slug A's
  // title/body/sources visible while slug B's fetch is in flight.
  useEffect(() => {
    setHydratedFor(null);
    setDirty(false);
    setTitle("");
    setBody("");
    setSources([]);
  }, [slug]);

  // Hydrate local state from the server snapshot ONCE per slug —
  // subsequent refetches (window-focus, polling) don't clobber
  // in-progress edits. Combined with the slug-reset effect above,
  // hydratedFor is null right after slug change → next data
  // arrival hydrates → set to slug → subsequent refetches no-op.
  useEffect(() => {
    if (draftQuery.data && hydratedFor !== slug) {
      setTitle(draftQuery.data.summary.title);
      setTarget((draftQuery.data.summary.target as DraftTarget) || "other");
      setBody(draftQuery.data.body);
      setSources(draftQuery.data.summary.source_pages);
      setDirty(false);
      setHydratedFor(slug);
    }
  }, [draftQuery.data, slug, hydratedFor]);

  const onTitleChange = useCallback((v: string) => {
    setTitle(v);
    setDirty(true);
  }, []);
  const onTargetChange = useCallback((v: DraftTarget) => {
    setTarget(v);
    setDirty(true);
  }, []);
  const onBodyChange = useCallback((v: string) => {
    setBody(v);
    setDirty(true);
  }, []);

  const saveMutation = useMutation({
    mutationFn: () => putDraft(slug, { title, target, body }),
    onSuccess: async () => {
      setDirty(false);
      await queryClient.invalidateQueries({
        queryKey: ["wiki", "drafts", "detail", slug],
      });
      await queryClient.invalidateQueries({
        queryKey: ["wiki", "drafts", "list"],
      });
    },
  });

  const sourcesMutation = useMutation({
    mutationFn: (next: string[]) => setDraftSources(slug, next),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["wiki", "drafts", "detail", slug],
      });
    },
  });

  const exportMutation = useMutation({
    mutationFn: () => exportDraft(slug),
    onSuccess: async (data) => {
      // Copy current body to clipboard only on FULL success — if
      // some sources were skipped, the user probably wants to fix
      // the missing ones before publishing. Don't poison the
      // clipboard with content the partial-export user might not
      // want yet. Best-effort: clipboard permission may be denied
      // in non-secure contexts (file://); the server-side write
      // still succeeded so we don't fail the mutation either way.
      if (data.skipped.length === 0) {
        try {
          await navigator.clipboard.writeText(body);
        } catch {
          // ignore — server write done, just no clipboard copy
        }
      }
      // Invalidate both drafts (for the exported_at stamp) and
      // wiki pages (so RoiPulsePanel sees the new expressed_in
      // entries).
      await queryClient.invalidateQueries({ queryKey: ["wiki", "drafts"] });
      await queryClient.invalidateQueries({ queryKey: ["wiki", "pages"] });
    },
  });

  const onSourcesChange = (next: string[]) => {
    setSources(next);
    sourcesMutation.mutate(next);
  };

  if (draftQuery.isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-muted-foreground">
        <Loader2 className="mr-2 size-4 animate-spin" />
        加载草稿…
      </div>
    );
  }
  if (draftQuery.isError) {
    return (
      <div className="p-6 text-[13px] text-destructive">
        加载失败：{(draftQuery.error as Error).message}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 p-6">
      <header className="flex items-center gap-3">
        <Link
          to="/drafts"
          className="inline-flex items-center gap-1 text-[12px] text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="size-3.5" />
          返回列表
        </Link>
        <input
          type="text"
          value={title}
          onChange={(e) => onTitleChange(e.target.value)}
          placeholder="草稿标题"
          className="min-w-0 flex-1 rounded-md border border-transparent bg-transparent px-2 py-1 text-[18px] font-medium hover:border-border focus:border-border focus:outline-none"
        />
        <select
          value={target}
          onChange={(e) => onTargetChange(e.target.value as DraftTarget)}
          className="rounded-md border border-border bg-background px-2 py-1 text-[12px]"
        >
          {DRAFT_TARGETS.map((t) => (
            <option key={t} value={t}>
              {labelForTarget(t)}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
          onClick={() => saveMutation.mutate()}
          disabled={saveMutation.isPending || !dirty || !title.trim()}
          title={dirty ? "保存修改" : "已保存"}
        >
          <Save className="size-3.5" />
          {saveMutation.isPending ? "保存中…" : dirty ? "保存" : "已保存"}
        </button>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-primary bg-card px-3 py-1.5 text-[13px] text-primary hover:bg-primary/5 disabled:opacity-50"
          onClick={() => exportMutation.mutate()}
          disabled={
            exportMutation.isPending || dirty || sources.length === 0
          }
          title={
            dirty
              ? "请先保存"
              : sources.length === 0
                ? "请先 pin 来源页面"
                : "复制正文到剪贴板 + 把这条草稿写进每个来源的 expressed_in"
          }
        >
          {exportMutation.isSuccess &&
          !exportMutation.isPending &&
          (exportMutation.data?.skipped.length ?? 0) === 0 ? (
            <Check className="size-3.5" />
          ) : (
            <Send className="size-3.5" />
          )}
          {exportMutation.isPending
            ? "导出中…"
            : exportMutation.isSuccess
              ? exportMutation.data && exportMutation.data.skipped.length > 0
                ? `部分导出 ${exportMutation.data.written}/${exportMutation.data.source_count}`
                : "已复制"
              : "导出"}
        </button>
      </header>

      {saveMutation.error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
          保存失败：{(saveMutation.error as Error).message}
        </div>
      ) : null}
      {sourcesMutation.error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
          来源更新失败：{(sourcesMutation.error as Error).message}
        </div>
      ) : null}
      {exportMutation.error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
          导出失败：{(exportMutation.error as Error).message}
        </div>
      ) : null}
      {exportMutation.isSuccess &&
      exportMutation.data &&
      exportMutation.data.skipped.length > 0 ? (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-700 dark:text-amber-400">
          部分来源未能写入 expressed_in（共 {exportMutation.data.source_count} 个，
          {exportMutation.data.written} 个成功）：
          <span className="ml-1 font-mono">
            {exportMutation.data.skipped.join("、")}
          </span>
          。请检查这些 wiki 页面是否仍存在，调整后再次导出。
        </div>
      ) : null}

      <div className="grid min-h-0 flex-1 gap-3 overflow-hidden md:grid-cols-[260px_1fr]">
        <div className="min-h-0 overflow-hidden">
          <DraftSourcePicker selected={sources} onChange={onSourcesChange} />
        </div>
        <div className="min-h-0 overflow-hidden rounded-lg border border-border">
          <CodeMirrorEditor
            value={body}
            onChange={onBodyChange}
            language="markdown"
            minHeight="100%"
            ariaLabel="草稿正文"
          />
        </div>
      </div>
    </div>
  );
}

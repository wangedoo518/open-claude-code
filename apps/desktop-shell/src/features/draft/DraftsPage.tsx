import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { ArrowRight, FileText, Plus } from "lucide-react";
import {
  DRAFT_TARGETS,
  createDraft,
  listDrafts,
  type DraftTarget,
} from "@/api/wiki/repository";
import type { DraftSummary } from "@/api/wiki/types";

/**
 * Slice E20.1 — Drafts list page.
 *
 * Buddy positioning: closes the 搜集→整理→表达 loop. Wiki pages are
 * knowledge crystals; drafts are what users actually publish. Each
 * row links to the editor at /drafts/<slug>.
 *
 * Strict no-input on rows themselves — the only mutation surface is
 * the "新建草稿" form gated behind a toggle. Edits happen in the
 * editor (E20.1 Task 7).
 */
export function DraftsPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const draftsQuery = useQuery({
    queryKey: ["wiki", "drafts", "list"],
    queryFn: () => listDrafts(),
  });
  const [showNewForm, setShowNewForm] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newTarget, setNewTarget] = useState<DraftTarget>("xhs");

  const createMutation = useMutation({
    mutationFn: () => createDraft({ title: newTitle, target: newTarget }),
    onSuccess: async ({ slug }) => {
      await queryClient.invalidateQueries({ queryKey: ["wiki", "drafts"] });
      setShowNewForm(false);
      setNewTitle("");
      navigate(`/drafts/${slug}`);
    },
  });

  const drafts: ReadonlyArray<DraftSummary> = draftsQuery.data?.drafts ?? [];

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex items-center gap-2">
        <FileText className="size-5 text-primary" />
        <h1 className="text-xl font-medium">草稿</h1>
        <span className="text-[12px] text-muted-foreground">
          ({drafts.length})
        </span>
        <button
          type="button"
          className="ml-auto inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-[13px] hover:bg-muted/50"
          onClick={() => setShowNewForm((v) => !v)}
        >
          <Plus className="size-3.5" />
          新建草稿
        </button>
      </header>

      <p className="max-w-2xl text-[12px] leading-5 text-muted-foreground">
        草稿是从知识库派生出来的可发布内容（小红书、博客、微信、长文）。
        编辑器里 pin 几个 wiki 页面作为来源，导出后会自动把这条草稿写进
        每个来源页面的 <code>expressed_in</code>，让"投入回报"面板第一次
        看见真实的表达数据。
      </p>

      {showNewForm && (
        <form
          className="rounded-lg border border-border bg-card p-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (newTitle.trim()) createMutation.mutate();
          }}
        >
          <div className="flex flex-wrap items-center gap-3">
            <input
              autoFocus
              type="text"
              placeholder="草稿标题"
              value={newTitle}
              onChange={(e) => setNewTitle(e.target.value)}
              className="min-w-[200px] flex-1 rounded-md border border-border bg-background px-3 py-1.5 text-[13px]"
            />
            <select
              value={newTarget}
              onChange={(e) => setNewTarget(e.target.value as DraftTarget)}
              className="rounded-md border border-border bg-background px-2 py-1.5 text-[13px]"
            >
              {DRAFT_TARGETS.map((t) => (
                <option key={t} value={t}>
                  {labelForTarget(t)}
                </option>
              ))}
            </select>
            <button
              type="submit"
              className="rounded-md bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
              disabled={createMutation.isPending || !newTitle.trim()}
            >
              {createMutation.isPending ? "创建中…" : "创建"}
            </button>
          </div>
          {createMutation.error ? (
            <div className="mt-2 text-[12px] text-destructive">
              创建失败：{(createMutation.error as Error).message}
            </div>
          ) : null}
        </form>
      )}

      <div className="flex-1 overflow-y-auto">
        {draftsQuery.isLoading ? (
          <div className="text-[12px] text-muted-foreground">加载中…</div>
        ) : null}
        {draftsQuery.isSuccess && drafts.length === 0 && !showNewForm ? (
          <div className="rounded-lg border border-dashed border-border p-8 text-center text-[13px] text-muted-foreground">
            还没有草稿。点右上角「新建草稿」开始。
          </div>
        ) : null}
        <ul className="grid gap-2">
          {drafts.map((d) => (
            <DraftRow key={d.slug} draft={d} />
          ))}
        </ul>
      </div>
    </div>
  );
}

function DraftRow({ draft }: { draft: DraftSummary }) {
  return (
    <li>
      <Link
        to={`/drafts/${draft.slug}`}
        className="flex items-center gap-3 rounded-md border border-border bg-card px-3 py-2.5 text-[13px] transition-colors hover:border-primary/40 hover:bg-muted/30"
      >
        <span className="min-w-0 flex-1 truncate font-medium">{draft.title}</span>
        <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">
          {labelForTarget(draft.target)}
        </span>
        {draft.exported_at ? (
          <span className="shrink-0 text-[10px] text-primary">
            已导出 {formatRelative(draft.exported_at)}
          </span>
        ) : (
          <span className="shrink-0 text-[10px] text-muted-foreground">
            草稿
          </span>
        )}
        <ArrowRight className="size-3.5 shrink-0 text-muted-foreground" />
      </Link>
    </li>
  );
}

export function labelForTarget(t: string): string {
  switch (t) {
    case "xhs":
      return "小红书";
    case "blog":
      return "博客";
    case "wechat":
      return "微信";
    case "other":
      return "其它";
    default:
      return t;
  }
}

function formatRelative(iso: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "";
  const diff = Date.now() - t;
  const m = Math.round(diff / 60_000);
  if (m < 1) return "刚刚";
  if (m < 60) return `${m} 分钟前`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} 小时前`;
  return `${Math.round(h / 24)} 天前`;
}

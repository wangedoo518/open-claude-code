/**
 * SkillProgressCard — floating progress card for absorb operations.
 * Per component-spec.md §5.
 */

import { CheckCircle2, Loader2, XCircle } from "lucide-react";
import { useSkillStore } from "@/state/skill-store";

export function SkillProgressCard() {
  const running = useSkillStore((s) => s.absorbRunning);
  const progress = useSkillStore((s) => s.absorbProgress);
  const result = useSkillStore((s) => s.absorbResult);
  const error = useSkillStore((s) => s.absorbError);
  const reset = useSkillStore((s) => s.resetAbsorb);

  // Don't render if nothing to show.
  if (!running && !result && !error) return null;

  const percent =
    progress && progress.total > 0
      ? Math.round((progress.processed / progress.total) * 100)
      : 0;

  return (
    <div
      className="mx-4 mt-3 rounded-xl border border-[var(--color-border)] p-3 shadow-sm"
      style={{
        background: "color-mix(in srgb, var(--color-background) 90%, transparent)",
        backdropFilter: "blur(12px) saturate(1.4)",
      }}
    >
      {/* ── Running state ────────────────────────────────────── */}
      {running && (
        <>
          <div className="mb-2 flex items-center justify-between text-[12px] text-[var(--color-muted-foreground)]">
            <span className="flex items-center gap-1.5">
              <Loader2 className="size-3.5 animate-spin text-[var(--color-primary)]" />
              正在维护...
            </span>
            {progress && (
              <span>
                {progress.processed}/{progress.total}
              </span>
            )}
          </div>

          {/* Progress bar — component-spec.md §5.2 */}
          <div className="h-1 w-full overflow-hidden rounded-full bg-[var(--color-muted)]">
            <div
              className="h-full rounded-full bg-[var(--color-primary)] transition-[width] duration-300 ease-out"
              style={{ width: `${percent}%` }}
            />
          </div>

          {/* Current action */}
          {progress?.page_slug && (
            <div className="mt-1.5 truncate text-[11px] text-[var(--color-muted-foreground)]">
              {progress.action === "create" && "创建 "}
              {progress.action === "update" && "更新 "}
              {progress.action === "skip" && "跳过 "}
              {progress.page_slug}
            </div>
          )}
        </>
      )}

      {/* ── Completed state ──────────────────────────────────── */}
      {!running && result && (
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1.5 text-[12px] text-[var(--deeptutor-ok,#3F8F5E)]">
            <CheckCircle2 className="size-3.5" />
            维护完成
          </span>
          <span className="text-[11px] text-[var(--color-muted-foreground)]">
            新增 {result.created} 页 · 更新 {result.updated} 页
            {result.skipped > 0 && ` · 跳过 ${result.skipped}`}
          </span>
          <button
            onClick={reset}
            className="text-[11px] text-[var(--color-muted-foreground)] hover:text-[var(--color-foreground)]"
          >
            关闭
          </button>
        </div>
      )}

      {/* ── Error state ──────────────────────────────────────── */}
      {!running && error && (
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1.5 text-[12px] text-[var(--color-destructive)]">
            <XCircle className="size-3.5" />
            维护失败
          </span>
          <span className="max-w-[60%] truncate text-[11px] text-[var(--color-muted-foreground)]">
            {error}
          </span>
          <button
            onClick={reset}
            className="text-[11px] text-[var(--color-muted-foreground)] hover:text-[var(--color-foreground)]"
          >
            关闭
          </button>
        </div>
      )}
    </div>
  );
}

/**
 * SkillProgressCard - floating progress card for absorb operations.
 */

import { useEffect } from "react";
import { CheckCircle2, Loader2, XCircle } from "lucide-react";
import { useSkillStore } from "@/state/skill-store";

export function SkillProgressCard({
  placement = "default",
}: {
  placement?: "default" | "bottom-toast";
}) {
  const running = useSkillStore((s) => s.absorbRunning);
  const progress = useSkillStore((s) => s.absorbProgress);
  const result = useSkillStore((s) => s.absorbResult);
  const error = useSkillStore((s) => s.absorbError);
  const reset = useSkillStore((s) => s.resetAbsorb);

  useEffect(() => {
    if (placement !== "bottom-toast" || running || !error) return;
    const timer = window.setTimeout(reset, 6_000);
    return () => window.clearTimeout(timer);
  }, [error, placement, reset, running]);

  if (!running && !result && !error) return null;

  const percent =
    progress && progress.total > 0
      ? Math.round((progress.processed / progress.total) * 100)
      : 0;
  const currentLabel = progress?.page_title ?? progress?.page_slug ?? null;

  return (
    <div
      role="status"
      className={`ds-skill-progress-popover ${
        placement === "bottom-toast" ? "ds-skill-progress-popover--bottom" : ""
      } rounded-xl border border-[var(--color-border)] p-3 shadow-sm`}
      style={{
        background: "color-mix(in srgb, var(--color-background) 90%, transparent)",
        backdropFilter: "blur(12px) saturate(1.4)",
      }}
    >
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

          <div className="h-1 w-full overflow-hidden rounded-full bg-[var(--color-muted)]">
            <div
              className="h-full rounded-full bg-[var(--color-primary)] transition-[width] duration-300 ease-out"
              style={{ width: `${percent}%` }}
            />
          </div>

          {currentLabel && (
            <div className="mt-1.5 truncate text-[11px] text-[var(--color-muted-foreground)]">
              {progress?.action === "create" && "创建 "}
              {progress?.action === "update" && "更新 "}
              {progress?.action === "skip" && "跳过 "}
              {currentLabel}
            </div>
          )}
          {progress?.error && (
            <div className="mt-1.5 truncate text-[11px] text-[var(--color-destructive)]">
              {progress.error}
            </div>
          )}
        </>
      )}

      {!running && result && (
        <div className="flex items-center justify-between gap-3">
          <span className="flex items-center gap-1.5 text-[12px] text-[var(--deeptutor-ok,#3F8F5E)]">
            <CheckCircle2 className="size-3.5" />
            维护完成
          </span>
          <span className="min-w-0 flex-1 truncate text-right text-[11px] text-[var(--color-muted-foreground)]">
            新增 {result.created} 页 · 更新 {result.updated} 页
            {result.skipped > 0 && ` · 跳过 ${result.skipped}`}
            {result.failed > 0 && ` · 失败 ${result.failed}`}
          </span>
          <button
            onClick={reset}
            className="text-[11px] text-[var(--color-muted-foreground)] hover:text-[var(--color-foreground)]"
          >
            关闭
          </button>
        </div>
      )}

      {!running && error && (
        <div className="flex items-center justify-between gap-3">
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

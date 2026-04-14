/**
 * AbsorbTriggerButton — triggers /absorb and polls for completion.
 * Per 01-skill-engine.md §6.2.
 */

import { useCallback, useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2, Sparkles } from "lucide-react";
import { toast } from "sonner";

import { triggerAbsorb, getWikiStats } from "@/features/ingest/persist";
import { useSkillStore } from "@/state/skill-store";

interface AbsorbTriggerButtonProps {
  /** Specific entry IDs to absorb. Omit for "absorb all pending". */
  entryIds?: number[];
  /** Compact mode (smaller text, inline). */
  compact?: boolean;
}

export function AbsorbTriggerButton({ entryIds, compact }: AbsorbTriggerButtonProps) {
  const queryClient = useQueryClient();
  const running = useSkillStore((s) => s.absorbRunning);
  const startAbsorb = useSkillStore((s) => s.startAbsorb);
  const completeAbsorb = useSkillStore((s) => s.completeAbsorb);
  const failAbsorb = useSkillStore((s) => s.failAbsorb);

  // Track the last_absorb_at timestamp for polling.
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const baselineRef = useRef<string | null>(null);

  // Clean up polling on unmount.
  useEffect(() => {
    return () => {
      if (pollingRef.current) clearInterval(pollingRef.current);
    };
  }, []);

  const handleClick = useCallback(async () => {
    if (running) return;

    try {
      // Capture baseline timestamp before triggering.
      const statsBefore = await getWikiStats().catch(() => null);
      baselineRef.current = statsBefore?.last_absorb_at ?? null;

      // Trigger absorb.
      const response = await triggerAbsorb(entryIds);
      startAbsorb(response.task_id);

      // Start polling every 3 seconds for completion.
      pollingRef.current = setInterval(async () => {
        try {
          const stats = await getWikiStats();
          const newTimestamp = stats.last_absorb_at;

          // If last_absorb_at has changed → absorb completed.
          if (newTimestamp && newTimestamp !== baselineRef.current) {
            // Stop polling.
            if (pollingRef.current) {
              clearInterval(pollingRef.current);
              pollingRef.current = null;
            }

            completeAbsorb({
              created: 0, // We don't have exact counts from stats alone.
              updated: 0,
              skipped: 0,
              failed: 0,
              duration_ms: 0,
            });

            // Invalidate all wiki-related queries.
            queryClient.invalidateQueries({ queryKey: ["wiki"] });
            queryClient.invalidateQueries({ queryKey: ["wiki-tree"] });

            toast.success("维护完成", { duration: 3000 });
          }
        } catch {
          // Polling failure is non-fatal; keep trying.
        }
      }, 3000);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("ABSORB_IN_PROGRESS")) {
        toast.warning("已有维护任务正在执行", { duration: 3000 });
      } else {
        failAbsorb(msg);
        toast.error(`维护启动失败: ${msg}`, { duration: 5000 });
      }
    }
  }, [running, entryIds, startAbsorb, completeAbsorb, failAbsorb, queryClient]);

  if (compact) {
    return (
      <button
        onClick={handleClick}
        disabled={running}
        className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-[var(--color-primary)] hover:bg-[var(--color-primary)]/10 disabled:cursor-not-allowed disabled:opacity-50 transition-colors"
        title={running ? "维护中..." : "开始维护"}
      >
        {running ? (
          <Loader2 className="size-3 animate-spin" />
        ) : (
          <Sparkles className="size-3" />
        )}
      </button>
    );
  }

  return (
    <button
      onClick={handleClick}
      disabled={running}
      className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] font-medium text-[var(--color-primary)] hover:bg-[var(--color-primary)]/10 disabled:cursor-not-allowed disabled:opacity-50 transition-colors"
    >
      {running ? (
        <>
          <Loader2 className="size-3.5 animate-spin" />
          维护中...
        </>
      ) : (
        <>
          <Sparkles className="size-3.5" />
          开始维护
        </>
      )}
    </button>
  );
}

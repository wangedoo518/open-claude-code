/**
 * A1 sprint — compact chip that surfaces the currently-detected (or
 * user-overridden) context mode in the Composer's bottom mode bar.
 *
 * Display spec (Explorer C):
 *   - follow_up    → grey   "继续前文"        (no icon)
 *   - source_first → primary "源优先" + Link2 icon
 *   - combine      → orange  "结合"   + Shuffle icon
 *
 * When `onChange` is supplied, clicking the chip opens a 3-item
 * dropdown so the user can explicitly override the detected mode.
 * When omitted, the chip is a pure display affordance.
 *
 * Deliberately small (no shadcn `DropdownMenu` — we reuse the
 * outside-click pattern from Composer.tsx to avoid over-engineering
 * a 3-option picker).
 */

import { useEffect, useRef, useState } from "react";
import { Link2, Shuffle } from "lucide-react";
import type { ContextMode } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { InfoTooltip } from "@/components/ui/info-tooltip";

interface ResponseModeChipProps {
  mode: ContextMode;
  confidence?: "high" | "medium" | "low";
  onChange?: (next: ContextMode) => void;
  /**
   * R1 trust layer — when true (default), render an `InfoTooltip`
   * next to the chip explaining the 3-mode system. Disable only for
   * compact layouts where the extra affordance is noise.
   */
  showHelpIcon?: boolean;
}

interface ModeMeta {
  label: string;
  /** R1 trust layer — one-line description surfaced under the label
   *  in the dropdown so users who don't know what "源优先" means
   *  don't have to guess. Kept to ≤14 Chinese characters so it fits
   *  the narrow popover without wrapping awkwardly. */
  description: string;
  className: string;
  icon: React.ComponentType<{ className?: string }> | null;
}

function metaFor(mode: ContextMode): ModeMeta {
  switch (mode) {
    case "source_first":
      return {
        label: "源优先",
        description: "优先基于新来源或链接",
        className: "bg-primary/10 text-primary border-primary/30",
        icon: Link2,
      };
    case "combine":
      return {
        label: "结合",
        description: "综合历史与新素材",
        className:
          "bg-[color:var(--color-warning)]/10 text-[color:var(--color-warning)] border-[color:var(--color-warning)]/30",
        icon: Shuffle,
      };
    case "follow_up":
    default:
      return {
        label: "继续前文",
        description: "沿用对话历史回答",
        className: "bg-muted/40 text-muted-foreground border-border/50",
        icon: null,
      };
  }
}

const MODES: ContextMode[] = ["follow_up", "source_first", "combine"];

export function ResponseModeChip({
  mode,
  confidence,
  onChange,
  showHelpIcon = true,
}: ResponseModeChipProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const meta = metaFor(mode);
  const Icon = meta.icon;
  const clickable = Boolean(onChange);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const title = confidence
    ? `上下文模式：${meta.label}（置信度：${confidence}）`
    : `上下文模式：${meta.label}`;

  return (
    <div className="inline-flex items-center gap-1">
      <div className="relative" ref={ref}>
        <button
          type="button"
          onClick={() => clickable && setOpen((v) => !v)}
          title={title}
          aria-label={title}
          disabled={!clickable}
          className={cn(
            "inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] transition-colors",
            meta.className,
            clickable && "cursor-pointer hover:brightness-105",
            !clickable && "cursor-default",
          )}
        >
          {Icon && <Icon className="size-3" />}
          <span>{meta.label}</span>
        </button>

        {open && clickable && (
          <div className="absolute bottom-full left-0 z-50 mb-1 min-w-[200px] rounded-lg border border-border bg-popover p-1 shadow-lg">
            <div className="px-2 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              上下文模式
            </div>
            {MODES.map((m) => {
              const mMeta = metaFor(m);
              const MIcon = mMeta.icon;
              const isActive = m === mode;
              return (
                <button
                  key={m}
                  type="button"
                  className={cn(
                    "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
                    isActive
                      ? "bg-accent text-foreground"
                      : "text-foreground hover:bg-accent/50",
                  )}
                  onClick={() => {
                    onChange?.(m);
                    setOpen(false);
                  }}
                >
                  {MIcon ? (
                    <MIcon className="mt-0.5 size-3 shrink-0" />
                  ) : (
                    <span className="mt-0.5 inline-block size-3 shrink-0" />
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="text-[11px] font-medium">
                        {mMeta.label}
                      </span>
                      {isActive && (
                        <span className="size-1.5 shrink-0 rounded-full bg-primary" />
                      )}
                    </div>
                    <div className="mt-0.5 text-[10px] text-muted-foreground/70">
                      {mMeta.description}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>
      {showHelpIcon && (
        <InfoTooltip side="top">
          <div className="space-y-1">
            <div className="font-medium text-background">3 种上下文模式</div>
            <div className="leading-relaxed">
              Ask 每轮会自动选一种模式：
              <br />
              <span className="text-background/90">· 继续前文</span>：沿用对话历史回答
              <br />
              <span className="text-background/90">· 源优先</span>：优先基于新来源或链接
              <br />
              <span className="text-background/90">· 结合</span>：综合历史与新素材
            </div>
          </div>
        </InfoTooltip>
      )}
    </div>
  );
}

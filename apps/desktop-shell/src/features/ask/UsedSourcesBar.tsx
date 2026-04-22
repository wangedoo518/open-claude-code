/**
 * A2 / A3 sprint — "Used sources" strip that renders under an assistant
 * message, directly below the ContextBasisLabel. Citation surface that
 * tells the user *which* concrete sources the backend used for this turn.
 *
 * Current scope:
 *   - A2 (manual session binding): `basis.bound_source` present AND
 *     `basis.auto_bound` is false/absent. Renders orange-toned "引用:"
 *     badge — matches the persistent session binding chip tone in the
 *     Composer.
 *   - A3 (turn-local auto binding): `basis.bound_source` present AND
 *     `basis.auto_bound === true`. Renders blue-toned "本轮自动锁定:"
 *     badge with a Lucide <Link> icon and an inline "固定到会话"
 *     action (Lucide <Pin>) that promotes the auto-bound turn source
 *     to a persistent session binding via `onPromoteToSession`.
 *     DS1.5 — icons are Lucide only; zero emoji in product copy.
 *   - Falls back to a generic "本链接" badge when `source_included` is
 *     true but no discrete `bound_source` was resolved (A1-era behavior
 *     for a URL-enrich turn without an explicit ref).
 *   - Returns null when `basis` is null/undefined or `source_included`
 *     isn't true and no bound_source exists.
 */

import { Hash, BookOpen, Inbox, Link2, Link, Pin, ShieldCheck } from "lucide-react";
import type { ContextBasis, SourceRef } from "@/lib/tauri";
import { formatSourceRefLabel, sourceRefKey } from "@/lib/tauri";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

/**
 * A4 sprint — "Grounded" badge (with leading check icon). Rendered next to the source chip in
 * UsedSourcesBar when `basis.grounding_applied === true`. The tone
 * (auto blue / manual orange) matches the parent branch via
 * `toneClassName` so the badge reads as part of the same strip.
 *
 * R1 trust layer: the tooltip copy was upgraded from a one-liner to
 * a three-line explanation so users can see the concrete behavior
 * guarantees that Grounded mode provides (quoted block, fallback
 * phrasing when sources are insufficient).
 */
function GroundedBadge({ toneClassName }: { toneClassName?: string }) {
  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge
            variant="outline"
            className={cn(
              "gap-1 text-[10px] font-normal",
              toneClassName,
            )}
          >
            <ShieldCheck className="size-3 shrink-0" />
            <span>Grounded</span>
          </Badge>
        </TooltipTrigger>
        <TooltipContent side="top" align="start" className="max-w-xs">
          <div className="space-y-1 leading-snug">
            <div className="font-medium">Grounded 模式生效</div>
            <div className="text-background/90">
              · 回答严格基于绑定来源
              <br />
              · 至少一次原文 blockquote 引用
              <br />
              · 来源不足时明确说「未提及」
            </div>
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

interface UsedSourcesBarProps {
  basis: ContextBasis | null | undefined;
  /**
   * A3 — callback when user clicks the Lucide <Pin> button to upgrade
   * an auto-bound turn source to a persistent session binding.
   * Called with the current bound_source. Parent wires this to
   * `bindSourceToSession`.
   */
  onPromoteToSession?: (source: SourceRef) => void | Promise<void>;
  className?: string;
}

function iconFor(
  source: SourceRef,
): React.ComponentType<{ className?: string }> {
  switch (source.kind) {
    case "raw":
      return Hash;
    case "wiki":
      return BookOpen;
    case "inbox":
      return Inbox;
  }
}

export function UsedSourcesBar({
  basis,
  onPromoteToSession,
  className,
}: UsedSourcesBarProps) {
  if (!basis) return null;

  const boundSource = basis.bound_source ?? null;
  const isAutoBound = boundSource != null && basis.auto_bound === true;

  // A1 compatibility branch: source_included=true but no discrete ref
  // means the backend ingested a URL inline. Render a generic badge so
  // the user still sees "a source was used" without an explicit ref.
  const showGenericLinkBadge =
    boundSource === null && basis.source_included === true;

  if (boundSource === null && !showGenericLinkBadge) return null;

  if (isAutoBound && boundSource) {
    // A3 — blue tone, link icon, "本轮自动锁定" prefix, inline pin action.
    const Icon = iconFor(boundSource);
    const label = formatSourceRefLabel(boundSource);
    return (
      <div
        className={cn(
          "mb-1 flex flex-wrap items-center gap-1 text-[11px] text-muted-foreground",
          className,
        )}
      >
        <Link className="size-3 shrink-0 text-[color:var(--claude-blue,rgb(87,105,247))]" />
        <span className="mr-1 shrink-0 text-[color:var(--claude-blue,rgb(87,105,247))]/80">
          本轮自动锁定：
        </span>
        <Badge
          key={sourceRefKey(boundSource)}
          variant="outline"
          className="max-w-[260px] gap-1 border-[color:var(--claude-blue,rgb(87,105,247))]/30 bg-[color:var(--claude-blue,rgb(87,105,247))]/10 text-[color:var(--claude-blue,rgb(87,105,247))] text-[11px] font-normal"
          title={label}
        >
          <Icon className="size-3 shrink-0" />
          <span className="truncate" dir="ltr">
            {label}
          </span>
        </Badge>
        {basis.grounding_applied === true && (
          <GroundedBadge toneClassName="border-[color:var(--claude-blue,rgb(87,105,247))]/30 bg-[color:var(--claude-blue,rgb(87,105,247))]/5 text-[color:var(--claude-blue,rgb(87,105,247))]" />
        )}
        {onPromoteToSession && (
          <button
            type="button"
            onClick={() => void onPromoteToSession(boundSource)}
            className="ml-1 inline-flex items-center gap-0.5 rounded-md border border-[color:var(--claude-blue,rgb(87,105,247))]/30 bg-[color:var(--claude-blue,rgb(87,105,247))]/5 px-1.5 py-0.5 text-[11px] text-[color:var(--claude-blue,rgb(87,105,247))] transition-colors hover:bg-[color:var(--claude-blue,rgb(87,105,247))]/15"
            title="固定到当前会话：后续每一轮都以此来源为主要上下文"
          >
            <Pin className="size-2.5" />
            <span>固定到会话</span>
          </button>
        )}
      </div>
    );
  }

  // A2 — manual session-binding tone (existing orange/secondary).
  return (
    <div
      className={cn(
        "mb-1 flex flex-wrap items-center gap-1 text-[11px] text-muted-foreground",
        className,
      )}
    >
      <span className="mr-1 shrink-0 text-muted-foreground/70">引用：</span>
      {boundSource && (() => {
        const Icon = iconFor(boundSource);
        return (
          <Badge
            key={sourceRefKey(boundSource)}
            variant="secondary"
            className="max-w-[260px] gap-1 text-[11px] font-normal"
            title={formatSourceRefLabel(boundSource)}
          >
            <Icon className="size-3 shrink-0" />
            <span className="truncate" dir="ltr">
              {formatSourceRefLabel(boundSource)}
            </span>
          </Badge>
        );
      })()}
      {showGenericLinkBadge && (
        <Badge
          variant="secondary"
          className="gap-1 text-[11px] font-normal"
          title="本链接"
        >
          <Link2 className="size-3 shrink-0" />
          <span>本链接</span>
        </Badge>
      )}
      {basis.grounding_applied === true && (
        <GroundedBadge toneClassName="border-muted-foreground/30 text-muted-foreground" />
      )}
    </div>
  );
}

/**
 * A1 sprint — one-line compact label that renders above an assistant
 * message and explains WHAT CONTEXT the backend actually used for that
 * turn. Driven by the `ContextBasis` side-channel (Worker A contract,
 * see `@/lib/tauri`).
 *
 * Display policy (Explorer C):
 *
 *   source_first:  <Link2 icon/>  主要依据：本链接  (≈{n} tokens)
 *   combine:       <Shuffle icon/> 结合 {k} 轮对话 + 本链接
 *   follow_up:     <MessageCircle/> 对话上下文（{k} 轮） — hidden by
 *                  default, only rendered when `forceShowFollowUp` is
 *                  true (e.g. the user explicitly asked "为什么你是
 *                  这样回答的?" via a 2-click disclosure; default view
 *                  stays clean). DS1.5 — Lucide icons only (stroke 1.5,
 *                  size-3), zero emoji in product copy.
 *
 * Hover surfaces a tooltip with the full basis dump for transparency.
 *
 * Spec size: ~80 lines.
 */

import { Link2, MessageCircle, Shuffle } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { ContextBasis } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { purposeLensLabel } from "@/features/purpose/purpose-lenses";

interface ContextBasisLabelProps {
  basis: ContextBasis | null | undefined;
  /**
   * Forces the `follow_up` variant to render even though it is normally
   * hidden. Wired up later by a disclosure affordance on the assistant
   * bubble ("show why"). Default false so the normal view stays quiet.
   */
  forceShowFollowUp?: boolean;
  className?: string;
}

function formatTokenHint(n: number | undefined): string {
  if (!n || n <= 0) return "";
  if (n >= 1000) return ` · 约 ${(n / 1000).toFixed(1)}k tokens`;
  return ` · 约 ${n} tokens`;
}

export function ContextBasisLabel({
  basis,
  forceShowFollowUp = false,
  className,
}: ContextBasisLabelProps) {
  if (!basis) return null;
  const purposeText = basis.purpose_lenses?.length
    ? `目的：${basis.purpose_lenses.map(purposeLensLabel).join(" / ")}`
    : "";

  // follow_up: hidden unless the caller explicitly opts in.
  if (basis.mode === "follow_up" && !forceShowFollowUp && !purposeText) return null;

  let icon: React.ReactNode;
  let text: string;
  let tone: string;

  switch (basis.mode) {
    case "source_first":
      icon = <Link2 className="size-3 shrink-0" />;
      text = `主要依据：本链接${formatTokenHint(basis.source_token_hint)}`;
      tone = "text-primary";
      break;
    case "combine":
      icon = <Shuffle className="size-3 shrink-0" />;
      text = `结合 ${basis.history_turns_included} 轮对话 + 本链接`;
      tone = "text-[color:var(--color-warning)]";
      break;
    case "follow_up":
    default:
      icon = <MessageCircle className="size-3 shrink-0" />;
      text = `对话上下文（${basis.history_turns_included} 轮）`;
      tone = "text-muted-foreground";
      break;
  }
  if (purposeText) {
    text = `${purposeText} · ${text}`;
  }

  const tooltipLines: string[] = [
    `mode: ${basis.mode}`,
    `purpose_lenses: ${basis.purpose_lenses?.length ? basis.purpose_lenses.join(", ") : "—"}`,
    `history_turns_included: ${basis.history_turns_included}`,
    `source_included: ${basis.source_included}`,
    basis.source_token_hint != null
      ? `source_token_hint: ${basis.source_token_hint}`
      : "source_token_hint: —",
    `boundary_marker: ${basis.boundary_marker}`,
    `grounding_applied: ${basis.grounding_applied === true}`,
  ];

  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div
            className={cn(
              "mb-1 inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] leading-none",
              tone,
              className,
            )}
          >
            {icon}
            <span>{text}</span>
          </div>
        </TooltipTrigger>
        <TooltipContent side="top" align="start">
          <pre className="m-0 whitespace-pre font-mono text-[10px] leading-snug">
            {tooltipLines.join("\n")}
          </pre>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

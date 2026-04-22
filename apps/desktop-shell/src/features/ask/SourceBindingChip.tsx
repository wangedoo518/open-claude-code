/**
 * A2 sprint — compact chip that surfaces the session's persistent source
 * binding (raw / wiki / inbox) above the Composer input. Hosts a close
 * button that clears the binding via `onClear`.
 *
 * Display spec:
 *   - raw   → Hash icon      + "raw #00123 · Example Domain"      + X
 *   - wiki  → BookOpen icon  + "wiki:foo-slug · Title"            + X
 *   - inbox → Inbox icon     + "inbox #42 · Title"                + X
 *
 * Tooltip hover reveals the full title + optional `binding_reason` for
 * provenance / debugging (same pattern as ContextBasisLabel).
 *
 * Returns null when `binding` is null/undefined — callers can render
 * the chip unconditionally without a guard.
 */

import { Hash, BookOpen, Inbox, X as XIcon } from "lucide-react";
import type { SessionSourceBinding } from "@/lib/tauri";
import { formatSourceRefLabel } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface SourceBindingChipProps {
  binding: SessionSourceBinding | null | undefined;
  onClear: () => void;
  className?: string;
}

export function SourceBindingChip({
  binding,
  onClear,
  className,
}: SourceBindingChipProps) {
  if (!binding) return null;
  const { source } = binding;

  let Icon: React.ComponentType<{ className?: string }>;
  switch (source.kind) {
    case "raw":
      Icon = Hash;
      break;
    case "wiki":
      Icon = BookOpen;
      break;
    case "inbox":
      Icon = Inbox;
      break;
  }

  const label = formatSourceRefLabel(source);

  const tooltipLines: string[] = [
    `kind: ${source.kind}`,
    source.kind === "raw" || source.kind === "inbox"
      ? `id: ${source.id}`
      : `slug: ${source.slug}`,
    `title: ${source.title}`,
    `bound_at: ${new Date(binding.bound_at).toISOString()}`,
  ];
  if (binding.binding_reason) {
    tooltipLines.push(`reason: ${binding.binding_reason}`);
  }

  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            className={cn(
              "inline-flex items-center gap-1 rounded-md border border-primary/30 bg-primary/10 px-2 py-1 text-[11px] text-primary",
              className,
            )}
          >
            <Icon className="size-3 shrink-0" />
            <span className="max-w-[280px] truncate" dir="ltr">
              {label}
            </span>
            <button
              type="button"
              onClick={onClear}
              aria-label="Clear source binding"
              className="ml-0.5 rounded p-0.5 opacity-60 transition-opacity hover:opacity-100"
            >
              <XIcon className="size-2.5" />
            </button>
          </span>
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

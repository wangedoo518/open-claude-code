/**
 * Ask Purpose Lens selector.
 *
 * This is the Ask-side counterpart to frontmatter `purpose`: the user
 * can constrain a turn to writing/building/research/etc. without
 * editing YAML or learning the schema first.
 */

import { useEffect, useRef, useState } from "react";
import { Check, Target } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  PURPOSE_LENSES,
  purposeLensLabel,
  type PurposeLensId,
} from "@/features/purpose/purpose-lenses";
import { InfoTooltip } from "@/components/ui/info-tooltip";

interface PurposeLensChipProps {
  value: PurposeLensId | null;
  onChange: (next: PurposeLensId | null) => void;
  disabled?: boolean;
}

export function PurposeLensChip({
  value,
  onChange,
  disabled = false,
}: PurposeLensChipProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const activeLens = value
    ? PURPOSE_LENSES.find((lens) => lens.id === value) ?? null
    : null;

  useEffect(() => {
    if (!open) return;
    const handler = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const label = activeLens ? purposeLensLabel(activeLens.id) : "自动";
  const title = activeLens
    ? `Purpose Lens：${activeLens.label} · ${activeLens.output}`
    : "Purpose Lens：自动匹配";

  return (
    <div className="inline-flex items-center gap-1">
      <div className="relative" ref={ref}>
        <button
          type="button"
          className={cn(
            "inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] transition-colors",
            activeLens
              ? "border-[color:var(--claude-blue)]/30 bg-[color:var(--claude-blue)]/10 text-[color:var(--claude-blue)]"
              : "border-border/50 bg-muted/30 text-muted-foreground",
            !disabled && "hover:bg-accent hover:text-foreground",
          )}
          title={title}
          aria-label={title}
          aria-haspopup="menu"
          aria-expanded={open}
          disabled={disabled}
          onClick={() => setOpen((current) => !current)}
        >
          <Target className="size-3" aria-hidden />
          <span>目的：{label}</span>
        </button>

        {open && (
          <div
            className="ask-floating-menu absolute bottom-full left-0 z-50 mb-1 min-w-[260px] rounded-lg border border-border bg-popover p-1 shadow-lg"
            role="menu"
          >
            <div className="px-2 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              Purpose Lens
            </div>
            <PurposeOption
              label="自动匹配"
              description="不限定目的，沿用当前上下文"
              active={value == null}
              onClick={() => {
                onChange(null);
                setOpen(false);
              }}
            />
            {PURPOSE_LENSES.map((lens) => (
              <PurposeOption
                key={lens.id}
                label={`${lens.zhLabel} · ${lens.label}`}
                description={lens.output}
                active={value === lens.id}
                onClick={() => {
                  onChange(lens.id);
                  setOpen(false);
                }}
              />
            ))}
          </div>
        )}
      </div>
      <InfoTooltip side="top">
        <div className="space-y-1">
          <div className="font-medium text-background">Purpose Lens</div>
          <div className="leading-relaxed">
            限定本轮 Ask 的用途：写作、构建、运营、学习、个人或研究。
            Buddy 会把这个目的传给后端，用来约束回答形态和后续行动。
          </div>
        </div>
      </InfoTooltip>
    </div>
  );
}

function PurposeOption({
  label,
  description,
  active,
  onClick,
}: {
  label: string;
  description: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={cn(
        "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
        active ? "bg-accent text-foreground" : "text-foreground hover:bg-accent/50",
      )}
      onClick={onClick}
      role="menuitemradio"
      aria-checked={active}
    >
      <span className="mt-0.5 flex size-3 shrink-0 items-center justify-center">
        {active && <Check className="size-3" aria-hidden />}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[11px] font-medium">{label}</span>
        <span className="mt-0.5 block text-[10px] leading-snug text-muted-foreground/75">
          {description}
        </span>
      </span>
    </button>
  );
}

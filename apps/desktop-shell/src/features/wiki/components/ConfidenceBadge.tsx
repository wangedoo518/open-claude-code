import { cn } from "@/lib/utils";

interface ConfidenceBadgeProps {
  confidence: number | null | undefined;
  className?: string;
  size?: "sm" | "md";
}

type ConfidenceLevel = "high" | "medium" | "low" | "decay";

function levelOf(confidence: number | null | undefined): ConfidenceLevel | null {
  if (confidence == null || !Number.isFinite(confidence)) return null;
  if (confidence >= 0.8) return "high";
  if (confidence >= 0.5) return "medium";
  if (confidence >= 0.3) return "low";
  return "decay";
}

const LEVEL_CONFIG: Record<
  ConfidenceLevel,
  {
    label: string;
    toneClass: string;
    ariaLabel: string;
  }
> = {
  high: {
    label: "已多源验证",
    toneClass: "bg-emerald-50 text-emerald-700 border-emerald-200",
    ariaLabel: "知识可信度：高（已多源验证）",
  },
  medium: {
    label: "信息可靠",
    toneClass: "bg-stone-50 text-stone-700 border-stone-200",
    ariaLabel: "知识可信度：中（信息可靠）",
  },
  low: {
    label: "需要验证",
    toneClass: "bg-amber-50 text-amber-700 border-amber-200",
    ariaLabel: "知识可信度：低（需要验证）",
  },
  decay: {
    label: "可能过时",
    toneClass: "bg-rose-50 text-rose-700 border-rose-200",
    ariaLabel: "知识可信度：极低（可能过时）",
  },
};

export function ConfidenceBadge({
  confidence,
  className,
  size = "sm",
}: ConfidenceBadgeProps) {
  const level = levelOf(confidence);
  if (!level) return null;

  const config = LEVEL_CONFIG[level];
  const percent = Math.round((confidence ?? 0) * 100);
  const sizeClass = size === "sm"
    ? "px-1.5 py-0.5 text-[11px]"
    : "px-2 py-1 text-xs";

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-md border font-medium",
        config.toneClass,
        sizeClass,
        className,
      )}
      aria-label={config.ariaLabel}
      title={`置信度 ${percent}%`}
      data-testid="confidence-badge"
    >
      {config.label}
    </span>
  );
}

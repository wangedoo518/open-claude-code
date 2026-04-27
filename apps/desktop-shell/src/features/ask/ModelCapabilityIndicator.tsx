import { cn } from "@/lib/utils";
import type { ModelCapability } from "./model-capabilities";

interface Props {
  capability: ModelCapability;
  className?: string;
}

export function ModelCapabilityIndicator({ capability, className }: Props) {
  return (
    <span
      className={cn("inline-flex items-center gap-1", className)}
      data-testid="model-capability-indicator"
    >
      <CapDot active={capability.chat} label="文本" />
      <CapDot active={capability.tools} label="工具" />
      <CapDot active={capability.vision} label="视觉" />
    </span>
  );
}

function CapDot({ active, label }: { active: boolean; label: string }) {
  const status = active ? "可用" : "不支持";
  return (
    <span
      className={cn(
        "h-1.5 w-1.5 rounded-full transition-colors",
        active ? "bg-[#1D9E75]" : "bg-[rgba(44,44,42,0.18)]",
      )}
      title={`${label}：${status}`}
      aria-label={`${label}：${status}`}
    />
  );
}

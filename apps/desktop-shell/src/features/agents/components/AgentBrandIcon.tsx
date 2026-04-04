/**
 * AgentBrandIcon — Renders the OpenClaw brand icon
 *
 * Port from clawhub123/src/v2/features/agents/components/AgentBrandIcon.tsx
 */

import { Shell } from "lucide-react";

interface AgentBrandIconProps {
  agentId: string;
  variant?: "box" | "plain";
}

export function AgentBrandIcon({
  agentId: _agentId,
  variant = "box",
}: AgentBrandIconProps) {
  if (variant === "plain") {
    return <Shell className="size-[52px] text-red-500" strokeWidth={1.5} />;
  }

  return (
    <div className="inline-flex items-center justify-center size-16 rounded-[20px] bg-gradient-to-b from-red-50/95 to-red-100/95 shadow-[inset_0_1px_0_rgba(255,255,255,0.75),0_10px_22px_rgba(15,23,42,0.08)] shrink-0">
      <Shell className="size-[34px] text-red-500" strokeWidth={1.5} />
    </div>
  );
}

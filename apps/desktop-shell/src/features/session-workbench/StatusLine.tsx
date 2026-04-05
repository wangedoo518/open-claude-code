import { Cpu, Globe, Zap, Hash, Clock } from "lucide-react";
import { cn } from "@/lib/utils";
import { useAppSelector } from "@/store";
import { getPermissionConfig } from "./InputBar";

interface StatusLineProps {
  modelLabel?: string;
  environmentLabel?: string;
  isRunning?: boolean;
  tokenCount?: number;
  sessionDuration?: string;
}

/**
 * StatusLine — bottom status bar matching Claude Code desktop.
 *
 * Layout:
 *   [Model badge] [Permission mode] [Environment]  ─────  [Running] [Tokens] [Duration]
 */
export function StatusLine({
  modelLabel = "Opus 4.6",
  environmentLabel = "Local",
  isRunning = false,
  tokenCount = 0,
  sessionDuration,
}: StatusLineProps) {
  const permissionMode = useAppSelector((s) => s.settings.permissionMode);
  const config = getPermissionConfig(permissionMode);
  const ModeIcon = config.icon;

  return (
    <div className="flex h-6 items-center justify-between border-t border-border/30 bg-muted/20 px-3 text-[10px] text-muted-foreground">
      {/* Left side */}
      <div className="flex items-center gap-3">
        {/* Model */}
        <StatusItem icon={Cpu} label={modelLabel} />

        {/* Permission mode */}
        <span
          className="flex items-center gap-1"
          style={config.color ? { color: config.color } : undefined}
        >
          <ModeIcon className="size-2.5" />
          <span>{config.label}</span>
        </span>

        {/* Environment */}
        <StatusItem icon={Globe} label={environmentLabel} />
      </div>

      {/* Right side */}
      <div className="flex items-center gap-3">
        {isRunning && (
          <span
            className="flex items-center gap-1 font-medium"
            style={{ color: "var(--claude-orange, rgb(215,119,87))" }}
          >
            <Zap className="size-2.5" />
            Running
          </span>
        )}

        {tokenCount > 0 && (
          <StatusItem
            icon={Hash}
            label={`${tokenCount.toLocaleString()} tokens`}
          />
        )}

        {sessionDuration && (
          <StatusItem icon={Clock} label={sessionDuration} />
        )}
      </div>
    </div>
  );
}

function StatusItem({
  icon: Icon,
  label,
}: {
  icon: typeof Cpu;
  label: string;
}) {
  return (
    <span className={cn("flex items-center gap-1")}>
      <Icon className="size-2.5" />
      <span>{label}</span>
    </span>
  );
}

// S0.3 extraction target: bottom status line (shared by Ask + Inbox).
//
// Original: features/session-workbench/StatusLine.tsx.
//
// S0.3 intentional drops:
// 1. `WorkspaceSkillsPanel` — the right-side "skills count + popover"
//    is dropped here. Canonical §11.1 deletes `WorkspaceSkillsPanel`
//    in S0.4 (it's part of the old session-workbench feature). S3 will
//    reintroduce an equivalent panel backed by the Codex Pool status
//    feed instead of workspace skill scanning, if needed.
// 2. `getPermissionConfig` now comes from
//    `@/features/permission/permission-config` (extracted during S0.3
//    to break the former common ← ask dependency).

import { Cpu, FileSearch, Globe, Zap } from "lucide-react";
import { cn } from "@/lib/utils";
import { getPermissionConfig } from "@/features/permission/permission-config";
import { useSettingsStore } from "@/state/settings-store";
import { useStreamingStore } from "@/state/streaming-store";

interface StatusLineProps {
  modelLabel?: string;
  environmentLabel?: string;
  isRunning?: boolean;
  /** Project root path — reserved for S3 status reporting. */
  projectPath?: string;
}

/**
 * StatusLine — bottom status bar matching Claude Code desktop.
 *
 * Layout:
 *   [Model badge] [Permission mode] [Plan mode?] [Environment]  ─────  [Running?]
 */
export function StatusLine({
  modelLabel = "Codex GPT-5.4",
  environmentLabel = "via internal broker",
  isRunning = false,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars -- reserved for S3 status feed
  projectPath: _projectPath,
}: StatusLineProps) {
  const permissionMode = useSettingsStore((state) => state.permissionMode);
  const isPlanMode = useStreamingStore((state) => state.isPlanMode);
  const config = getPermissionConfig(permissionMode);
  const ModeIcon = config.icon;

  return (
    <div className="flex h-6 items-center justify-between border-t border-border/30 bg-muted/20 px-3 text-caption text-muted-foreground">
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

        {/* Plan Mode badge */}
        {isPlanMode && (
          <span
            className="flex items-center gap-1 font-medium"
            style={{ color: "var(--color-warning)" }}
          >
            <FileSearch className="size-2.5" />
            Plan Mode
          </span>
        )}

        {/* Environment */}
        <StatusItem icon={Globe} label={environmentLabel} />
      </div>

      {/* Right side */}
      <div className="flex items-center gap-3">
        {isRunning && (
          <span
            className="flex items-center gap-1 font-medium"
            style={{ color: "var(--claude-orange)" }}
          >
            <Zap className="size-2.5" />
            Running
          </span>
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

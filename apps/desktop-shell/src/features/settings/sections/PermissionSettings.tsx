import { Shield, ShieldAlert, ShieldCheck } from "lucide-react";
import { SettingGroup } from "../components/SettingGroup";
import { cn } from "@/lib/utils";
import type { DesktopCustomizeState } from "@/lib/tauri";

interface PermissionSettingsProps {
  customize: DesktopCustomizeState | null;
  error?: string;
}

const MODES = [
  {
    value: "accept_edits",
    label: "Accept Edits",
    desc: "Automatically approve edit operations while still surfacing execution context.",
    icon: ShieldCheck,
    color: "text-green-500",
  },
  {
    value: "ask",
    label: "Ask",
    desc: "Prompt before tools run.",
    icon: Shield,
    color: "text-yellow-500",
  },
  {
    value: "danger_full_access",
    label: "Danger Full Access",
    desc: "Execute tools without confirmation.",
    icon: ShieldAlert,
    color: "text-destructive",
  },
] as const;

export function PermissionSettings({
  customize,
  error,
}: PermissionSettingsProps) {
  const activeMode = normalizePermissionMode(customize?.permission_mode);

  return (
    <div className="space-y-4">
      <SettingGroup
        title="Permission Mode"
        description="Mirrors the permission mode from the live Rust runtime"
      >
        <div className="space-y-2">
          {MODES.map((mode) => (
            <div
              key={mode.value}
              className={cn(
                "flex w-full items-center gap-3 rounded-md border p-3 text-left",
                activeMode === mode.value
                  ? "border-primary bg-primary/5"
                  : "border-border"
              )}
            >
              <mode.icon className={cn("size-5", mode.color)} />
              <div className="flex-1">
                <div className="text-sm font-medium">{mode.label}</div>
                <div className="text-xs text-muted-foreground">{mode.desc}</div>
              </div>
              {activeMode === mode.value && (
                <div className="size-2 rounded-full bg-primary" />
              )}
            </div>
          ))}
        </div>
        <div className="text-xs text-muted-foreground">
          Current runtime value: {customize?.permission_mode ?? "Unavailable"}
        </div>
      </SettingGroup>

      {error && (
        <SettingGroup title="Warnings">
          <div className="text-xs text-muted-foreground">{error}</div>
        </SettingGroup>
      )}
    </div>
  );
}

function normalizePermissionMode(value: string | undefined) {
  const normalized = value?.toLowerCase().replace(/\s+/g, "_") ?? "";
  if (normalized.includes("danger")) return "danger_full_access";
  if (normalized.includes("ask")) return "ask";
  return "accept_edits";
}

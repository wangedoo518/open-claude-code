import { SettingGroup, SettingRow } from "../components/SettingGroup";
import type { DesktopSettingsState } from "@/lib/tauri";

interface DataSettingsProps {
  settings: DesktopSettingsState | null;
  error?: string;
}

export function DataSettings({ settings, error }: DataSettingsProps) {
  return (
    <div className="space-y-4">
      <SettingGroup title="Project">
        <SettingRow
          label="Current Project Path"
          description="Working directory used by the desktop runtime"
        >
          <div className="max-w-[360px] text-right text-xs text-muted-foreground">
            {settings?.project_path ?? "Unavailable"}
          </div>
        </SettingRow>
        <SettingRow
          label="Config Home"
          description="Resolved CLAW/OpenClaudeCode config root"
        >
          <div className="max-w-[360px] text-right text-xs text-muted-foreground">
            {settings?.config_home ?? "Unavailable"}
          </div>
        </SettingRow>
      </SettingGroup>

      <SettingGroup
        title="Storage Locations"
        description="Paths currently used by the desktop runtime"
      >
        <div className="space-y-2">
          {settings?.storage_locations.map((location) => (
            <div
              key={`${location.label}-${location.path}`}
              className="rounded-md border border-border bg-muted/20 px-3 py-2"
            >
              <div className="text-sm font-medium">{location.label}</div>
              <div className="mt-0.5 break-all text-xs text-muted-foreground">
                {location.path}
              </div>
              <div className="mt-1 text-xs text-muted-foreground">
                {location.description}
              </div>
            </div>
          ))}

          {settings?.storage_locations.length === 0 && (
            <div className="py-4 text-center text-xs text-muted-foreground">
              No storage locations reported by the runtime.
            </div>
          )}
        </div>
      </SettingGroup>

      {(error || settings?.warnings.length) && (
        <SettingGroup title="Warnings">
          <div className="space-y-2 text-xs text-muted-foreground">
            {error && <div>{error}</div>}
            {settings?.warnings.map((warning) => (
              <div key={warning}>{warning}</div>
            ))}
          </div>
        </SettingGroup>
      )}
    </div>
  );
}

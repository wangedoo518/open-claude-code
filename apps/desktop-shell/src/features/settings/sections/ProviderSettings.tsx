import { Badge } from "@/components/ui/badge";
import { SettingGroup, SettingRow } from "../components/SettingGroup";
import type {
  DesktopCustomizeState,
  DesktopSettingsState,
} from "@/lib/tauri";

interface ProviderSettingsProps {
  settings: DesktopSettingsState | null;
  customize: DesktopCustomizeState | null;
  error?: string;
}

export function ProviderSettings({
  settings,
  customize,
  error,
}: ProviderSettingsProps) {
  return (
    <div className="space-y-4">
      <SettingGroup title="Active Model">
        <SettingRow
          label="Runtime Model"
          description="Loaded from the merged Rust runtime configuration"
        >
          <div className="text-right">
            <div className="text-sm font-medium">
              {customize?.model_label ?? "Unavailable"}
            </div>
            <div className="text-xs text-muted-foreground">
              {customize?.model_id ?? "No model detected"}
            </div>
          </div>
        </SettingRow>
      </SettingGroup>

      <SettingGroup
        title="Configured Providers"
        description="Provider endpoints discovered from the current desktop runtime"
      >
        <div className="space-y-2">
          {settings?.providers.map((provider) => (
            <div
              key={provider.id}
              className="rounded-md border border-border bg-muted/20 px-3 py-2"
            >
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-sm font-medium">{provider.label}</div>
                  <div className="text-xs text-muted-foreground">
                    {provider.base_url}
                  </div>
                </div>
                <Badge variant="secondary" className="text-[10px]">
                  {provider.auth_status}
                </Badge>
              </div>
            </div>
          ))}

          {settings?.providers.length === 0 && (
            <div className="py-4 text-center text-xs text-muted-foreground">
              No provider configuration was discovered.
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

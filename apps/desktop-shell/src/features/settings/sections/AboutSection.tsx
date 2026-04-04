import { Sparkles } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { SettingGroup } from "../components/SettingGroup";
import type { DesktopSettingsState } from "@/lib/tauri";

interface AboutSectionProps {
  productName?: string;
  settings: DesktopSettingsState | null;
  error?: string;
}

export function AboutSection({
  productName,
  settings,
  error,
}: AboutSectionProps) {
  return (
    <div className="space-y-4">
      <SettingGroup title="About">
        <div className="flex items-center gap-4">
          <div className="flex size-12 items-center justify-center rounded-xl bg-primary/10">
            <Sparkles className="size-6 text-primary" />
          </div>
          <div>
            <div className="text-sm font-semibold">
              {productName ?? "Warwolf"}
            </div>
            <div className="text-xs text-muted-foreground">Desktop shell</div>
            <div className="mt-1 flex gap-1">
              <Badge variant="secondary" className="text-[10px]">
                Tauri 2
              </Badge>
              <Badge variant="secondary" className="text-[10px]">
                React 19
              </Badge>
              <Badge variant="secondary" className="text-[10px]">
                Rust runtime
              </Badge>
            </div>
          </div>
        </div>
      </SettingGroup>

      <SettingGroup title="Runtime Paths">
        <div className="space-y-1.5 text-sm">
          <div className="flex justify-between gap-4">
            <span className="text-muted-foreground">Project</span>
            <span className="max-w-[360px] truncate text-right">
              {settings?.project_path ?? "Unavailable"}
            </span>
          </div>
          <div className="flex justify-between gap-4">
            <span className="text-muted-foreground">Session Store</span>
            <span className="max-w-[360px] truncate text-right">
              {settings?.desktop_session_store_path ?? "Unavailable"}
            </span>
          </div>
          <div className="flex justify-between gap-4">
            <span className="text-muted-foreground">OAuth Credentials</span>
            <span className="max-w-[360px] truncate text-right">
              {settings?.oauth_credentials_path ?? "Unavailable"}
            </span>
          </div>
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

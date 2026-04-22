import { SettingGroup, SettingRow } from "../components/SettingGroup";
import type { DesktopSettingsState } from "@/lib/tauri";

interface DataSettingsProps {
  settings: DesktopSettingsState | null;
  error?: string;
}

/**
 * P2-1 — map backend-authored English engineering labels from
 * `desktop-core/src/lib.rs:7351-7373` to user-facing Chinese copy.
 *
 * The backend enumerates 4 fixed storage locations (Config home /
 * Desktop sessions / Plugin install root / Plugin registry). Default
 * copy was direct engineering jargon ("Config home — Merged runtime
 * settings, plugin settings, and local desktop metadata.") which the
 * DS layer forbids.
 *
 * Each entry keeps ≤ 20-char Chinese labels/descriptions, drops
 * runtime terminology (config / registry / sidebar / plugin-metadata),
 * and falls back to the raw label/description when an unknown key
 * appears — so a future backend entry degrades gracefully rather than
 * silently hiding information.
 */
const LOCATION_LABEL_MAP: Record<
  string,
  { label: string; description: string }
> = {
  "Config home": {
    label: "应用配置",
    description: "桌面端的运行时配置与本地偏好设置",
  },
  "Desktop sessions": {
    label: "对话存档",
    description: "Ask 页面保存下来的会话历史",
  },
  "Plugin install root": {
    label: "扩展目录",
    description: "已安装扩展的代码与资源",
  },
  "Plugin registry": {
    label: "扩展清单",
    description: "当前启用的扩展记录",
  },
};

function localizeLocation(label: string, description: string): {
  label: string;
  description: string;
} {
  const mapped = LOCATION_LABEL_MAP[label];
  if (mapped) return mapped;
  // Unknown key — keep the backend-authored copy so the user at least
  // sees *something*, but this is a signal that LOCATION_LABEL_MAP
  // needs to be extended the next time the backend adds a location.
  return { label, description };
}

export function DataSettings({ settings, error }: DataSettingsProps) {
  return (
    <div className="space-y-4">
      <SettingGroup
        title="知识库根目录"
        description="ClawWiki 的所有素材、知识页、整理规则都保存在这个目录下。一台机器只有一个根目录。"
      >
        <SettingRow
          label="当前运行路径"
          description="桌面端当前使用的知识库目录"
        >
          <div className="max-w-[360px] text-right text-caption text-muted-foreground">
            {settings?.project_path ?? "暂不可用"}
          </div>
        </SettingRow>
        <SettingRow
          label="配置文件目录"
          description="ClawWiki / OpenClaudeCode 的配置文件所在目录"
        >
          <div className="max-w-[360px] text-right text-caption text-muted-foreground">
            {settings?.config_home ?? "暂不可用"}
          </div>
        </SettingRow>
      </SettingGroup>

      <SettingGroup
        title="存储位置"
        description="桌面端运行时正在使用的各类文件路径"
      >
        <div className="space-y-2">
          {settings?.storage_locations.map((location) => {
            const ui = localizeLocation(location.label, location.description);
            return (
              <div
                key={`${location.label}-${location.path}`}
                className="rounded-md border border-border bg-muted/20 px-3 py-2"
              >
                <div className="text-body-sm font-semibold text-foreground">
                  {ui.label}
                </div>
                <div className="mt-0.5 break-all text-caption text-muted-foreground">
                  {location.path}
                </div>
                <div className="mt-1 text-caption text-muted-foreground">
                  {ui.description}
                </div>
              </div>
            );
          })}

          {settings?.storage_locations.length === 0 && (
            <div className="py-4 text-center text-caption text-muted-foreground">
              运行时未汇报任何存储位置。
            </div>
          )}
        </div>
      </SettingGroup>

      {(error || settings?.warnings.length) && (
        <SettingGroup title="警告">
          <div className="space-y-2 text-caption text-muted-foreground">
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

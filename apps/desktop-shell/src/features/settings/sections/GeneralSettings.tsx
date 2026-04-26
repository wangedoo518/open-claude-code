import { SettingGroup } from "../components/SettingGroup";
import { useSettingsStore, type ThemeMode } from "@/state/settings-store";

const LANGUAGES = [
  { value: "zh-CN", label: "简体中文" },
  { value: "en", label: "English" },
] as const;

export function GeneralSettings() {
  const theme = useSettingsStore((state) => state.theme);
  const fontSize = useSettingsStore((state) => state.fontSize);
  const language = useSettingsStore((state) => state.language);
  const setTheme = useSettingsStore((state) => state.setTheme);
  const setFontSize = useSettingsStore((state) => state.setFontSize);
  const setLanguage = useSettingsStore((state) => state.setLanguage);

  const themes: { value: ThemeMode; label: string }[] = [
    { value: "light", label: "浅色" },
    { value: "dark", label: "深色" },
    { value: "system", label: "跟随系统" },
  ];

  const fontSizes = [12, 13, 14, 15, 16];

  function resetAppearance() {
    setTheme("system");
    setFontSize(14);
    setLanguage("zh-CN");
  }

  return (
    <div>
      <SettingGroup title="主题" description="选择你偏好的配色方案">
        <div className="settings-segmented">
          {themes.map((t) => (
            <button
              key={t.value}
              type="button"
              data-active={theme === t.value || undefined}
              onClick={() => setTheme(t.value)}
            >
              {t.label}
            </button>
          ))}
        </div>
      </SettingGroup>

      <SettingGroup title="字体大小" description="编辑器和终端字体大小">
        <div className="flex flex-wrap items-center gap-3">
          <div className="settings-segmented">
            {fontSizes.map((size) => (
              <button
                key={size}
                type="button"
                data-active={fontSize === size || undefined}
                onClick={() => setFontSize(size)}
              >
                {size}
              </button>
            ))}
          </div>
          <div
            className="settings-font-preview"
            style={{ fontSize }}
          >
            Aa 这是示例文本
          </div>
        </div>
      </SettingGroup>

      <SettingGroup title="语言" description="应用显示语言">
        <div className="settings-segmented">
          {LANGUAGES.map((lang) => (
            <button
              key={lang.value}
              type="button"
              data-active={language === lang.value || undefined}
              onClick={() => setLanguage(lang.value)}
            >
              {lang.label}
            </button>
          ))}
        </div>
      </SettingGroup>

      <div className="settings-action-row">
        <button
          type="button"
          className="settings-secondary-action"
          onClick={resetAppearance}
        >
          恢复默认外观
        </button>
      </div>
    </div>
  );
}

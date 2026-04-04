import { useAppDispatch, useAppSelector } from "@/store";
import { setTheme, setFontSize, type ThemeMode } from "@/store/slices/settings";
import { SettingGroup, SettingRow } from "../components/SettingGroup";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function GeneralSettings() {
  const dispatch = useAppDispatch();
  const theme = useAppSelector((s) => s.settings.theme);
  const fontSize = useAppSelector((s) => s.settings.fontSize);

  const themes: { value: ThemeMode; label: string }[] = [
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
    { value: "system", label: "System" },
  ];

  const fontSizes = [12, 13, 14, 15, 16];

  return (
    <div className="space-y-4">
      <SettingGroup title="Appearance">
        <SettingRow label="Theme" description="Choose your preferred color scheme">
          <div className="flex gap-1">
            {themes.map((t) => (
              <Button
                key={t.value}
                variant={theme === t.value ? "default" : "outline"}
                size="sm"
                className="text-xs"
                onClick={() => dispatch(setTheme(t.value))}
              >
                {t.label}
              </Button>
            ))}
          </div>
        </SettingRow>
        <SettingRow label="Font Size" description="Editor and terminal font size">
          <div className="flex gap-1">
            {fontSizes.map((size) => (
              <Button
                key={size}
                variant={fontSize === size ? "default" : "outline"}
                size="sm"
                className={cn("w-10 text-xs")}
                onClick={() => dispatch(setFontSize(size))}
              >
                {size}
              </Button>
            ))}
          </div>
        </SettingRow>
      </SettingGroup>

      <SettingGroup
        title="Language"
        description="Application display language"
      >
        <SettingRow label="Language">
          <Button variant="outline" size="sm" className="text-xs">
            English
          </Button>
        </SettingRow>
      </SettingGroup>
    </div>
  );
}

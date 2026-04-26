import { useMemo, useState } from "react";
import { Keyboard, Search } from "lucide-react";
import { SettingGroup } from "../components/SettingGroup";

interface ShortcutEntry {
  keys: string;
  description: string;
}

const ASK_SHORTCUTS: readonly ShortcutEntry[] = [
  { keys: "Esc", description: "停止流式输出 / 关闭对话框" },
  { keys: "Ctrl+L", description: "清空消息" },
  { keys: "Ctrl+N", description: "新建对话" },
  { keys: "Ctrl+K", description: "聚焦输入框" },
  { keys: "Ctrl+,", description: "打开设置" },
  { keys: "Ctrl+Shift+S", description: "切换侧边栏" },
  { keys: "Ctrl+Shift+B", description: "切换维护任务栏" },
];

export function ShortcutsSettings() {
  const [query, setQuery] = useState("");

  const filteredShortcuts = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return ASK_SHORTCUTS;
    return ASK_SHORTCUTS.filter((shortcut) =>
      `${shortcut.description} ${shortcut.keys}`.toLowerCase().includes(normalized),
    );
  }, [query]);

  return (
    <SettingGroup
      title="快捷键"
      description="搜索或编辑 Ask 对话页面中的键位"
    >
      <div className="settings-shortcut-toolbar">
        <Search className="size-3.5" strokeWidth={1.5} aria-hidden="true" />
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="搜索快捷键…"
          aria-label="搜索快捷键"
        />
      </div>

      <div className="settings-shortcut-table">
        {filteredShortcuts.map((shortcut) => (
          <div key={shortcut.keys} className="settings-shortcut-row">
            <div className="flex min-w-0 items-center gap-2">
              <Keyboard className="size-3.5 shrink-0 text-[#888780]" strokeWidth={1.5} />
              <span className="truncate">{shortcut.description}</span>
            </div>
            <div className="settings-shortcut-actions">
              <button type="button" className="settings-shortcut-edit">
                编辑
              </button>
              <kbd>{shortcut.keys}</kbd>
            </div>
          </div>
        ))}
        {filteredShortcuts.length === 0 ? (
          <div className="settings-empty-row">没有找到匹配的快捷键。</div>
        ) : null}
      </div>

      <details className="settings-dev-details">
        <summary>开发者高级选项 · 外观偏好与键位配置</summary>
        <div className="settings-dev-details-body text-caption text-muted-foreground">
          主题、字号和语言保存在本机设置中；快捷键目前使用内置键位表，后续接入自定义键位后会在这里显示原始配置。
        </div>
      </details>
    </SettingGroup>
  );
}

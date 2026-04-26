import { useEffect, useState } from "react";
import { AlertTriangle, Edit3 } from "lucide-react";
import { SettingGroup } from "../components/SettingGroup";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { cn } from "@/lib/utils";
import { PERMISSION_MODES } from "@/features/permission/permission-config";
import type { DesktopCustomizeState } from "@/lib/tauri";
import { useSettingsStore, type PermissionMode } from "@/state/settings-store";

interface PermissionSettingsProps {
  customize: DesktopCustomizeState | null;
  error?: string;
}

/**
 * PermissionSettings — DS1.4 localised copy.
 *
 * Pre-DS1.4 this surface leaked three English strings into the
 * default layer:
 *   - "Permission Mode"
 *   - "Controls how tool execution permissions are handled"
 *   - "Runtime value: Danger full access"
 *
 * The first two became plain Chinese. The third (the raw runtime
 * value from `customize.permission_mode`) is a debug value; DS1.4
 * keeps it, but only inside a collapsed `<details>` block so
 *灰度测试 users don't see "Danger full access" as the dominant
 * headline of their permissions page.
 */
export function PermissionSettings({
  customize,
  error,
}: PermissionSettingsProps) {
  const currentMode = useSettingsStore((state) => state.permissionMode);
  const setPermissionMode = useSettingsStore((state) => state.setPermissionMode);
  const [draftMode, setDraftMode] = useState<PermissionMode>(currentMode);
  const [confirmBypass, setConfirmBypass] = useState(false);

  useEffect(() => {
    setDraftMode(currentMode);
  }, [currentMode]);

  const hasChanges = draftMode !== currentMode;

  function saveDraft() {
    if (draftMode === "bypassPermissions" && currentMode !== "bypassPermissions") {
      setConfirmBypass(true);
      return;
    }
    setPermissionMode(draftMode);
  }

  return (
    <div>
      <SettingGroup
        title="权限模式"
        description="决定执行工具和修改文件前是否需要你确认"
      >
        <div className="grid gap-2">
          {PERMISSION_MODES.map((mode) => {
            const Icon = mode.value === "acceptEdits" ? Edit3 : mode.icon;
            const isActive = draftMode === mode.value;
            const isDanger = mode.value === "bypassPermissions";
            return (
              <button
                key={mode.value}
                className={cn(
                  "flex w-full items-center gap-3 rounded-md border px-3 py-3 text-left transition-colors",
                  isActive
                    ? "border-[#D85A30] bg-[rgba(216,90,48,0.04)]"
                    : "border-[rgba(44,44,42,0.12)] bg-white/60 hover:bg-white",
                  isDanger && "border-[rgba(196,69,69,0.3)] bg-[#FBE9E9]/50",
                )}
                onClick={() => setDraftMode(mode.value)}
              >
                <Icon
                  className="size-5 shrink-0"
                  style={isDanger ? { color: "#C44545" } : mode.color ? { color: mode.color } : undefined}
                />
                <div className="flex-1">
                  <div className="text-body-sm font-semibold text-foreground">
                    {mode.label}
                    {isDanger ? <span className="ml-1 text-[#C44545]">危险</span> : null}
                  </div>
                  <div className={cn("text-caption text-muted-foreground", isDanger && "text-[#C44545]")}>
                    {mode.desc}
                  </div>
                </div>
                {isActive && (
                  <span className="size-2 rounded-full bg-[#D85A30]" aria-hidden="true" />
                )}
              </button>
            );
          })}
        </div>

        {draftMode === "bypassPermissions" ? (
          <div className="settings-danger-panel mt-3 flex items-start gap-2">
            <AlertTriangle className="mt-0.5 size-4 shrink-0" />
            <div>
              跳过权限会让外脑无需确认就执行读写和工具操作。只建议在完全可信的本地项目中短时间开启。
            </div>
          </div>
        ) : null}

        <div className="settings-action-row">
          <span className={`settings-status-pill ${currentMode === "bypassPermissions" ? "settings-status-pill--error" : "settings-status-pill--ok"}`}>
            {currentMode === "bypassPermissions" ? "高风险模式" : "当前设置正常"}
          </span>
          <Button
            size="sm"
            disabled={!hasChanges}
            onClick={saveDraft}
          >
            保存设置
          </Button>
        </div>

      </SettingGroup>

      {error && (
        <SettingGroup title="状态提醒">
          <div className="settings-danger-panel">{error}</div>
        </SettingGroup>
      )}

      <details className="settings-dev-details">
        <summary>开发者高级选项 · 运行环境中的原始权限值</summary>
        <div className="settings-dev-details-body">
          <code className="settings-dev-code">
            permission_mode = {customize?.permission_mode ?? "未上报"}
          </code>
        </div>
      </details>

      <ConfirmDialog
        open={confirmBypass}
        onOpenChange={setConfirmBypass}
        title="你确定要关闭所有安全检查吗？"
        description="开启后，外脑执行工具和文件操作时不会再逐项询问。请只在完全可信的本地环境中短时间使用。"
        confirmLabel="确认开启"
        cancelLabel="取消"
        variant="destructive"
        onConfirm={() => {
          setPermissionMode("bypassPermissions");
          setConfirmBypass(false);
        }}
      />
    </div>
  );
}

// S0.3 extraction target: permission-mode catalogue and resolver.
//
// Original location: `PERMISSION_MODES` + `getPermissionConfig` were
// inline exports inside `features/session-workbench/InputBar.tsx`.
// Extracting them here breaks what would otherwise be a cycle between
// `features/common/StatusLine` (which renders the current mode badge)
// and `features/ask/Composer` (which renders the mode picker). Both
// now import from this single, dependency-light module.
//
// MVP note: the `PermissionMode` type still lives in
// `@/state/settings-store` next to its setter. Only the display config
// moves here.

import {
  FileSearch,
  Shield,
  ShieldCheck,
  ShieldOff,
} from "lucide-react";

import type { PermissionMode } from "@/state/settings-store";

export interface PermissionModeConfig {
  value: PermissionMode;
  label: string;
  desc: string;
  icon: typeof Shield;
  color?: string;
}

export const PERMISSION_MODES: readonly PermissionModeConfig[] = [
  {
    value: "default",
    label: "需要确认",
    desc: "危险操作需要确认",
    icon: Shield,
  },
  {
    value: "acceptEdits",
    label: "允许编辑",
    desc: "自动接受文件编辑，其他操作需确认",
    icon: ShieldCheck,
    color: "var(--color-success)",
  },
  {
    value: "bypassPermissions",
    label: "跳过权限",
    desc: "跳过所有权限检查 · 仅在受信任环境使用",
    icon: ShieldOff,
    color: "var(--color-error)",
  },
  {
    value: "plan",
    label: "计划模式",
    desc: "只规划，不执行工具",
    icon: FileSearch,
    color: "var(--color-warning)",
  },
] as const;

/**
 * Resolve the display config for a given permission mode.
 *
 * Always returns a defined value — unknown modes fall back to the first
 * entry (`"default"`) so callers never have to handle `undefined`.
 */
export function getPermissionConfig(mode: PermissionMode): PermissionModeConfig {
  return PERMISSION_MODES.find((m) => m.value === mode) ?? PERMISSION_MODES[0];
}

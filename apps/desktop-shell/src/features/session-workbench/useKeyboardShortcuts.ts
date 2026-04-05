/**
 * Global keyboard shortcuts for the session workbench.
 *
 * Mirrors Claude Code CLI keyboard behavior:
 * - Escape      → Stop streaming / close dialogs
 * - Ctrl+L      → Clear messages
 * - Ctrl+N      → New session
 * - Ctrl+K      → Focus input / command palette
 * - Ctrl+,      → Open settings
 * - Ctrl+Shift+S → Toggle sidebar
 * - Ctrl+Shift+E → Export session
 */

import { useEffect, useCallback } from "react";

export interface KeyboardShortcutHandlers {
  onEscape?: () => void;
  onClearMessages?: () => void;
  onNewSession?: () => void;
  onFocusInput?: () => void;
  onOpenSettings?: () => void;
  onToggleSidebar?: () => void;
  onExportSession?: () => void;
  onToggleAgentPanel?: () => void;
}

interface ShortcutDef {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  handler: keyof KeyboardShortcutHandlers;
  description: string;
}

const SHORTCUTS: ShortcutDef[] = [
  {
    key: "Escape",
    handler: "onEscape",
    description: "Stop streaming / close dialogs",
  },
  {
    key: "l",
    ctrl: true,
    handler: "onClearMessages",
    description: "Clear messages",
  },
  {
    key: "n",
    ctrl: true,
    handler: "onNewSession",
    description: "New session",
  },
  {
    key: "k",
    ctrl: true,
    handler: "onFocusInput",
    description: "Focus input",
  },
  {
    key: ",",
    ctrl: true,
    handler: "onOpenSettings",
    description: "Open settings",
  },
  {
    key: "S",
    ctrl: true,
    shift: true,
    handler: "onToggleSidebar",
    description: "Toggle sidebar",
  },
  {
    key: "E",
    ctrl: true,
    shift: true,
    handler: "onExportSession",
    description: "Export session",
  },
  {
    key: "B",
    ctrl: true,
    shift: true,
    handler: "onToggleAgentPanel",
    description: "Toggle agent panel",
  },
];

export function useKeyboardShortcuts(handlers: KeyboardShortcutHandlers) {
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Don't intercept when typing in input/textarea (except Escape)
      const target = e.target as HTMLElement;
      const isInput =
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable;

      for (const shortcut of SHORTCUTS) {
        const ctrlMatch = shortcut.ctrl
          ? e.ctrlKey || e.metaKey
          : !e.ctrlKey && !e.metaKey;
        const shiftMatch = shortcut.shift ? e.shiftKey : !e.shiftKey;
        const altMatch = shortcut.alt ? e.altKey : !e.altKey;

        if (e.key === shortcut.key && ctrlMatch && shiftMatch && altMatch) {
          // Allow Escape in inputs, block others
          if (isInput && shortcut.key !== "Escape" && !shortcut.ctrl) {
            continue;
          }

          const fn = handlers[shortcut.handler];
          if (fn) {
            e.preventDefault();
            e.stopPropagation();
            fn();
            return;
          }
        }
      }
    },
    [handlers]
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [handleKeyDown]);
}

/** Returns the list of shortcuts for help display */
export function getShortcutsList(): Array<{
  keys: string;
  description: string;
}> {
  return SHORTCUTS.map((s) => {
    const parts: string[] = [];
    if (s.ctrl) parts.push("Ctrl");
    if (s.shift) parts.push("Shift");
    if (s.alt) parts.push("Alt");
    parts.push(s.key === "Escape" ? "Esc" : s.key.toUpperCase());
    return {
      keys: parts.join("+"),
      description: s.description,
    };
  });
}

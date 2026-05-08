/**
 * SlashCommandPalette — minimal command palette triggered by typing "/"
 * at the start of the Composer input.
 *
 * Follows the CodePilot slash-command pattern with a simplified built-in
 * command set (no SDK skill discovery yet).
 */

import { memo, useCallback, useState, useEffect, useMemo } from "react";
import {
  Trash2,
  Plus,
  Download,
  Minimize2,
  FileSearch,
  Layers3,
  Palette,
  Music2,
  FileText,
  type LucideIcon,
} from "lucide-react";
import {
  ASK_REFLECTION_PROMPTS,
  type AskReflectionPromptId,
} from "./ask-reflection-prompts";

export interface SlashCommand {
  name: string;
  description: string;
  icon: LucideIcon;
  action: string;
  prompt?: string;
}

const REFLECTION_PROMPT_ICONS: Record<AskReflectionPromptId, LucideIcon> = {
  continue: FileSearch,
  safe_archive: Minimize2,
  recurring_themes: Layers3,
  priority_reason: FileSearch,
  brief: FileText,
};

export const SLASH_COMMANDS: SlashCommand[] = [
  { name: "/clear",   description: "清空对话历史",       icon: Trash2,     action: "clear" },
  { name: "/new",     description: "新建对话",           icon: Plus,       action: "new" },
  { name: "/export",  description: "导出为 Markdown",    icon: Download,   action: "export" },
  { name: "/compact", description: "总结并压缩历史",     icon: Minimize2,  action: "compact" },
  { name: "/plan",    description: "切换计划模式",       icon: FileSearch, action: "plan" },
  {
    name: "/cross-domain",
    description: "找出来源 App 和真实用途不一致的素材",
    icon: Layers3,
    action: "prompt",
    prompt: "哪些素材的真实用途和来源 App 不一致？请按 source -> likely use 分组，并引用原始来源。",
  },
  {
    name: "/shopping-inspiration",
    description: "从购物收藏里提炼设计灵感",
    icon: Palette,
    action: "prompt",
    prompt: "这些购物收藏里哪些其实是设计灵感？请保留商品/来源引用，并说明审美共性、可用方向和不确定项。",
  },
  {
    name: "/music-mood",
    description: "从音乐收藏识别情绪和社交线索",
    icon: Music2,
    action: "prompt",
    prompt: "这些音乐相关收藏里，哪些在表达治愈、社交或创作方向？请引用来源，并把证据弱的项标为观察中。",
  },
  {
    name: "/brief",
    description: "把跨界素材提炼成 brief",
    icon: FileText,
    action: "prompt",
    prompt: "把这组跨界素材提炼成一个 brief：主题、审美共性、情绪线索、优先级、冷却/归档候选，以及下一步行动。",
  },
  ...ASK_REFLECTION_PROMPTS.map(
    (prompt): SlashCommand => ({
      name: prompt.slashName,
      description: prompt.description,
      icon: REFLECTION_PROMPT_ICONS[prompt.id],
      action: "prompt",
      prompt: prompt.prompt,
    })
  ),
];

interface SlashCommandPaletteProps {
  query: string;
  visible: boolean;
  onSelect: (command: SlashCommand) => void;
  onClose: () => void;
}

export const SlashCommandPalette = memo(function SlashCommandPalette({
  query,
  visible,
  onSelect,
  onClose,
}: SlashCommandPaletteProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);

  const filtered = useMemo(() => {
    const q = query.toLowerCase();
    return SLASH_COMMANDS.filter(
      (cmd) =>
        cmd.name.toLowerCase().includes(q) ||
        cmd.description.toLowerCase().includes(q)
    );
  }, [query]);

  // Reset selection when query changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!visible || filtered.length === 0) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopPropagation();
        setSelectedIndex((prev) => (prev + 1) % filtered.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        setSelectedIndex((prev) =>
          (prev - 1 + filtered.length) % filtered.length
        );
      } else if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        e.stopPropagation();
        onSelect(filtered[selectedIndex]);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    },
    [visible, filtered, selectedIndex, onSelect, onClose]
  );

  useEffect(() => {
    if (visible) {
      document.addEventListener("keydown", handleKeyDown, true);
      return () => document.removeEventListener("keydown", handleKeyDown, true);
    }
  }, [visible, handleKeyDown]);

  if (!visible || filtered.length === 0) return null;

  return (
    <div className="absolute bottom-full left-0 z-50 mb-2 w-[280px] animate-slide-up rounded-lg border border-border bg-popover py-1 shadow-[var(--deeptutor-shadow-md,0_4px_12px_-2px_rgba(0,0,0,0.1))]">
      <div className="px-2.5 py-1 text-caption font-semibold uppercase tracking-wider text-muted-foreground">
        命令
      </div>
      {filtered.map((cmd, idx) => {
        const Icon = cmd.icon;
        const isActive = idx === selectedIndex;
        return (
          <button
            key={cmd.name}
            className={`flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left transition-colors ${
              isActive
                ? "bg-[color:var(--deeptutor-primary-soft,var(--color-accent))] text-foreground"
                : "text-foreground hover:bg-accent/50"
            }`}
            onClick={() => onSelect(cmd)}
            onMouseEnter={() => setSelectedIndex(idx)}
          >
            <Icon
              className="size-3.5 shrink-0"
              style={{ color: isActive ? "var(--deeptutor-primary, var(--claude-orange))" : undefined }}
            />
            <div className="min-w-0 flex-1">
              <div className="text-body-sm font-medium">{cmd.name}</div>
              <div className="text-caption text-muted-foreground">
                {cmd.description}
              </div>
            </div>
          </button>
        );
      })}
    </div>
  );
});

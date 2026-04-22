/**
 * QuickActionsBar — shortcut buttons for common SKILL actions.
 * Per 03-chat-tab.md §4 (Quick Action definitions) and §6 (QuickActionsBar).
 *
 * Shown when the session has no messages (empty welcome state).
 * 2x2 grid of action buttons; click → prefill Composer or send directly.
 *
 * DS1.5: card face only shows one-line action label (+ Lucide icon).
 * The prompt template that fires on click is NOT rendered as an inline
 * description — that was a capability-matrix read on DS1. The full
 * template is carried through `promptTemplate` but only surfaces via
 * native `title` hover when the user asks.
 */

import { useState } from "react";
import { Link, Search, Clock, BarChart3, ArrowRight } from "lucide-react";

interface QuickAction {
  id: string;
  label: string;
  icon: React.ReactNode;
  promptTemplate: string;
  requiresInput: boolean;
  inputPlaceholder?: string;
}

const QUICK_ACTIONS: QuickAction[] = [
  {
    id: "feed-url",
    label: "投喂URL",
    icon: <Link className="size-5" />,
    promptTemplate: "请摄入这个URL的内容: {url}",
    requiresInput: true,
    inputPlaceholder: "粘贴 URL...",
  },
  {
    id: "query-wiki",
    label: "查询知识",
    icon: <Search className="size-5" />,
    promptTemplate: "/query {question}",
    requiresInput: true,
    inputPlaceholder: "输入问题...",
  },
  {
    id: "recent-ingest",
    label: "最近摄入",
    icon: <Clock className="size-5" />,
    promptTemplate: "列出最近 7 天摄入的所有素材和生成的 wiki 页面",
    requiresInput: false,
  },
  {
    id: "wiki-stats",
    label: "知识统计",
    icon: <BarChart3 className="size-5" />,
    promptTemplate: "统计当前知识库的整体情况: raw 数量、wiki 页面数量、各分类分布、最近活跃页面",
    requiresInput: false,
  },
];

interface QuickActionsBarProps {
  onAction: (prompt: string) => void;
  visible: boolean;
}

export function QuickActionsBar({ onAction, visible }: QuickActionsBarProps) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");

  if (!visible) return null;

  const handleActionClick = (action: QuickAction) => {
    if (action.requiresInput) {
      setExpandedId(expandedId === action.id ? null : action.id);
      setInputValue("");
    } else {
      onAction(action.promptTemplate);
    }
  };

  const handleInputSubmit = (action: QuickAction) => {
    if (!inputValue.trim()) return;
    const prompt = action.promptTemplate.replace(
      /\{[^}]+\}/,
      inputValue.trim(),
    );
    onAction(prompt);
    setExpandedId(null);
    setInputValue("");
  };

  return (
    <div className="mx-auto max-w-md px-4 py-8">
      <h3 className="mb-4 text-center text-[14px] font-medium text-[var(--color-muted-foreground)]">
        快捷操作
      </h3>
      <div className="grid grid-cols-2 gap-3">
        {QUICK_ACTIONS.map((action) => (
          <div key={action.id}>
            <button
              onClick={() => handleActionClick(action)}
              title={action.promptTemplate}
              className="flex w-full flex-col items-center gap-2 rounded-xl border border-[var(--color-border)] bg-[var(--color-card)] p-4 text-[var(--color-foreground)] transition-all hover:scale-[1.02] hover:border-[var(--color-primary)]/30 hover:shadow-sm"
            >
              <span className="text-[var(--color-primary)]">{action.icon}</span>
              <span className="text-[12px] font-medium">{action.label}</span>
            </button>

            {/* Inline input for actions that require user input */}
            {expandedId === action.id && action.requiresInput && (
              <div className="mt-2 flex items-center gap-1.5">
                <input
                  type="text"
                  value={inputValue}
                  onChange={(e) => setInputValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleInputSubmit(action);
                  }}
                  placeholder={action.inputPlaceholder}
                  className="h-8 flex-1 rounded-lg border border-[var(--color-border)] bg-[var(--color-background)] px-2.5 text-[12px] text-[var(--color-foreground)] placeholder:text-[var(--color-muted-foreground)] outline-none focus:border-[var(--color-primary)]"
                  autoFocus
                />
                <button
                  onClick={() => handleInputSubmit(action)}
                  className="flex size-8 items-center justify-center rounded-lg bg-[var(--color-primary)] text-white hover:opacity-90"
                >
                  <ArrowRight className="size-3.5" />
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

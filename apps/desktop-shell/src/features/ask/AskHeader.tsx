/**
 * AskHeader - compact conversation header for the Ask workspace.
 *
 * Model / environment badges are deliberately omitted here; the composer
 * owns that control surface so the page does not repeat status metadata.
 */

import { Loader2, Settings, Share2 } from "lucide-react";

interface AskHeaderProps {
  title?: string;
  projectPath?: string;
  modelLabel?: string;
  environmentLabel?: string;
  isStreaming?: boolean;
  agentCount?: number;
  showAgentPanel?: boolean;
  onToggleAgentPanel?: () => void;
  onExportMarkdown?: () => void;
  onExportJson?: () => void;
}

export function AskHeader({
  title,
  projectPath,
  isStreaming = false,
}: AskHeaderProps) {
  const trimmedTitle = title?.trim();
  const displayTitle = trimmedTitle && trimmedTitle.length > 0 ? trimmedTitle : "未命名";

  return (
    <header className="ask-chat-header">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="text-[13px] text-[#5F5E5A]">新对话</span>
          <span className="text-[12px] text-[#888780]">·</span>
          <span className="min-w-0 truncate text-[12px] text-[#888780]">
            {displayTitle}
          </span>
        </div>
        {projectPath && (
          <p className="mt-0.5 max-w-[420px] truncate text-[11px] text-[#888780]">
            {projectPath}
          </p>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {isStreaming && (
          <span className="mr-1 inline-flex items-center gap-1.5 rounded-full bg-[#FAECE7] px-2 py-1 text-[11px] text-[#D85A30]">
            <Loader2 className="size-3 animate-spin" />
            正在回答
          </span>
        )}
        <button
          type="button"
          className="ask-chat-header-button"
          onClick={() => {
            void navigator.clipboard?.writeText(window.location.href);
          }}
          title="复制当前对话链接"
          aria-label="复制当前对话链接"
        >
          <Share2 className="size-3.5" />
        </button>
        <button
          type="button"
          className="ask-chat-header-button"
          onClick={() => {
            window.location.hash = "#/settings";
          }}
          title="打开设置"
          aria-label="打开设置"
        >
          <Settings className="size-3.5" />
        </button>
      </div>
    </header>
  );
}

/**
 * AskHeader - compact conversation header for the Ask workspace.
 *
 * Model / environment badges are deliberately omitted here; the composer
 * owns that control surface so the page does not repeat status metadata.
 */

import { Settings, Share2 } from "lucide-react";
import type { ConversationTurnStatus } from "./useConversationTurnState";

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
  turnStatus?: ConversationTurnStatus;
}

export function AskHeader({
  title,
  projectPath,
  isStreaming = false,
  turnStatus,
}: AskHeaderProps) {
  const trimmedTitle = title?.trim();
  const displayTitle = trimmedTitle && trimmedTitle.length > 0 ? trimmedTitle : "未命名";
  const fallbackLabel = isStreaming ? "回答中" : `新对话 · ${displayTitle}`;
  const fallbackTone = isStreaming ? "working" : "idle";
  const statusLabel = turnStatus?.label ?? fallbackLabel;
  const statusTone = turnStatus?.tone ?? fallbackTone;
  const metricsLabel = turnStatus?.metricsLabel ?? "";

  return (
    <header className="ask-chat-header">
      <div className="min-w-0">
        <div className="ask-chat-header-status">
          <span
            className="ask-chat-status-dot"
            data-tone={statusTone}
            aria-hidden
          />
          <span className="min-w-0 truncate text-[13px] text-[#5F5E5A]">
            {statusLabel}
          </span>
        </div>
        {turnStatus?.detail ? (
          <p className="mt-0.5 max-w-[520px] truncate text-[11px] text-[#888780]">
            {turnStatus.detail}
          </p>
        ) : projectPath ? (
          <p className="mt-0.5 max-w-[420px] truncate text-[11px] text-[#888780]">
            {projectPath}
          </p>
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {metricsLabel && (
          <span className="ask-chat-header-metrics">
            {metricsLabel}
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

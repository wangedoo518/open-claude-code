import { AlertTriangle, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { cn } from "@/lib/utils";
import type { ModelCapability } from "./model-capabilities";

const SEARCH_INTENT_KEYWORDS = [
  "搜一下",
  "查一下",
  "查找",
  "帮我找",
  "联网",
  "最新",
  "最近",
  "上网",
  "搜索网页",
  "帮我搜",
];

const DISMISS_MS = 30 * 60 * 1000;

interface Props {
  modelLabel: string;
  modelId: string;
  capability: ModelCapability;
  inputValue: string;
  onSwitchToAnthropic?: () => void;
  hasOtherHint?: boolean;
  className?: string;
}

export function CapabilityHint({
  modelLabel,
  modelId,
  capability,
  inputValue,
  onSwitchToAnthropic,
  hasOtherHint = false,
  className,
}: Props) {
  const storageKey = `clawwiki:ask:capability-hint-dismissed:${modelId || "unknown"}`;
  const [dismissedUntil, setDismissedUntil] = useState(0);

  useEffect(() => {
    const raw = window.sessionStorage.getItem(storageKey);
    setDismissedUntil(raw ? Number(raw) || 0 : 0);
  }, [storageKey]);

  const hasSearchIntent = useMemo(() => {
    const normalized = inputValue.trim();
    return SEARCH_INTENT_KEYWORDS.some((keyword) =>
      normalized.includes(keyword),
    );
  }, [inputValue]);

  const shouldShow =
    hasSearchIntent &&
    !capability.tools &&
    !hasOtherHint &&
    Date.now() > dismissedUntil;

  if (!shouldShow) {
    return null;
  }

  function dismiss() {
    const until = Date.now() + DISMISS_MS;
    window.sessionStorage.setItem(storageKey, String(until));
    setDismissedUntil(until);
  }

  return (
    <div
      className={cn(
        "mb-1.5 flex items-center gap-2 rounded-lg border border-[rgba(186,117,23,0.14)] bg-[rgba(186,117,23,0.05)] px-3 py-2 text-[12px] text-[#5F5E5A]",
        className,
      )}
    >
      <div className="h-8 w-0.5 shrink-0 rounded-full bg-[#BA7517]" />
      <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-[#BA7517]" />
      <div className="min-w-0 flex-1 leading-relaxed">
        当前模型「{modelLabel}」不支持工具调用，无法联网搜索。
        <button
          type="button"
          className="ml-2 font-medium text-[#2C2C2A] underline decoration-[rgba(44,44,42,0.2)] underline-offset-2 hover:text-[#D85A30]"
          onClick={onSwitchToAnthropic}
        >
          切换到 Anthropic →
        </button>
      </div>
      <button
        type="button"
        className="shrink-0 rounded p-1 text-[#888780] transition-colors hover:bg-[rgba(44,44,42,0.06)] hover:text-[#2C2C2A]"
        aria-label="关闭能力提示"
        onClick={dismiss}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

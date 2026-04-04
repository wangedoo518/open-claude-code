/**
 * RealtimeLogCard — Terminal-style log display
 *
 * Port from clawhub123/src/v2/shared/components/RealtimeLogCard.tsx
 * Converted from Ant Design + BEM CSS to Tailwind + shadcn/ui
 */

import { useEffect, useRef, useState } from "react";
import { RefreshCw, Trash2, Terminal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface RealtimeLogCardProps {
  lines: string[];
  title?: string;
  emptyText?: string;
  height?: number;
  lineColor?: (line: string) => string;
  onRefresh?: () => void;
  onClear?: () => void;
}

export function RealtimeLogCard({
  lines,
  title = "实时日志",
  emptyText = "暂无日志",
  height = 260,
  lineColor,
  onRefresh,
  onClear,
}: RealtimeLogCardProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  // Auto-scroll to bottom when new lines arrive
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines, autoScroll]);

  // Detect manual scroll to disable auto-scroll
  const handleScroll = () => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 40;
    setAutoScroll(isAtBottom);
  };

  const resolveLineColor = (line: string): string => {
    if (lineColor) return lineColor(line);
    if (line.includes("[stderr]") || line.includes("error") || line.includes("Error")) {
      return "#ff7875";
    }
    return "#52c41a";
  };

  return (
    <div className="flex flex-col rounded-xl overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-[#1a1a1a] border-b border-white/5">
        <div className="flex items-center gap-2 text-xs text-[#a0a0a0]">
          <Terminal className="size-3.5" />
          <span>{title}</span>
          {lines.length > 0 && (
            <span className="text-[#666]">({lines.length} 行)</span>
          )}
        </div>
        <div className="flex items-center gap-1">
          {onRefresh && (
            <Button
              variant="ghost"
              size="icon"
              className="size-6 text-[#666] hover:text-[#a0a0a0] hover:bg-white/5"
              onClick={onRefresh}
            >
              <RefreshCw className="size-3" />
            </Button>
          )}
          {onClear && lines.length > 0 && (
            <Button
              variant="ghost"
              size="icon"
              className="size-6 text-[#666] hover:text-[#a0a0a0] hover:bg-white/5"
              onClick={onClear}
            >
              <Trash2 className="size-3" />
            </Button>
          )}
        </div>
      </div>

      {/* Log content */}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        style={{ height }}
        className="overflow-auto bg-[#141414] px-4 py-3 font-mono text-xs leading-[1.85]"
      >
        {lines.length === 0 ? (
          <div className="flex items-center justify-center h-full text-[#555]">
            {emptyText}
          </div>
        ) : (
          lines.map((line, index) => (
            <div
              key={index}
              className={cn(
                "whitespace-pre-wrap break-all",
                "hover:bg-white/[0.03] rounded px-1 -mx-1"
              )}
              style={{ color: resolveLineColor(line) }}
            >
              {line}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

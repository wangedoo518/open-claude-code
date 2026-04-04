import { useCallback, useRef, useState, type KeyboardEvent } from "react";
import { SendHorizonal, Square, Shield, Monitor } from "lucide-react";
import { cn } from "@/lib/utils";

interface InputBarProps {
  onSend: (message: string) => void | Promise<void>;
  onStop?: () => void;
  isBusy?: boolean;
  permissionModeLabel?: string;
  environmentLabel?: string;
}

/**
 * Input bar — Claude Code desktop style.
 *
 * Layout:
 *   ┌──────────────────────────────────────────┐
 *   │ Describe the next step...                │
 *   │                                          │
 *   └──────────────────────────────────────────┘
 *   [Ask permissions]  [Local]         [Send ●] / [Stop ■]
 *
 * - Larger standalone textarea with rounded border
 * - Bottom row: permission + env buttons left, Send/Stop right
 * - Send uses Claude Orange; Stop uses destructive red
 * - No "Shift+Enter" hint text
 */
export function InputBar({
  onSend,
  onStop,
  isBusy = false,
  permissionModeLabel = "Ask permissions",
  environmentLabel = "Local",
}: InputBarProps) {
  const [value, setValue] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSend = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || isBusy) return;
    void onSend(trimmed);
    setValue("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [value, isBusy, onSend]);

  const handleStop = useCallback(() => {
    onStop?.();
  }, [onStop]);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleInput = () => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = Math.min(textarea.scrollHeight, 200) + "px";
  };

  return (
    <div className="border-t border-border/50 bg-background px-4 py-3">
      {/* Textarea — standalone rounded card */}
      <div
        className={cn(
          "rounded-xl border border-input bg-muted/10 px-4 py-3 transition-colors",
          "focus-within:border-ring focus-within:ring-1 focus-within:ring-ring/50"
        )}
      >
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          onInput={handleInput}
          placeholder={
            isBusy
              ? "Waiting for response..."
              : "Describe the next step for this desktop implementation..."
          }
          disabled={isBusy}
          rows={2}
          className="max-h-[200px] min-h-[48px] w-full resize-none bg-transparent text-sm leading-relaxed text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-50"
        />
      </div>

      {/* Bottom button row */}
      <div className="mt-2 flex items-center justify-between">
        {/* Left: permission + environment */}
        <div className="flex items-center gap-2">
          <button className="flex items-center gap-1.5 rounded-lg border border-border/50 px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground">
            <Shield className="size-3" />
            <span>{permissionModeLabel}</span>
          </button>
          <button className="flex items-center gap-1.5 rounded-lg border border-border/50 px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground">
            <Monitor className="size-3" />
            <span>{environmentLabel}</span>
          </button>
        </div>

        {/* Right: Send or Stop */}
        {isBusy ? (
          <button
            className="flex items-center gap-1.5 rounded-lg bg-destructive px-3 py-1.5 text-xs font-medium text-destructive-foreground transition-colors hover:bg-destructive/90"
            onClick={handleStop}
          >
            <Square className="size-3" />
            <span>Stop</span>
          </button>
        ) : (
          <button
            className={cn(
              "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium text-white transition-colors",
              value.trim()
                ? "cursor-pointer"
                : "cursor-not-allowed opacity-50"
            )}
            style={{ backgroundColor: "var(--claude-orange, rgb(215,119,87))" }}
            onClick={handleSend}
            disabled={!value.trim()}
          >
            <SendHorizonal className="size-3" />
            <span>Send</span>
          </button>
        )}
      </div>
    </div>
  );
}

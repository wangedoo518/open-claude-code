import { useCallback, useRef, useState, type KeyboardEvent } from "react";
import { SendHorizonal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface InputBarProps {
  onSend: (message: string) => void | Promise<void>;
  isBusy?: boolean;
}

export function InputBar({ onSend, isBusy = false }: InputBarProps) {
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
    <div className="border-t border-border bg-background px-4 py-3">
      <div
        className={cn(
          "flex items-end gap-2 rounded-lg border border-input bg-muted/20 px-3 py-2 transition-colors",
          "focus-within:border-ring focus-within:ring-1 focus-within:ring-ring"
        )}
      >
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          onInput={handleInput}
          placeholder={isBusy ? "Waiting for response..." : "Type your message..."}
          disabled={isBusy}
          rows={1}
          className="max-h-[200px] min-h-[24px] flex-1 resize-none bg-transparent font-mono text-sm text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-50"
        />
        <Button
          variant="ghost"
          size="icon"
          className="size-8 shrink-0"
          onClick={handleSend}
          disabled={!value.trim() || isBusy}
        >
          <SendHorizonal className="size-4" />
        </Button>
      </div>
      <div className="mt-1 flex items-center justify-between px-1">
        <span className="text-[10px] text-muted-foreground">
          Shift+Enter for new line
        </span>
        <span className="text-[10px] text-muted-foreground">
          Enter to send
        </span>
      </div>
    </div>
  );
}

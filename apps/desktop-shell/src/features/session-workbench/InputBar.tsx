import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import {
  SendHorizonal,
  Square,
  Shield,
  ShieldCheck,
  ShieldOff,
  FileSearch,
  Monitor,
  Slash,
  ChevronDown,
} from "lucide-react";
import { cn } from "@/lib/utils";
import {
  useSettingsStore,
  type PermissionMode,
} from "@/state/settings-store";

/* ─── Permission mode config ────────────────────────────────────── */

const PERMISSION_MODES: {
  value: PermissionMode;
  label: string;
  desc: string;
  icon: typeof Shield;
  color?: string;
}[] = [
  {
    value: "default",
    label: "Ask permissions",
    desc: "Dangerous operations require confirmation",
    icon: Shield,
  },
  {
    value: "acceptEdits",
    label: "Accept edits",
    desc: "Auto-accept file edits, ask for others",
    icon: ShieldCheck,
    color: "var(--color-success)",
  },
  {
    value: "bypassPermissions",
    label: "Bypass permissions",
    desc: "Skip all permission checks",
    icon: ShieldOff,
    color: "var(--color-error)",
  },
  {
    value: "plan",
    label: "Plan mode",
    desc: "Plan only, don't execute tools",
    icon: FileSearch,
    color: "var(--color-warning)",
  },
];

function getPermissionConfig(mode: PermissionMode) {
  return PERMISSION_MODES.find((m) => m.value === mode) ?? PERMISSION_MODES[0];
}

/* ─── Slash commands ─────────────────────────────────────────────── */

const SLASH_COMMANDS = [
  { name: "help", desc: "Get help with using Claude Code" },
  { name: "clear", desc: "Clear conversation history" },
  { name: "commit", desc: "Commit code changes" },
  { name: "compact", desc: "Compact conversation to save context" },
  { name: "config", desc: "Open configuration" },
  { name: "cost", desc: "Show token usage and costs" },
  { name: "diff", desc: "Show file changes in this session" },
  { name: "init", desc: "Initialize CLAUDE.md in this project" },
  { name: "model", desc: "Switch AI model" },
  { name: "permissions", desc: "View and manage permissions" },
  { name: "review", desc: "Review code changes" },
  { name: "session", desc: "Show session information" },
  { name: "status", desc: "Show session status" },
  { name: "theme", desc: "Switch theme" },
] as const;

interface InputBarProps {
  onSend: (message: string) => void | Promise<void>;
  onStop?: () => void;
  onSlashCommand?: (input: string) => boolean;
  isBusy?: boolean;
  environmentLabel?: string;
  inputRef?: React.RefObject<HTMLTextAreaElement | null>;
}

export function InputBar({
  onSend,
  onStop,
  onSlashCommand,
  isBusy = false,
  environmentLabel = "Local",
  inputRef,
}: InputBarProps) {
  const permissionMode = useSettingsStore((state) => state.permissionMode);
  const setPermissionMode = useSettingsStore((state) => state.setPermissionMode);
  const modeConfig = getPermissionConfig(permissionMode);

  const [value, setValue] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [showCommands, setShowCommands] = useState(false);
  const [commandFilter, setCommandFilter] = useState("");
  const [selectedCommandIndex, setSelectedCommandIndex] = useState(0);
  const [showPermissionMenu, setShowPermissionMenu] = useState(false);
  const internalRef = useRef<HTMLTextAreaElement>(null);
  const textareaRef = inputRef ?? internalRef;
  const commandListRef = useRef<HTMLDivElement>(null);
  const permMenuRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);

  // Filter slash commands based on input
  const filteredCommands = SLASH_COMMANDS.filter((cmd) =>
    cmd.name.toLowerCase().includes(commandFilter.toLowerCase())
  );

  // Detect slash command input
  useEffect(() => {
    if (value.startsWith("/")) {
      const filter = value.slice(1);
      setCommandFilter(filter);
      setShowCommands(true);
      setSelectedCommandIndex(0);
    } else {
      setShowCommands(false);
      setCommandFilter("");
    }
  }, [value]);

  // Close permission menu on outside click
  useEffect(() => {
    if (!showPermissionMenu) return;
    const handler = (e: MouseEvent) => {
      if (
        permMenuRef.current &&
        !permMenuRef.current.contains(e.target as Node)
      ) {
        setShowPermissionMenu(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [showPermissionMenu]);

  const handleSend = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || isBusy) return;
    // Save to history
    setHistory((prev) => [trimmed, ...prev.slice(0, 49)]);
    setHistoryIndex(-1);

    // Check for slash commands first
    if (trimmed.startsWith("/") && onSlashCommand?.(trimmed)) {
      setValue("");
      if (textareaRef.current) {
        textareaRef.current.style.height = "auto";
      }
      return;
    }

    void onSend(trimmed);
    setValue("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [value, isBusy, onSend, onSlashCommand]);

  const handleStop = useCallback(() => {
    onStop?.();
  }, [onStop]);

  const selectCommand = useCallback(
    (cmdName: string) => {
      const fullCommand = `/${cmdName}`;
      setShowCommands(false);
      setValue("");
      setHistory((prev) => [fullCommand, ...prev.slice(0, 49)]);

      // Try slash command handler first
      if (onSlashCommand?.(fullCommand)) {
        textareaRef.current?.focus();
        return;
      }

      // Fallback: send as message
      void onSend(fullCommand);
      textareaRef.current?.focus();
    },
    [onSend, onSlashCommand]
  );

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // Command palette navigation
    if (showCommands && filteredCommands.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedCommandIndex((i) =>
          Math.min(i + 1, filteredCommands.length - 1)
        );
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedCommandIndex((i) => Math.max(i - 1, 0));
        return;
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey)) {
        e.preventDefault();
        selectCommand(filteredCommands[selectedCommandIndex].name);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setShowCommands(false);
        return;
      }
    }

    // History navigation (only when no command palette)
    if (!showCommands && e.key === "ArrowUp" && !e.shiftKey && value === "") {
      e.preventDefault();
      if (history.length > 0) {
        const newIndex = Math.min(historyIndex + 1, history.length - 1);
        setHistoryIndex(newIndex);
        setValue(history[newIndex]);
      }
      return;
    }
    if (!showCommands && e.key === "ArrowDown" && !e.shiftKey && historyIndex >= 0) {
      e.preventDefault();
      const newIndex = historyIndex - 1;
      if (newIndex < 0) {
        setHistoryIndex(-1);
        setValue("");
      } else {
        setHistoryIndex(newIndex);
        setValue(history[newIndex]);
      }
      return;
    }

    // Send on Enter
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }

    // Clear on Escape
    if (e.key === "Escape" && value) {
      e.preventDefault();
      setValue("");
    }
  };

  // Batch height calculation into next animation frame to avoid layout thrashing
  const handleInput = () => {
    cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (!textarea) return;
      textarea.style.height = "auto";
      textarea.style.height = Math.min(textarea.scrollHeight, 200) + "px";
    });
  };

  // Cleanup pending rAF on unmount
  useEffect(() => {
    return () => cancelAnimationFrame(rafRef.current);
  }, []);

  const ModeIcon = modeConfig.icon;

  return (
    <div className="relative border-t border-border/50 bg-background px-4 py-3">
      {/* Slash command palette */}
      {showCommands && filteredCommands.length > 0 && (
        <div
          ref={commandListRef}
          className="absolute bottom-full left-4 right-4 mb-1 max-h-[240px] overflow-y-auto rounded-lg border border-border bg-popover shadow-lg"
        >
          <div className="px-2 py-1.5">
            <div className="px-2 pb-1 text-caption font-semibold uppercase tracking-wider text-muted-foreground">
              Commands
            </div>
            {filteredCommands.map((cmd, i) => (
              <button
                key={cmd.name}
                className={cn(
                  "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors",
                  i === selectedCommandIndex
                    ? "bg-accent text-accent-foreground"
                    : "text-foreground hover:bg-accent/50"
                )}
                onClick={() => selectCommand(cmd.name)}
                onMouseEnter={() => setSelectedCommandIndex(i)}
              >
                <Slash className="size-3 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="text-body-sm font-medium">{cmd.name}</div>
                  <div className="text-label text-muted-foreground">{cmd.desc}</div>
                </div>
                {i === selectedCommandIndex && (
                  <kbd className="rounded border border-border/50 bg-muted/30 px-1 py-0.5 text-nano text-muted-foreground">
                    Enter
                  </kbd>
                )}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Textarea */}
      <div
        className={cn(
          "rounded-xl border border-input bg-muted/10 px-4 py-2.5 transition-colors",
          "focus-within:border-ring focus-within:ring-1 focus-within:ring-ring/50"
        )}
      >
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          onInput={handleInput}
          aria-label="Message input"
          placeholder={
            isBusy
              ? "Waiting for response..."
              : permissionMode === "plan"
                ? "Describe what you want to plan..."
                : "Type a message... (/ for commands)"
          }
          disabled={isBusy}
          rows={2}
          className="max-h-[200px] min-h-[44px] w-full resize-none bg-transparent text-body leading-relaxed text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-50"
        />
      </div>

      {/* Bottom button row */}
      <div className="mt-2 flex items-center justify-between">
        {/* Left: permission + environment */}
        <div className="flex items-center gap-2">
          {/* Permission mode selector */}
          <div className="relative" ref={permMenuRef}>
            <button
              className={cn(
                "flex items-center gap-1.5 rounded-lg border border-border/50 px-2 py-1 text-label transition-colors hover:bg-accent hover:text-foreground",
                showPermissionMenu
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground"
              )}
              style={modeConfig.color ? { color: modeConfig.color } : undefined}
              onClick={() => setShowPermissionMenu((prev) => !prev)}
              aria-label={`Permission mode: ${modeConfig.label}`}
              aria-expanded={showPermissionMenu}
            >
              <ModeIcon className="size-3" />
              <span>{modeConfig.label}</span>
              <ChevronDown className="size-2.5 opacity-50" />
            </button>

            {/* Permission mode dropdown */}
            {showPermissionMenu && (
              <div className="absolute bottom-full left-0 mb-1 w-[260px] rounded-lg border border-border bg-popover p-1 shadow-lg">
                <div className="px-2 pb-1 pt-1 text-caption font-semibold uppercase tracking-wider text-muted-foreground">
                  Permission Mode
                </div>
                {PERMISSION_MODES.map((mode) => {
                  const Icon = mode.icon;
                  const isActive = permissionMode === mode.value;
                  return (
                    <button
                      key={mode.value}
                      className={cn(
                        "flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left transition-colors",
                        isActive
                          ? "bg-accent text-accent-foreground"
                          : "text-foreground hover:bg-accent/50"
                      )}
                      onClick={() => {
                        setPermissionMode(mode.value);
                        setShowPermissionMenu(false);
                      }}
                    >
                      <Icon
                        className="size-3.5 shrink-0"
                        style={mode.color ? { color: mode.color } : undefined}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="text-body-sm font-medium">
                          {mode.label}
                        </div>
                        <div className="text-caption text-muted-foreground">
                          {mode.desc}
                        </div>
                      </div>
                      {isActive && (
                        <div
                          className="size-1.5 shrink-0 rounded-full"
                          style={{
                            backgroundColor:
                              mode.color ??
                              "var(--claude-orange)",
                          }}
                        />
                      )}
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          <button
            className="flex items-center gap-1.5 rounded-lg border border-border/50 px-2 py-1 text-label text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            aria-label={`Environment: ${environmentLabel}`}
          >
            <Monitor className="size-3" />
            <span>{environmentLabel}</span>
          </button>
        </div>

        {/* Right: Send or Stop */}
        {isBusy ? (
          <button
            className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-label font-medium text-white transition-colors"
            style={{
              backgroundColor: "var(--color-error)",
            }}
            onClick={handleStop}
          >
            <Square className="size-3" />
            <span>Stop</span>
          </button>
        ) : (
          <button
            className={cn(
              "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-label font-medium text-white transition-colors",
              value.trim()
                ? "cursor-pointer"
                : "cursor-not-allowed opacity-50"
            )}
            style={{ backgroundColor: "var(--claude-orange)" }}
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

export { PERMISSION_MODES, getPermissionConfig };

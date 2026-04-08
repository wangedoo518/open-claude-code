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
  Paperclip,
  X as XIcon,
  FileText,
  Image as ImageIcon,
  AlertCircle,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { sanitizeFilename } from "@/lib/security";
import {
  useSettingsStore,
  type PermissionMode,
} from "@/state/settings-store";

/* ─── Attachment helpers ─────────────────────────────────────────── */

interface ProcessedAttachment {
  filename: string;
  content: string;
  truncated: boolean;
  kind: "text" | "image_base64" | "binary_stub";
  byte_size: number;
}

/** Read a File object as base64 (no `data:` prefix). */
function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // result is "data:<mime>;base64,<b64>" — strip everything before the comma
      const commaIdx = result.indexOf(",");
      resolve(commaIdx >= 0 ? result.slice(commaIdx + 1) : result);
    };
    reader.onerror = () => reject(reader.error ?? new Error("FileReader failed"));
    reader.readAsDataURL(file);
  });
}

async function uploadAttachment(file: File): Promise<ProcessedAttachment> {
  const base64 = await readFileAsBase64(file);
  const response = await fetch("/api/desktop/attachments/process", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ filename: file.name, base64 }),
  });
  if (!response.ok) {
    throw new Error(`upload failed: ${response.status} ${response.statusText}`);
  }
  return (await response.json()) as ProcessedAttachment;
}

/* ─── Attachment validation (CR-02) ─────────────────────────────── */

/** Hard size cap in bytes — keep in sync with backend (15 MiB body limit). */
const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;

/**
 * MIME types accepted for upload. Restricting to a whitelist avoids silently
 * forwarding arbitrary binaries (executables, archives) to the backend tool
 * layer where they would be treated as text or trigger stub handling.
 */
const ALLOWED_MIME_TYPES: ReadonlySet<string> = new Set([
  // Text
  "text/plain",
  "text/markdown",
  "text/csv",
  "text/x-log",
  "text/tab-separated-values",
  // Structured
  "application/json",
  "application/xml",
  "application/x-yaml",
  "application/yaml",
  // Source code files often arrive with these common types
  "application/javascript",
  "application/typescript",
  "application/x-sh",
  // Images
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
  "image/svg+xml",
  // Docs
  "application/pdf",
]);

/**
 * Extensions whose content is safe to treat as text-like even when the
 * browser reports an empty MIME type (common on Windows for newer code
 * file extensions). Used as a fallback when `file.type === ""`.
 */
const ALLOWED_TEXT_EXTENSIONS: ReadonlySet<string> = new Set([
  "txt", "md", "log", "json", "yaml", "yml", "xml", "csv", "tsv",
  "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "rb", "sh",
  "toml", "ini", "cfg", "conf", "env", "gitignore",
  "html", "css", "scss", "vue", "svelte",
  "c", "cpp", "h", "hpp", "cs", "swift", "kt", "dart", "php",
]);

/**
 * Validate a file before upload. Returns `null` if the file is acceptable,
 * or a human-readable error string explaining why it was rejected.
 *
 * Rejection reasons:
 *  - Size > MAX_ATTACHMENT_BYTES
 *  - MIME not in ALLOWED_MIME_TYPES and extension not in ALLOWED_TEXT_EXTENSIONS
 *  - Looks like a directory drop (size 0 AND no extension)
 */
function validateAttachment(file: File): string | null {
  if (file.size > MAX_ATTACHMENT_BYTES) {
    const mb = (file.size / 1024 / 1024).toFixed(1);
    return `${file.name} is ${mb} MB — exceeds 10 MB limit`;
  }

  // Heuristic: folders dropped via dataTransfer.files appear as 0-byte
  // entries with no file extension. Reject these proactively.
  if (file.size === 0 && !file.name.includes(".")) {
    return `${file.name} looks like a folder — only individual files are supported`;
  }

  if (file.type && ALLOWED_MIME_TYPES.has(file.type)) {
    return null;
  }

  // Fallback: accept known text-like extensions when MIME is missing or
  // unrecognized (common for source code files on various OSes).
  const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
  if (ext && ALLOWED_TEXT_EXTENSIONS.has(ext)) {
    return null;
  }

  const mimeLabel = file.type || "unknown type";
  return `${file.name} (${mimeLabel}) is not an allowed file type`;
}

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
  const [attachments, setAttachments] = useState<ProcessedAttachment[]>([]);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isUploading, setIsUploading] = useState(false);
  const internalRef = useRef<HTMLTextAreaElement>(null);
  const textareaRef = inputRef ?? internalRef;
  const commandListRef = useRef<HTMLDivElement>(null);
  const permMenuRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // ── Attachment handling ─────────────────────────────────────────
  const handleFiles = useCallback(async (files: FileList | File[]) => {
    const list = Array.from(files);
    if (list.length === 0) return;
    setIsUploading(true);
    setUploadError(null);

    // CR-02: Validate each file BEFORE attempting upload. Failures are
    // collected so the user sees every rejected file, not just the last.
    const errors: string[] = [];
    const uploaded: ProcessedAttachment[] = [];

    for (const file of list) {
      const rejection = validateAttachment(file);
      if (rejection) {
        errors.push(rejection);
        continue;
      }
      try {
        const result = await uploadAttachment(file);
        uploaded.push(result);
      } catch (e) {
        const msg = e instanceof Error ? e.message : "attachment upload failed";
        errors.push(`${file.name}: ${msg}`);
      }
    }

    if (errors.length > 0) {
      // Show up to 2 errors inline; collapse the rest into a count.
      const preview = errors.slice(0, 2).join("; ");
      const more = errors.length > 2 ? ` (+${errors.length - 2} more)` : "";
      setUploadError(preview + more);
    }

    setAttachments((prev) => [...prev, ...uploaded]);
    setIsUploading(false);
  }, []);

  const removeAttachment = useCallback((index: number) => {
    setAttachments((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer.types.includes("Files")) {
      setIsDragging(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    // Only clear if leaving the input area entirely (not just moving over a child)
    if (e.currentTarget === e.target) {
      setIsDragging(false);
    }
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragging(false);

      // SG-05: Filter out directory entries. The File API does not expose
      // a direct `isDirectory` flag, but folders dragged from the OS come
      // through in one of two ways:
      //   1. With webkitGetAsEntry() returning a directory entry — best
      //      signal, but only works via items not files. We prefer this.
      //   2. As 0-byte File objects whose name has no extension — the
      //      validateAttachment() helper also catches this as a fallback.
      //
      // Using `dataTransfer.items` lets us reject folders before they
      // even enter the upload pipeline.
      const items = e.dataTransfer.items;
      const fileList: File[] = [];
      let rejectedFolders = 0;

      if (items && items.length > 0) {
        for (let i = 0; i < items.length; i++) {
          const item = items[i];
          if (item.kind !== "file") continue;

          // webkitGetAsEntry is the Chrome/Edge/Firefox non-standard API
          // for inspecting drag items as filesystem entries. TS doesn't
          // type it, so we cast through unknown.
          const entry = (
            item as unknown as {
              webkitGetAsEntry?: () => { isDirectory?: boolean } | null;
            }
          ).webkitGetAsEntry?.();

          if (entry?.isDirectory) {
            rejectedFolders++;
            continue;
          }

          const f = item.getAsFile();
          if (f) fileList.push(f);
        }
      } else {
        // Fallback to `files` if items is empty (some older browsers).
        fileList.push(...Array.from(e.dataTransfer.files));
      }

      if (rejectedFolders > 0) {
        setUploadError(
          `${rejectedFolders} folder${rejectedFolders === 1 ? "" : "s"} ignored — only individual files are supported`,
        );
      }

      if (fileList.length > 0) {
        void handleFiles(fileList);
      }
    },
    [handleFiles],
  );

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
    // Allow sending if there's text OR at least one attachment.
    if ((!trimmed && attachments.length === 0) || isBusy) return;
    // Save to history (only the typed text, not the prepended attachments)
    if (trimmed) {
      setHistory((prev) => [trimmed, ...prev.slice(0, 49)]);
      setHistoryIndex(-1);
    }

    // Check for slash commands first (skip attachments for slash commands)
    if (trimmed.startsWith("/") && onSlashCommand?.(trimmed)) {
      setValue("");
      setAttachments([]);
      if (textareaRef.current) {
        textareaRef.current.style.height = "auto";
      }
      return;
    }

    // Build the final message with attachments prepended as a markdown block.
    let finalMessage = trimmed;
    if (attachments.length > 0) {
      const attachmentBlock = attachments
        .map((a) => {
          // CR-03 defense-in-depth: sanitize filenames before including
          // them in LLM context, so RTL overrides can't leak downstream.
          const safeName = sanitizeFilename(a.filename);
          if (a.kind === "image_base64") {
            return `## ${safeName}\n\n[Image attached: ${a.byte_size} bytes]\n`;
          }
          if (a.kind === "binary_stub") {
            return `## ${safeName}\n\n${a.content}\n`;
          }
          // text
          return `## ${safeName}\n\n\`\`\`\n${a.content}\n\`\`\`${a.truncated ? "\n\n_(content was truncated)_" : ""}\n`;
        })
        .join("\n");
      finalMessage = `# Attached files\n\n${attachmentBlock}\n${trimmed}`;
    }

    void onSend(finalMessage);
    setValue("");
    setAttachments([]);
    setUploadError(null);
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [value, attachments, isBusy, onSend, onSlashCommand, textareaRef]);

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

      {/* Hidden file input for the paperclip button */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(e) => {
          if (e.target.files) {
            void handleFiles(e.target.files);
          }
          // Reset so user can re-pick the same file if they removed it
          e.target.value = "";
        }}
      />

      {/* Attachment chips row */}
      {(attachments.length > 0 || uploadError) && (
        <div className="mb-2 flex flex-wrap items-center gap-1.5">
          {attachments.map((att, i) => {
            // CR-03: Sanitize filename before display. Strips RTL override
            // and zero-width chars so e.g. `evil\u202Etxt.exe` cannot be
            // visually disguised as `eviltxt.exe`.
            const safeName = sanitizeFilename(att.filename);
            return (
              <div
                key={`${safeName}-${att.byte_size}-${i}`}
                className="flex items-center gap-1.5 rounded-md border border-border/50 bg-muted/20 px-2 py-1 text-caption"
                title={`${safeName} (${att.byte_size} bytes${att.truncated ? ", truncated" : ""})`}
                dir="ltr"
              >
                {att.kind === "image_base64" ? (
                  <ImageIcon className="size-3" style={{ color: "var(--claude-blue)" }} />
                ) : att.kind === "binary_stub" ? (
                  <AlertCircle className="size-3" style={{ color: "var(--color-warning)" }} />
                ) : (
                  <FileText className="size-3" style={{ color: "var(--muted-foreground)" }} />
                )}
                <span className="max-w-[180px] truncate font-medium" dir="ltr">
                  {safeName}
                </span>
                <button
                  type="button"
                  className="ml-0.5 rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                  onClick={() => removeAttachment(i)}
                  aria-label={`Remove ${safeName}`}
                >
                  <XIcon className="size-2.5" />
                </button>
              </div>
            );
          })}
          {isUploading && (
            <span className="text-caption text-muted-foreground">Uploading…</span>
          )}
          {uploadError && (
            <span
              className="flex items-center gap-1 text-caption"
              style={{ color: "var(--color-error)" }}
            >
              <AlertCircle className="size-3" />
              {uploadError}
              <button
                type="button"
                className="ml-1 underline"
                onClick={() => setUploadError(null)}
              >
                dismiss
              </button>
            </span>
          )}
        </div>
      )}

      {/* Textarea (with drag-drop) */}
      <div
        className={cn(
          "relative rounded-xl border border-input bg-muted/10 px-4 py-2.5 transition-colors",
          "focus-within:border-ring focus-within:ring-1 focus-within:ring-ring/50",
          isDragging && "border-[color:var(--claude-blue)] bg-[color:var(--claude-blue)]/5",
        )}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        {isDragging && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center rounded-xl bg-[color:var(--claude-blue)]/10">
            <div className="flex items-center gap-2 text-body-sm font-medium" style={{ color: "var(--claude-blue)" }}>
              <Paperclip className="size-4" />
              Drop files to attach
            </div>
          </div>
        )}
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
        {/* Left: permission + attachment + environment */}
        <div className="flex items-center gap-2">
          {/* Attach file button */}
          <button
            type="button"
            className={cn(
              "flex items-center gap-1 rounded-lg border border-border/50 px-2 py-1 text-label transition-colors hover:bg-accent hover:text-foreground",
              "text-muted-foreground",
              isUploading && "opacity-50",
            )}
            onClick={() => fileInputRef.current?.click()}
            disabled={isBusy || isUploading}
            aria-label="Attach file"
            title="Attach file (or drag-drop into the textarea)"
          >
            <Paperclip className="size-3" />
          </button>

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
              // CR-01: Enable Send when EITHER text or attachments are present.
              // handleSend() (line ~259) already allows attachment-only sends,
              // so the disabled check must mirror that logic or users cannot
              // send files without typing text first.
              value.trim() || attachments.length > 0
                ? "cursor-pointer"
                : "cursor-not-allowed opacity-50"
            )}
            style={{ backgroundColor: "var(--claude-orange)" }}
            onClick={handleSend}
            disabled={!value.trim() && attachments.length === 0}
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

/**
 * S0.3 extraction target: message composer (CCD soul ②).
 *
 * Original: features/session-workbench/InputBar.tsx.
 *
 * MVP cuts per ClawWiki canonical §11.1 "Composer（InputBar）— MVP 裁剪：
 * 只留 @mention 和多行，斜杠命令暂不要":
 *   - Remove SLASH_COMMANDS palette and all its keyboard handling.
 *   - Remove `onSlashCommand` prop.
 *   - Remove the "/ for commands" placeholder hint.
 *
 * Preserved verbatim:
 *   - File attachments (validation, drag-drop, upload, chips row).
 *   - Permission-mode dropdown (menu + picker).
 *   - Textarea with history (ArrowUp/ArrowDown).
 *   - Send / Stop button.
 *
 * Moved:
 *   - `PERMISSION_MODES` + `getPermissionConfig` live in
 *     `@/features/permission/permission-config` now so both Composer
 *     and StatusLine can consume them without a cycle.
 */

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
  ChevronDown,
  Paperclip,
  X as XIcon,
  FileText,
  Image as ImageIcon,
  AlertCircle,
  Monitor,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { sanitizeFilename } from "@/lib/security";
import { useSettingsStore } from "@/state/settings-store";
import {
  PERMISSION_MODES,
  getPermissionConfig,
} from "@/features/permission/permission-config";

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

/* ─── Attachment validation ─────────────────────────────────────── */

/** Hard size cap in bytes — keep in sync with backend (15 MiB body limit). */
const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;

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
 */
function validateAttachment(file: File): string | null {
  if (file.size > MAX_ATTACHMENT_BYTES) {
    const mb = (file.size / 1024 / 1024).toFixed(1);
    return `${file.name} is ${mb} MB — exceeds 10 MB limit`;
  }

  if (file.size === 0 && !file.name.includes(".")) {
    return `${file.name} looks like a folder — only individual files are supported`;
  }

  if (file.type && ALLOWED_MIME_TYPES.has(file.type)) {
    return null;
  }

  const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
  if (ext && ALLOWED_TEXT_EXTENSIONS.has(ext)) {
    return null;
  }

  const mimeLabel = file.type || "unknown type";
  return `${file.name} (${mimeLabel}) is not an allowed file type`;
}

/* ─── Composer component ─────────────────────────────────────────── */

interface ComposerProps {
  onSend: (message: string) => void | Promise<void>;
  onStop?: () => void;
  isBusy?: boolean;
  environmentLabel?: string;
  inputRef?: React.RefObject<HTMLTextAreaElement | null>;
}

export function Composer({
  onSend,
  onStop,
  isBusy = false,
  environmentLabel = "Local",
  inputRef,
}: ComposerProps) {
  const permissionMode = useSettingsStore((state) => state.permissionMode);
  const setPermissionMode = useSettingsStore((state) => state.setPermissionMode);
  const modeConfig = getPermissionConfig(permissionMode);

  const [value, setValue] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [showPermissionMenu, setShowPermissionMenu] = useState(false);
  const [attachments, setAttachments] = useState<ProcessedAttachment[]>([]);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isUploading, setIsUploading] = useState(false);
  const internalRef = useRef<HTMLTextAreaElement>(null);
  const textareaRef = inputRef ?? internalRef;
  const permMenuRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // ── Attachment handling ─────────────────────────────────────────
  const handleFiles = useCallback(async (files: FileList | File[]) => {
    const list = Array.from(files);
    if (list.length === 0) return;
    setIsUploading(true);
    setUploadError(null);

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
    if (e.currentTarget === e.target) {
      setIsDragging(false);
    }
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragging(false);

      // Reject folder drops — we only accept individual files.
      const items = e.dataTransfer.items;
      const fileList: File[] = [];
      let rejectedFolders = 0;

      if (items && items.length > 0) {
        for (let i = 0; i < items.length; i++) {
          const item = items[i];
          if (item.kind !== "file") continue;

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

    // Build the final message with attachments prepended as a markdown block.
    let finalMessage = trimmed;
    if (attachments.length > 0) {
      const attachmentBlock = attachments
        .map((a) => {
          const safeName = sanitizeFilename(a.filename);
          if (a.kind === "image_base64") {
            return `## ${safeName}\n\n[Image attached: ${a.byte_size} bytes]\n`;
          }
          if (a.kind === "binary_stub") {
            return `## ${safeName}\n\n${a.content}\n`;
          }
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
  }, [value, attachments, isBusy, onSend, textareaRef]);

  const handleStop = useCallback(() => {
    onStop?.();
  }, [onStop]);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // History navigation
    if (e.key === "ArrowUp" && !e.shiftKey && value === "") {
      e.preventDefault();
      if (history.length > 0) {
        const newIndex = Math.min(historyIndex + 1, history.length - 1);
        setHistoryIndex(newIndex);
        setValue(history[newIndex]);
      }
      return;
    }
    if (e.key === "ArrowDown" && !e.shiftKey && historyIndex >= 0) {
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
          e.target.value = "";
        }}
      />

      {/* Attachment chips row */}
      {(attachments.length > 0 || uploadError) && (
        <div className="mb-2 flex flex-wrap items-center gap-1.5">
          {attachments.map((att, i) => {
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
                : "Ask your external brain..."
          }
          disabled={isBusy}
          rows={2}
          className="max-h-[200px] min-h-[44px] w-full resize-none bg-transparent text-body leading-relaxed text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-50"
        />
      </div>

      {/* Bottom button row */}
      <div className="mt-2 flex items-center justify-between">
        {/* Left: attach + permission + environment */}
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

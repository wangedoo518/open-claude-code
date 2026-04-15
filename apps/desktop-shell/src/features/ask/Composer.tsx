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
  ArrowUp,
  Square,
  ChevronDown,
  Paperclip,
  X as XIcon,
  FileText,
  Image as ImageIcon,
  AlertCircle,
  Monitor,
  Code2,
  FileSearch,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { sanitizeFilename } from "@/lib/security";
import { useSettingsStore } from "@/state/settings-store";
import { useStreamingStore } from "@/state/streaming-store";
import {
  PERMISSION_MODES,
  getPermissionConfig,
} from "@/features/permission/permission-config";
import {
  SlashCommandPalette,
  type SlashCommand,
} from "./SlashCommandPalette";
import { fetchJson } from "@/lib/desktop/transport";
import { ingestRawEntry } from "@/features/ingest/persist";

/* ─── MarkItDown constants ──────────────────────────────────────── */

/** File extensions that should be routed through MarkItDown conversion. */
const MARKITDOWN_EXTENSIONS = new Set([
  "pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls", "epub", "ipynb",
  // Images (OCR/metadata via MarkItDown)
  "jpg", "jpeg", "png", "webp", "gif",
  // Audio (SpeechRecognition via MarkItDown)
  "mp3", "wav", "m4a", "ogg", "flac",
  // Video (metadata/fallback)
  "mp4", "mov", "mkv", "avi",
]);

/** Response shape from `POST /api/desktop/markitdown/convert`. */
interface MarkItDownConvertResult {
  ok: boolean;
  title: string;
  markdown: string;
  source: string;
  raw_id: number | null;
}

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

interface ProviderOption {
  id: string;
  label: string;
  model: string;
  isActive: boolean;
}

interface ComposerProps {
  onSend: (message: string) => void | Promise<void>;
  onStop?: () => void;
  isBusy?: boolean;
  modelLabel?: string;
  environmentLabel?: string;
  providers?: ProviderOption[];
  onSwitchProvider?: (id: string) => void;
  inputRef?: React.RefObject<HTMLTextAreaElement | null>;
  onClear?: () => void;
  onNewSession?: () => void;
  onExportMarkdown?: () => void;
  onCompact?: () => void;
}

export function Composer({
  onSend,
  onStop,
  isBusy = false,
  modelLabel,
  environmentLabel = "Local",
  providers,
  onSwitchProvider,
  inputRef,
  onClear,
  onNewSession,
  onExportMarkdown,
  onCompact,
}: ComposerProps) {
  const permissionMode = useSettingsStore((state) => state.permissionMode);
  const setPermissionMode = useSettingsStore((state) => state.setPermissionMode);
  const modeConfig = getPermissionConfig(permissionMode);
  const isPlanMode = useStreamingStore((s) => s.isPlanMode);
  const setPlanMode = useStreamingStore((s) => s.setPlanMode);

  const [value, setValue] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [showPermissionMenu, setShowPermissionMenu] = useState(false);
  const [attachments, setAttachments] = useState<ProcessedAttachment[]>([]);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isUploading, setIsUploading] = useState(false);
  const [convertingFile, setConvertingFile] = useState<string | null>(null);
  const internalRef = useRef<HTMLTextAreaElement>(null);
  const textareaRef = inputRef ?? internalRef;
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);
  const permMenuRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // ── Slash command state ────────────────────────────────────────
  const showSlashPalette = value.startsWith("/") && !isBusy;
  const slashQuery = showSlashPalette ? value.slice(1) : "";

  const handleSlashSelect = useCallback(
    (cmd: SlashCommand) => {
      setValue("");
      switch (cmd.action) {
        case "clear":
          onClear?.();
          break;
        case "new":
          onNewSession?.();
          break;
        case "export":
          onExportMarkdown?.();
          break;
        case "compact":
          onCompact?.();
          break;
        case "plan":
          void onSend("/plan");
          break;
      }
    },
    [onClear, onNewSession, onExportMarkdown, onSend]
  );

  const handleSlashClose = useCallback(() => {
    // Just leave the text — user can keep typing
  }, []);

  /** Clear the composer input, attachments, and reset textarea height. */
  const resetComposer = useCallback(() => {
    setValue("");
    setAttachments([]);
    setUploadError(null);
    if (textareaRef.current) textareaRef.current.style.height = "auto";
  }, [textareaRef]);

  // ── Attachment handling ─────────────────────────────────────────
  const handleFiles = useCallback(async (files: FileList | File[]) => {
    const list = Array.from(files);
    if (list.length === 0) return;

    // ── MarkItDown file detection ──
    // Separate files into MarkItDown-convertible vs regular attachments.
    const markitdownFiles: File[] = [];
    const regularFiles: File[] = [];

    for (const file of list) {
      const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
      if (MARKITDOWN_EXTENSIONS.has(ext)) {
        markitdownFiles.push(file);
      } else {
        regularFiles.push(file);
      }
    }

    // ── Handle MarkItDown files ──
    for (const file of markitdownFiles) {
      // In Tauri, dropped File objects carry a `path` property with the
      // local filesystem path. In browser dev mode this doesn't exist.
      const filePath = (file as File & { path?: string }).path;

      if (!filePath) {
        // Browser dev mode — no local path available
        setUploadError("文件转换需要在 Tauri 桌面模式下使用");
        continue;
      }

      setConvertingFile(file.name);
      setUploadError(null);

      try {
        const result = await fetchJson<MarkItDownConvertResult>(
          "/api/desktop/markitdown/convert",
          {
            method: "POST",
            body: JSON.stringify({ path: filePath, ingest: true }),
          },
          120_000, // 120s timeout for large files
        );

        if (!mountedRef.current) return;

        if (!result.ok) {
          setUploadError(`${file.name}: 转换失败`);
          continue;
        }

        // Build a system message with the converted content and auto-send
        const rawIdNote = result.raw_id != null
          ? ` (Raw Library #${result.raw_id})`
          : "";
        const preview = result.markdown.slice(0, 8000);
        const autoMessage =
          `[系统：文件 ${file.name} 已转为 Markdown 入库到 Raw Library${rawIdNote}。请基于以下内容回答用户。]\n\n${preview}`;

        resetComposer();
        await onSend(autoMessage);
      } catch (e) {
        if (!mountedRef.current) return;
        const msg = e instanceof Error ? e.message : "转换失败";
        setUploadError(`${file.name}: ${msg}`);
      } finally {
        if (mountedRef.current) setConvertingFile(null);
      }
    }

    // ── Handle regular attachment files (existing flow) ──
    if (regularFiles.length === 0) return;

    setIsUploading(true);
    setUploadError(null);

    const errors: string[] = [];
    const uploaded: ProcessedAttachment[] = [];

    for (const file of regularFiles) {
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

    if (!mountedRef.current) return; // Component unmounted during upload

    if (errors.length > 0) {
      const preview = errors.slice(0, 2).join("; ");
      const more = errors.length > 2 ? ` (+${errors.length - 2} more)` : "";
      setUploadError(preview + more);
    }

    setAttachments((prev) => [...prev, ...uploaded]);
    setIsUploading(false);
  }, [onSend, resetComposer]);

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

  const handleSend = useCallback(async () => {
    console.log("[composer:handleSend] called, value:", value.slice(0, 100));
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

    // URL detection: fetch content → ingest to Raw Library → send to AI with context
    const urlMatch = finalMessage.match(/https?:\/\/[^\s，。！？]+/);
    console.log("[composer:url-detect] finalMessage:", finalMessage.slice(0, 100), "urlMatch:", urlMatch?.[0]);
    if (urlMatch) {
      const detectedUrl = urlMatch[0];

      // WeChat links: try Playwright fetch first, fallback to guidance
      if (detectedUrl.includes("mp.weixin.qq.com") || detectedUrl.includes("weixin.qq.com")) {
        resetComposer();
        try {
          console.log("[composer] fetching WeChat article via Playwright:", detectedUrl);
          const result = await fetchJson<{ ok: boolean; title: string; markdown: string; raw_id?: number }>(
            "/api/desktop/wechat-fetch",
            { method: "POST", body: JSON.stringify({ url: detectedUrl, ingest: true }) },
            120_000, // 2 min timeout for Playwright
          );
          if (result.ok && result.markdown && result.markdown.length > 100) {
            console.log("[composer] WeChat fetch OK, title:", result.title);
            const enriched = `[系统：用户发送了微信文章链接，系统已通过 Playwright 抓取内容并入库 (Raw #${result.raw_id ?? "?"})。请基于以下内容回答用户。]\n\n标题：${result.title}\n\n${result.markdown.slice(0, 6000)}\n\n---\n用户原始消息：${finalMessage}`;
            await onSend(enriched);
            return;
          }
        } catch (err) {
          console.warn("[composer] WeChat Playwright fetch failed:", err);
        }
        // Fallback: guidance
        await onSend("用户发送了一个微信公众号链接，但 Playwright 抓取失败。请告诉用户：1. 在微信中将文章转发给 ClawBot 自动入库；2. 或手动复制文章内容粘贴到输入框。不要给出代码示例。");
        return;
      }

      // Non-WeChat URL: clear input first so the user sees responsiveness,
      // then await the fetch → ingest → onSend pipeline sequentially.
      resetComposer();

      try {
        console.log("[composer] fetching URL:", detectedUrl);
        const preview = await fetchJson<{ title: string; body: string; source_url: string }>(
          "/api/wiki/fetch",
          {
            method: "POST",
            body: JSON.stringify({ url: detectedUrl }),
          },
          60_000, // 60s timeout for large pages
        );
        console.log("[composer] fetch OK, title:", preview.title, "body length:", preview.body.length);

        // Check if content is valid (not a CAPTCHA page)
        if (preview.body.length < 200 || preview.body.includes("环境异常") || preview.body.includes("完成验证")) {
          await onSend(`用户想将链接 ${detectedUrl} 的内容入库到知识库，但抓取到的内容被反爬拦截。请告诉用户：去 Raw Library 页面，选择 URL 标签，粘贴链接后点击 Ingest 入库；或手动复制文章内容粘贴到输入框。不要给出代码示例。`);
          return;
        }

        // Ingest to Raw Library (non-fatal — don't block AI message on ingest failure)
        try {
          await ingestRawEntry({
            source: "url",
            title: preview.title || detectedUrl,
            body: preview.body,
            source_url: detectedUrl,
          });
        } catch (ingestErr) {
          console.warn("[composer] Raw Library ingest failed (non-fatal):", ingestErr);
        }

        // Send fetched content to AI with clear instruction
        const enriched = `[系统：用户发送了一个链接，系统已自动抓取内容并入库到 Raw Library。请基于以下抓取内容回答用户，确认入库成功，并简要总结文章要点。不要说"无法访问链接"。]\n\n标题：${preview.title}\n来源：${detectedUrl}\n\n${preview.body.slice(0, 6000)}\n\n---\n用户原始消息：${finalMessage}`;
        await onSend(enriched);
      } catch (err) {
        console.warn("[composer] URL fetch failed:", err);
        // Fetch failed — guide user to Raw Library page instead
        await onSend(`用户想将链接 ${detectedUrl} 的内容入库到知识库，但系统自动抓取失败。请告诉用户：去 Raw Library 页面，选择 URL 标签，粘贴链接后点击 Ingest 入库。不要给出代码示例。`);
      }
      return;
    }

    // Normal (non-URL) message path
    resetComposer();
    await onSend(finalMessage);
  }, [value, attachments, isBusy, onSend, textareaRef, resetComposer]);

  const handleStop = useCallback(() => {
    onStop?.();
  }, [onStop]);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // Skip history nav when slash palette is open
    if (showSlashPalette && (e.key === "ArrowUp" || e.key === "ArrowDown")) return;

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

    // Send on Enter (unless slash palette is intercepting)
    if (e.key === "Enter" && !e.shiftKey) {
      if (showSlashPalette) return; // palette handles Enter via document listener
      e.preventDefault();
      void handleSend();
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

      {/* Attachment preview bar */}
      {(attachments.length > 0 || uploadError || convertingFile) && (
        <div className="mb-2 flex items-center gap-2 overflow-x-auto">
          {attachments.map((att, i) => {
            const safeName = sanitizeFilename(att.filename);
            const isImage = att.kind === "image_base64";

            if (isImage) {
              return (
                <div
                  key={`${safeName}-${att.byte_size}-${i}`}
                  className="group/att relative size-16 shrink-0 overflow-hidden rounded-lg border border-border"
                  title={safeName}
                >
                  <div className="flex size-full items-center justify-center bg-muted/30">
                    <ImageIcon className="size-6 text-muted-foreground/40" />
                  </div>
                  <button
                    type="button"
                    className="absolute -right-1.5 -top-1.5 flex size-4 items-center justify-center rounded-full bg-foreground text-background opacity-0 shadow-sm transition-opacity group-hover/att:opacity-100"
                    onClick={() => removeAttachment(i)}
                    aria-label={`Remove ${safeName}`}
                  >
                    <XIcon className="size-2.5" />
                  </button>
                </div>
              );
            }

            return (
              <div
                key={`${safeName}-${att.byte_size}-${i}`}
                className="group/att inline-flex shrink-0 items-center gap-1 rounded-lg bg-muted/50 px-3 py-1.5 text-[11px]"
                title={`${safeName} (${att.byte_size} bytes${att.truncated ? ", truncated" : ""})`}
                dir="ltr"
              >
                {att.kind === "binary_stub" ? (
                  <AlertCircle className="size-3 shrink-0" style={{ color: "var(--color-warning)" }} />
                ) : (
                  <FileText className="size-3 shrink-0 text-muted-foreground" />
                )}
                <span className="max-w-[140px] truncate font-medium text-muted-foreground" dir="ltr">
                  {safeName}
                </span>
                <button
                  type="button"
                  className="ml-0.5 opacity-60 transition-opacity hover:opacity-100"
                  onClick={() => removeAttachment(i)}
                  aria-label={`Remove ${safeName}`}
                >
                  <XIcon className="size-2.5" />
                </button>
              </div>
            );
          })}
          {convertingFile && (
            <span className="shrink-0 text-[11px] text-muted-foreground">
              正在转换 {convertingFile}...
            </span>
          )}
          {isUploading && (
            <span className="shrink-0 text-[11px] text-muted-foreground">上传中…</span>
          )}
          {uploadError && (
            <span
              className="flex shrink-0 items-center gap-1 text-[11px]"
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

      {/* Slash command palette */}
      <SlashCommandPalette
        query={slashQuery}
        visible={showSlashPalette}
        onSelect={handleSlashSelect}
        onClose={handleSlashClose}
      />

      {/* Input area — CodePilot style: textarea with inline tools */}
      <div
        className={cn(
          "relative overflow-hidden rounded-2xl border bg-card shadow-[0_1px_8px_rgba(0,0,0,0.03)] transition-colors",
          isDragging
            ? "border-2 border-dashed border-primary/50 bg-primary/[0.03]"
            : "border-border focus-within:border-ring",
        )}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        {isDragging && (
          <div className="pointer-events-none absolute inset-0 z-10 flex flex-col items-center justify-center gap-1.5">
            <Paperclip className="size-5 text-primary" strokeWidth={1.6} />
            <span className="text-[13px] font-medium text-primary">拖放文件到这里</span>
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
              ? "等待回复中..."
              : permissionMode === "plan"
                ? "描述你的计划..."
                : "问点什么…    Enter 发送 · Shift+Enter 换行 · / 命令"
          }
          disabled={isBusy}
          rows={1}
          className="max-h-[200px] min-h-[52px] w-full resize-none bg-transparent px-4 pb-1 pt-3.5 text-[14px] leading-relaxed text-foreground outline-none transition-[height] duration-150 ease-out placeholder:text-muted-foreground/50 disabled:pointer-events-none disabled:opacity-50"
        />

        {/* Inline toolbar inside the input card */}
        <div className="flex items-center justify-between px-3 pb-2.5">
          <div className="flex items-center gap-1">
            {/* Attach */}
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              className={cn(
                "text-muted-foreground hover:text-foreground",
                isUploading && "opacity-50",
              )}
              onClick={() => fileInputRef.current?.click()}
              disabled={isBusy || isUploading}
              aria-label="附件"
            >
              <Paperclip className="size-3.5" />
            </Button>

            {/* Slash commands */}
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              className="text-muted-foreground hover:text-foreground"
              onClick={() => { setValue("/"); textareaRef.current?.focus(); }}
              aria-label="命令"
            >
              <ChevronDown className="size-3.5 rotate-[-90deg]" />
            </Button>

            {/* Separator + model label (selector is in bottom bar) */}
            <div className="mx-1 h-4 w-px bg-border" />
            <span className="text-[11px] text-muted-foreground/60">
              {modelLabel || "AI"}
            </span>
          </div>

          {/* Send / Stop button */}
          {isBusy ? (
            <Button
              size="icon-sm"
              variant="destructive"
              className="rounded-full transition-transform duration-150 active:scale-95"
              onClick={handleStop}
              aria-label="停止"
            >
              <Square className="size-3.5" />
            </Button>
          ) : (
            <Button
              size="icon-sm"
              variant="default"
              className={cn(
                "rounded-full text-white transition-[transform,opacity,box-shadow] duration-150",
                value.trim() || attachments.length > 0
                  ? "shadow-sm hover:shadow-md hover:scale-105 active:scale-95"
                  : "bg-primary/40 pointer-events-none",
              )}
              onClick={() => void handleSend()}
              disabled={!value.trim() && attachments.length === 0}
              aria-label="发送"
            >
              <ArrowUp className="size-4" />
            </Button>
          )}
        </div>
      </div>

      {/* Bottom mode bar — CodePilot style */}
      <div className="mt-1.5 flex items-center gap-3">
        {/* Code / Plan mode toggle — like CodePilot */}
        <div className="flex items-center rounded-md border border-border/50">
          <button
            className={cn(
              "flex items-center gap-1 rounded-l-md px-2 py-1 text-[11px] transition-colors",
              !isPlanMode ? "bg-accent text-foreground font-medium" : "text-muted-foreground hover:bg-accent/50"
            )}
            onClick={() => setPlanMode(false)}
          >
            <Code2 className="size-3" />
            代码
          </button>
          <button
            className={cn(
              "flex items-center gap-1 rounded-r-md px-2 py-1 text-[11px] transition-colors",
              isPlanMode ? "bg-accent text-foreground font-medium" : "text-muted-foreground hover:bg-accent/50"
            )}
            onClick={() => setPlanMode(true)}
          >
            <FileSearch className="size-3" />
            计划
          </button>
        </div>

        {/* Permission mode selector */}
        <div className="relative" ref={permMenuRef}>
          <button
            className={cn(
              "flex items-center gap-1 rounded-md px-2 py-1 text-[11px] transition-colors hover:bg-accent",
              showPermissionMenu ? "bg-accent text-foreground" : "text-muted-foreground"
            )}
            style={modeConfig.color ? { color: modeConfig.color } : undefined}
            onClick={() => setShowPermissionMenu((prev) => !prev)}
          >
            <ModeIcon className="size-3" />
            <span>{modeConfig.label}</span>
          </button>

          {showPermissionMenu && (
            <div className="absolute bottom-full left-0 mb-1 w-[240px] rounded-lg border border-border bg-popover p-1 shadow-lg">
              <div className="px-2 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                权限模式
              </div>
              {PERMISSION_MODES.map((mode) => {
                const Icon = mode.icon;
                const isActive = permissionMode === mode.value;
                return (
                  <button
                    key={mode.value}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
                      isActive ? "bg-accent text-accent-foreground" : "text-foreground hover:bg-accent/50"
                    )}
                    onClick={() => { setPermissionMode(mode.value); setShowPermissionMenu(false); }}
                  >
                    <Icon className="size-3 shrink-0" style={mode.color ? { color: mode.color } : undefined} />
                    <div className="min-w-0 flex-1">
                      <div className="text-[11px] font-medium">{mode.label}</div>
                      <div className="text-[10px] text-muted-foreground">{mode.desc}</div>
                    </div>
                    {isActive && <div className="size-1.5 shrink-0 rounded-full" style={{ backgroundColor: mode.color ?? "var(--claude-orange)" }} />}
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* Model selector — in bottom bar so no overflow clip */}
        <ModelSelector
          currentLabel={modelLabel || "AI"}
          providers={providers}
          onSwitch={onSwitchProvider}
        />

        {/* Environment */}
        <span className="text-[11px] text-muted-foreground/50">
          <Monitor className="mr-0.5 inline size-3 align-[-2px]" />
          {environmentLabel}
        </span>
      </div>
    </div>
  );
}

/* ─── Model selector dropdown ──────────────────────────────────── */

function ModelSelector({
  currentLabel,
  providers,
  onSwitch,
}: {
  currentLabel: string;
  providers?: ProviderOption[];
  onSwitch?: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  if (!providers || providers.length === 0 || !onSwitch) {
    return (
      <span className="text-[11px] text-muted-foreground/60">
        {currentLabel}
      </span>
    );
  }

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className="flex items-center gap-0.5 text-[11px] text-muted-foreground/60 transition-colors hover:text-foreground"
        onClick={() => setOpen(!open)}
      >
        {currentLabel}
        <ChevronDown className="size-2.5 opacity-40" />
      </button>

      {open && (
        <div className="absolute bottom-full left-0 z-50 mb-1 min-w-[200px] rounded-lg border border-border bg-popover p-1 shadow-lg">
          <div className="px-2 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            切换模型
          </div>
          {providers.map((p) => (
            <button
              key={p.id}
              type="button"
              className={cn(
                "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] transition-colors",
                p.isActive ? "bg-accent text-foreground" : "text-foreground hover:bg-accent/50"
              )}
              onClick={() => {
                onSwitch(p.id);
                setOpen(false);
              }}
            >
              <div className="min-w-0 flex-1">
                <div className="font-medium">{p.label}</div>
                <div className="text-[10px] text-muted-foreground">{p.model}</div>
              </div>
              {p.isActive && (
                <div className="size-1.5 shrink-0 rounded-full bg-primary" />
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

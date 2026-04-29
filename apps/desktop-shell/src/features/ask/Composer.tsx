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
  useMemo,
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
import { ResponseModeChip } from "./ResponseModeChip";
import { SourceBindingChip } from "./SourceBindingChip";
import { PurposeLensChip } from "./PurposeLensChip";
import { classifyContextMode, extractFirstUrl } from "./mode-classifier";
import type { ContextMode, SessionSourceBinding } from "@/lib/tauri";
import type { ConversationTurnStatus } from "./useConversationTurnState";
import type { PurposeLensId } from "@/features/purpose/purpose-lenses";
import { CapabilityHint } from "./CapabilityHint";
import { ModelCapabilityCard } from "./ModelCapabilityCard";
import { ModelCapabilityIndicator } from "./ModelCapabilityIndicator";
import {
  getModelCapability,
  type CapabilityProviderKind,
  type ModelCapability,
} from "./model-capabilities";

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
  kind: CapabilityProviderKind;
  isActive: boolean;
}

interface ComposerProps {
  /**
   * Submit handler. A1 sprint — optional second argument carries the
   * per-turn context mode (auto-detected, possibly overridden). Legacy
   * callers that accept only a single string argument keep working;
   * the parameter is positional and defaults away cleanly.
   */
  onSend: (
    message: string,
    options?: { mode?: ContextMode; purpose?: string[] },
  ) => void | Promise<void>;
  onStop?: () => void;
  isBusy?: boolean;
  turnStatus?: ConversationTurnStatus;
  onComposingChange?: (isComposing: boolean) => void;
  modelLabel?: string;
  environmentLabel?: string;
  providers?: ProviderOption[];
  onSwitchProvider?: (id: string) => void;
  inputRef?: React.RefObject<HTMLTextAreaElement | null>;
  onClear?: () => void;
  onNewSession?: () => void;
  onExportMarkdown?: () => void;
  onCompact?: () => void;
  /**
   * Optional source-pin id (raw-entry / URL the user pinned via the
   * side panel). Feeds the context-mode classifier. Omit when no
   * source is pinned (default: classifier treats as no-pin).
   */
  selectedSourceId?: string;
  /**
   * A2 sprint — persistent session-level source binding (from
   * `DesktopSessionDetail.source_binding`). When non-null, a
   * `SourceBindingChip` renders above the URL-detect row so the user
   * sees which source is currently pinned for the whole session.
   */
  binding?: SessionSourceBinding | null;
  /** A2 sprint — clear the session source binding. */
  onClearBinding?: () => void;
  /** External draft injection, used by starter prompts and command UX. */
  draftValue?: string | null;
  onDraftConsumed?: () => void;
}

export function Composer({
  onSend,
  onStop,
  isBusy = false,
  turnStatus,
  onComposingChange,
  modelLabel,
  providers,
  onSwitchProvider,
  inputRef,
  onClear,
  onNewSession,
  onExportMarkdown,
  onCompact,
  selectedSourceId,
  binding,
  onClearBinding,
  draftValue,
  onDraftConsumed,
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

  // A1 sprint — per-keystroke context-mode classification. `overrideMode`
  // is set when the user clicks the ResponseModeChip to force a
  // particular mode; null means "trust the auto-detected value".
  const [overrideMode, setOverrideMode] = useState<ContextMode | null>(null);
  const [selectedPurpose, setSelectedPurpose] = useState<PurposeLensId | null>(null);
  const classification = useMemo(
    () => classifyContextMode(value, { selectedSourceId }),
    [value, selectedSourceId],
  );
  const effectiveMode: ContextMode = overrideMode ?? classification.mode;
  const detectedUrl = useMemo(() => extractFirstUrl(value), [value]);
  const internalRef = useRef<HTMLTextAreaElement>(null);
  const textareaRef = inputRef ?? internalRef;
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);
  const permMenuRef = useRef<HTMLDivElement>(null);
  const composerRootRef = useRef<HTMLDivElement>(null);
  const refocusAfterSendRef = useRef(false);
  const rafRef = useRef<number>(0);
  const focusTimerRef = useRef<number | null>(null);
  const focusRafRef = useRef<number | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [modelSelectorOpenRequest, setModelSelectorOpenRequest] = useState(0);
  const activeProvider = providers?.find((provider) => provider.isActive);
  const activeModelId = activeProvider?.model ?? modelLabel ?? "unknown";
  const activeProviderKind: CapabilityProviderKind =
    activeProvider?.kind ?? (activeModelId.startsWith("claude-") ? "anthropic" : "openai_compat");
  const activeCapability = getModelCapability(activeModelId, activeProviderKind);
  const turnState =
    turnStatus?.state ??
    (isBusy ? "streaming" : value.trim() ? "composing" : "idle");
  const isWorkingTurn = turnStatus?.isWorking ?? isBusy;
  const isWaitingPermission = turnState === "waiting_permission";
  const isFatalTurn = turnState === "failed_fatal";
  const inputBlocked =
    (turnStatus?.isInputBlocked ?? isBusy) ||
    isUploading ||
    convertingFile != null;
  const canSend =
    !inputBlocked &&
    !isUploading &&
    convertingFile == null &&
    (value.trim().length > 0 || attachments.length > 0);
  const composerPlaceholder = isWorkingTurn
    ? "Press Esc to interrupt"
    : isWaitingPermission
      ? "上方需要你的确认"
      : isFatalTurn
        ? "服务不可用，请检查设置"
        : turnState === "complete" || turnState === "interrupted"
          ? "继续问点什么…"
          : permissionMode === "plan"
            ? "描述你的计划…"
            : "问点什么…";
  const composerMeta = isWorkingTurn
    ? "Esc 中断"
    : isWaitingPermission
      ? "↑ 处理上方授权请求"
      : isFatalTurn
        ? "⚠ 服务不可用 · 请检查设置"
        : "Enter 发送 · Shift+Enter 换行";

  const focusTextareaIfReady = useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea || textarea.disabled) return false;

    textarea.focus({ preventScroll: true });
    const cursor = textarea.value.length;
    try {
      textarea.setSelectionRange(cursor, cursor);
    } catch {
      // Selection can fail for unusual IME/browser states; focus is enough.
    }
    return true;
  }, [textareaRef]);

  const scheduleTextareaFocus = useCallback(() => {
    if (focusTimerRef.current !== null) {
      window.clearTimeout(focusTimerRef.current);
    }
    focusTimerRef.current = window.setTimeout(() => {
      focusTimerRef.current = null;
      if (focusRafRef.current !== null) {
        window.cancelAnimationFrame(focusRafRef.current);
      }
      focusRafRef.current = window.requestAnimationFrame(() => {
        focusRafRef.current = null;
        if (!mountedRef.current) return;
        if (focusTextareaIfReady()) {
          refocusAfterSendRef.current = false;
        }
      });
    }, 0);
  }, [focusTextareaIfReady]);

  useEffect(() => {
    if (!inputBlocked && refocusAfterSendRef.current) {
      scheduleTextareaFocus();
    }
  }, [inputBlocked, scheduleTextareaFocus]);

  useEffect(() => {
    onComposingChange?.(value.trim().length > 0 && !inputBlocked);
  }, [inputBlocked, onComposingChange, value]);

  useEffect(() => {
    if (draftValue == null) return;
    setValue(draftValue);
    setOverrideMode(null);
    const raf = requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (textarea) {
        textarea.focus();
        textarea.style.height = "auto";
        textarea.style.height = Math.min(textarea.scrollHeight, 200) + "px";
        textarea.setSelectionRange(draftValue.length, draftValue.length);
      }
      onDraftConsumed?.();
    });
    return () => cancelAnimationFrame(raf);
  }, [draftValue, onDraftConsumed, textareaRef]);

  // ── Slash command state ────────────────────────────────────────
  const [dismissedSlashValue, setDismissedSlashValue] = useState<string | null>(null);
  const slashCandidateVisible = value.startsWith("/") && !inputBlocked;
  const showSlashPalette = slashCandidateVisible && dismissedSlashValue !== value;
  const slashQuery = slashCandidateVisible ? value.slice(1) : "";

  useEffect(() => {
    if (!value.startsWith("/") && dismissedSlashValue !== null) {
      setDismissedSlashValue(null);
    }
  }, [dismissedSlashValue, value]);

  const handleSlashSelect = useCallback(
    (cmd: SlashCommand) => {
      setDismissedSlashValue(null);
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
    [onClear, onNewSession, onExportMarkdown, onCompact, onSend]
  );

  const handleSlashClose = useCallback(() => {
    setDismissedSlashValue(value);
  }, [value]);

  const handleSlashToggle = useCallback(() => {
    if (showSlashPalette) {
      setDismissedSlashValue(value);
    } else {
      setDismissedSlashValue(null);
      setValue((current) => (current.startsWith("/") ? current : "/"));
    }
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [showSlashPalette, textareaRef, value]);

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
    const trimmed = value.trim();
    // Allow sending if there's text OR at least one attachment.
    if ((!trimmed && attachments.length === 0) || inputBlocked) return;
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

    // Composer does NOT perform URL / WeChat fetch or Raw ingest here.
    //
    // The canonical Ask pipeline owns URL ingest exactly once:
    //   Ask send
    //     → desktop-core::append_user_message
    //       → maybe_enrich_url      (wiki_ingest fetch + wiki_store raw write +
    //                                append_new_raw_task Inbox item)
    //       → enriched context injected into the agent system prompt
    //     → user's original message text stays as-is in session history
    //
    // A previous revision of this file called `/api/desktop/wechat-fetch`
    // and `/api/wiki/fetch` + `ingestRawEntry` BEFORE `onSend`, which
    // double-wrote Raw / Inbox entries and raced the backend enrichment.
    // The progress hint ("⏳ 正在抓取 ...") is emitted by
    // `AskWorkbench.handleSendWithUrlFetch` as a pure UI affordance and
    // does NOT imply any frontend fetch.
    //
    // Raw Library still has its own explicit ingest button that calls
    // `ingestRawEntry` via `features/ingest/adapters/url.ts` — that path
    // is unchanged and intentional (user opts in to ingest a specific URL).

    // A1 sprint — hand the effective mode (override ?? detected) to the
    // upstream sender. Legacy callers that ignore the second arg stay
    // working (JS variadic dispatch). We reset `overrideMode` after
    // sending so the next turn is auto-classified fresh.
    const modeToSend = overrideMode ?? classification.mode;
    refocusAfterSendRef.current = true;
    resetComposer();
    setOverrideMode(null);
    await onSend(finalMessage, {
      mode: modeToSend,
      ...(selectedPurpose ? { purpose: [selectedPurpose] } : {}),
    });
    scheduleTextareaFocus();
  }, [
    value,
    attachments,
    inputBlocked,
    onSend,
    resetComposer,
    overrideMode,
    classification.mode,
    selectedPurpose,
    scheduleTextareaFocus,
  ]);

  const handleStop = useCallback(() => {
    onStop?.();
  }, [onStop]);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Escape" && turnStatus?.canInterrupt) {
      e.preventDefault();
      onStop?.();
      return;
    }

    if (inputBlocked) {
      return;
    }

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
    return () => {
      cancelAnimationFrame(rafRef.current);
      if (focusTimerRef.current !== null) {
        window.clearTimeout(focusTimerRef.current);
      }
      if (focusRafRef.current !== null) {
        window.cancelAnimationFrame(focusRafRef.current);
      }
    };
  }, []);

  const ModeIcon = modeConfig.icon;

  return (
    <div
      ref={composerRootRef}
      className="ask-composer relative border-t border-border/50 bg-background px-4 py-3"
      data-busy={isBusy || undefined}
      data-state={turnState}
      data-turn-state={turnState}
      data-input-blocked={inputBlocked ? "true" : "false"}
      data-dragging={isDragging || undefined}
    >
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
        <div className="ask-attachment-strip mb-2 flex items-center gap-2 overflow-x-auto">
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

      {/* A2 — persistent session source-binding chip. Rendered above
          the URL-detect row so the user always sees which source is
          currently pinned for the whole session. Null binding → null. */}
      {binding && onClearBinding && (
        <div className="mb-1.5 flex items-center gap-2 px-1 text-[11px] text-muted-foreground">
          <SourceBindingChip binding={binding} onClear={onClearBinding} />
        </div>
      )}

      {/* A1 — URL detection chip. Inline div (no new component). Shown
          whenever the draft contains a URL so the user has explicit
          feedback that the backend will try to enrich it. Empty string
          deliberately makes this render nothing (not a zero-height div). */}
      {detectedUrl && (
        <div className="ask-url-chip mb-1.5 flex items-center gap-1 px-1 text-[11px] text-muted-foreground">
          <span aria-hidden="true">🔗</span>
          <span className="truncate" title={detectedUrl}>
            识别：{detectedUrl.length > 60 ? `${detectedUrl.slice(0, 60)}…` : detectedUrl}
          </span>
        </div>
      )}

      <CapabilityHint
        modelLabel={modelLabel || activeModelId}
        modelId={activeModelId}
        capability={activeCapability}
        inputValue={value}
        hasOtherHint={showSlashPalette || isWaitingPermission || isFatalTurn}
        onSwitchToAnthropic={() => setModelSelectorOpenRequest((count) => count + 1)}
      />

      {/* Input area — CodePilot style: textarea with inline tools */}
      <div
        className={cn(
          "ask-composer-card relative overflow-visible rounded-2xl border bg-card shadow-[0_1px_8px_rgba(0,0,0,0.03)] transition-colors",
          isDragging
            ? "border-2 border-dashed border-primary/50 bg-primary/[0.03]"
            : "border-border",
        )}
        data-ready={value.trim() || attachments.length > 0 ? "true" : "false"}
        data-turn-state={turnState}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        {isDragging && (
          <div className="ask-composer-drag-overlay pointer-events-none absolute inset-0 z-10 flex flex-col items-center justify-center gap-1.5">
            <Paperclip className="size-5 text-primary" strokeWidth={1.6} />
            <span className="text-[13px] font-medium text-primary">拖放文件到这里</span>
          </div>
        )}
        {!value && (
          <span
            className="ask-composer-placeholder pointer-events-none absolute left-4 top-3.5 z-[1] text-[14px] leading-relaxed text-muted-foreground/50"
            aria-hidden="true"
          >
            {composerPlaceholder}
          </span>
        )}
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          onInput={handleInput}
          aria-label="Message input"
          placeholder=""
          disabled={inputBlocked}
          rows={1}
          className="ask-composer-textarea relative z-[2] max-h-[200px] min-h-[52px] w-full resize-none bg-transparent px-4 pb-1 pt-3.5 text-[14px] leading-relaxed text-foreground outline-none transition-[height] duration-150 ease-out placeholder:text-muted-foreground/50"
        />

        {/* Inline toolbar inside the input card */}
        <div className="ask-composer-toolbar flex items-center justify-between px-3 pb-2.5">
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
              disabled={inputBlocked || isUploading}
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
              onClick={handleSlashToggle}
              disabled={inputBlocked}
              aria-label="命令"
            >
              <ChevronDown className="size-3.5 rotate-[-90deg]" />
            </Button>

            {/* Inline mode controls: keep capability, remove the extra bottom row. */}
            <div className="mx-1 h-4 w-px bg-border" />
            <div className="ask-composer-mode-group flex items-center rounded-md border border-border/50">
              <button
                type="button"
                className={cn(
                  "flex items-center gap-1 rounded-l-md px-2 py-1 text-[11px] transition-colors",
                  !isPlanMode ? "bg-accent text-foreground font-medium" : "text-muted-foreground hover:bg-accent/50"
                )}
                onClick={() => setPlanMode(false)}
                disabled={inputBlocked}
              >
                <Code2 className="size-3" />
                代码
              </button>
              <button
                type="button"
                className={cn(
                  "flex items-center gap-1 rounded-r-md px-2 py-1 text-[11px] transition-colors",
                  isPlanMode ? "bg-accent text-foreground font-medium" : "text-muted-foreground hover:bg-accent/50"
                )}
                onClick={() => setPlanMode(true)}
                disabled={inputBlocked}
              >
                <FileSearch className="size-3" />
                计划
              </button>
            </div>

            <div className="relative" ref={permMenuRef}>
              <button
                type="button"
                className={cn(
                  "flex items-center gap-1 rounded-md px-2 py-1 text-[11px] transition-colors hover:bg-accent",
                  showPermissionMenu ? "bg-accent text-foreground" : "text-muted-foreground"
                )}
                style={modeConfig.color ? { color: modeConfig.color } : undefined}
                onClick={() => setShowPermissionMenu((prev) => !prev)}
                disabled={inputBlocked}
              >
                <ModeIcon className="size-3" />
                <span>{modeConfig.label}</span>
              </button>

              {showPermissionMenu && (
                <div className="ask-floating-menu absolute bottom-full left-0 mb-1 w-[240px] rounded-lg border border-border bg-popover p-1 shadow-lg">
                  <div className="px-2 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    权限模式
                  </div>
                  {PERMISSION_MODES.map((mode) => {
                    const Icon = mode.icon;
                    const isActive = permissionMode === mode.value;
                    return (
                      <button
                        key={mode.value}
                        type="button"
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

            <ResponseModeChip
              mode={effectiveMode}
              confidence={classification.confidence}
              onChange={(next) => setOverrideMode(next === classification.mode ? null : next)}
            />

            <PurposeLensChip
              value={selectedPurpose}
              onChange={setSelectedPurpose}
              disabled={inputBlocked}
            />
          </div>

          {/* Send / Stop button */}
          <div className="flex items-center gap-1.5">
            <span className="ask-composer-state-meta hidden sm:inline">
              {composerMeta}
            </span>
            <ModelSelector
              currentLabel={modelLabel || "AI"}
              providers={providers}
              onSwitch={onSwitchProvider}
              capability={activeCapability}
              openRequest={modelSelectorOpenRequest}
            />
            {isWorkingTurn ? (
              <Button
                size="icon-sm"
                variant="destructive"
                className="ask-send-button ask-composer-stop rounded-full transition-transform duration-150 active:scale-95"
                data-mode="stop"
                onClick={handleStop}
                aria-label="停止"
              >
                <Square className="size-3.5" />
              </Button>
            ) : isFatalTurn ? (
              <Button
                size="icon-sm"
                variant="outline"
                className="ask-composer-settings rounded-full transition-transform duration-150 active:scale-95"
                onClick={() => {
                  window.location.hash = "#/settings";
                }}
                aria-label="打开设置"
              >
                <AlertCircle className="size-3.5" />
              </Button>
            ) : (
              <Button
                size="icon-sm"
                variant="default"
                data-mode="send"
                className={cn(
                  "ask-send-button ask-composer-send rounded-full text-white transition-[transform,opacity,box-shadow] duration-150",
                  canSend
                    ? "shadow-sm hover:shadow-md hover:scale-105 active:scale-95"
                    : "bg-primary/40 pointer-events-none",
                )}
                onClick={() => void handleSend()}
                disabled={!canSend}
                aria-label="发送"
              >
                <ArrowUp className="size-4" />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ─── Model selector dropdown ──────────────────────────────────── */

function ModelSelector({
  currentLabel,
  providers,
  onSwitch,
  capability,
  openRequest = 0,
}: {
  currentLabel: string;
  providers?: ProviderOption[];
  onSwitch?: (id: string) => void;
  capability: ModelCapability;
  openRequest?: number;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (openRequest > 0) {
      setOpen(true);
    }
  }, [openRequest]);

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
      <span className="ask-model-pill">
        {currentLabel}
        <ModelCapabilityIndicator capability={capability} />
      </span>
    );
  }

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className="ask-model-pill"
        onClick={() => setOpen(!open)}
      >
        {currentLabel}
        <ModelCapabilityIndicator capability={capability} />
        <ChevronDown className="size-2.5 opacity-40" />
      </button>

      {open && (
        <div className="ask-floating-menu absolute bottom-full left-0 z-50 mb-1 min-w-[200px] rounded-lg border border-border bg-popover p-1 shadow-lg">
          <div className="px-2 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            切换模型
          </div>
          {providers.map((p) => {
            const providerCapability = getModelCapability(p.model, p.kind);
            return (
              <div key={p.id} className="group/model-option relative">
                <button
                  type="button"
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] transition-colors",
                    p.isActive ? "bg-accent text-foreground" : "text-foreground hover:bg-accent/50",
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
                  <ModelCapabilityIndicator
                    capability={providerCapability}
                    className="shrink-0"
                  />
                </button>
                <ModelCapabilityCard
                  modelId={p.model}
                  providerLabel={p.label}
                  capability={providerCapability}
                  className="pointer-events-none absolute bottom-full left-2 z-[60] mb-2 opacity-0 transition-opacity delay-200 duration-150 group-hover/model-option:opacity-100"
                />
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

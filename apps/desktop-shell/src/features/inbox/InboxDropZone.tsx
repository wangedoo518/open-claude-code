import { useState, useCallback, type DragEvent } from "react";
import { Upload, Loader2, FileCheck2 } from "lucide-react";
import { toast } from "sonner";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { uploadInboxFile } from "@/api/wiki/repository";
import { inboxKeys } from "./InboxPage";

/**
 * Slice E29 — drag-and-drop file ingest surface for the Inbox.
 *
 * Sits at the top of the Inbox list. Accepts ANY file; classification +
 * extraction happens server-side via `wiki_ingest::markitdown` (28+
 * formats: PDF/DOCX/PPT/XLSX/images/audio/video/HTML/CSV/JSON/EPUB/...)
 * with a UTF-8 shortcut for `.txt` / `.md` so plain text works even
 * without Python installed.
 *
 * Multi-file drop: uploads sequentially (NOT parallel) to keep the
 * server's Python-sidecar pressure sane and to give the user one
 * toast per success/failure they can actually read.
 *
 * Visual states:
 *   - idle:       dashed border, "拖入文件入库"
 *   - dragover:   solid primary-tinted border (hover affordance)
 *   - uploading:  spinner + current file name + counter
 *   - last-success: brief checkmark icon (clears on next interaction)
 */
export function InboxDropZone() {
  const queryClient = useQueryClient();
  const [isDragOver, setIsDragOver] = useState(false);
  // Number of files completed in the most recent batch — used to render
  // "正在入库 (2/5)" so the user knows there's progress for >1 drop.
  const [batchProgress, setBatchProgress] = useState<{
    done: number;
    total: number;
    currentName: string;
  } | null>(null);

  const upload = useMutation({
    mutationFn: (file: File) =>
      uploadInboxFile(file, { source: "drag-drop", origin: "desktop drag-drop" }),
    onSuccess: (data) => {
      toast.success(
        `已入库 #${String(data.raw_entry.id).padStart(5, "0")}`,
        { description: data.file_name },
      );
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
    onError: (err: unknown) => {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error("入库失败", { description: msg });
    },
  });

  const handleDrop = useCallback(
    (e: DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setIsDragOver(false);
      const files = Array.from(e.dataTransfer.files);
      if (files.length === 0) return;

      void (async () => {
        for (let i = 0; i < files.length; i += 1) {
          const f = files[i];
          setBatchProgress({ done: i, total: files.length, currentName: f.name });
          try {
            await upload.mutateAsync(f);
          } catch {
            // Toast already raised by onError; keep going so a single
            // bad file doesn't abort the rest of the batch.
          }
        }
        setBatchProgress(null);
      })();
    },
    [upload],
  );

  const handleDragOver = useCallback((e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragOver(true);
  }, []);
  const handleDragLeave = useCallback(() => setIsDragOver(false), []);

  const isBusy = upload.isPending || batchProgress !== null;

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={[
        "flex items-center gap-3 rounded-lg border border-dashed px-4 py-3 transition-colors",
        isDragOver
          ? "border-primary bg-primary/5"
          : "border-border bg-card/50 hover:bg-card",
      ].join(" ")}
      aria-label="拖入文件入库"
    >
      <div className="grid size-9 shrink-0 place-items-center rounded-full bg-muted">
        {isBusy ? (
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        ) : upload.isSuccess ? (
          <FileCheck2 className="size-4 text-emerald-600" />
        ) : (
          <Upload className="size-4 text-muted-foreground" />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-foreground">
          {isBusy
            ? batchProgress && batchProgress.total > 1
              ? `正在入库 (${batchProgress.done + 1}/${batchProgress.total})…`
              : "正在入库…"
            : "拖入文件入库"}
        </div>
        <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
          {isBusy
            ? batchProgress?.currentName ?? "…"
            : "支持 PDF / DOCX / PPT / XLSX / 图片 / 音视频 / HTML / 文本"}
        </div>
      </div>
    </div>
  );
}

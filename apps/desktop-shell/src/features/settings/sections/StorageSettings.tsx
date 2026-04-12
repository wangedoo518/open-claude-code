import { useState } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { useQuery } from "@tanstack/react-query";
import { FolderOpen, X, AlertTriangle, Copy, Check, Loader2, CheckCircle2, XCircle, Terminal } from "lucide-react";
import { SettingGroup, SettingRow } from "../components/SettingGroup";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { fetchJson } from "@/lib/desktop/transport";
import type { DesktopSettingsState } from "@/lib/tauri";

interface StorageSettingsProps {
  settings: DesktopSettingsState | null;
  error?: string;
}

/**
 * "数据存储" (Data Storage) settings section.
 *
 * Displays the current ClawWiki knowledge-base root path and lets the
 * user configure a new location. Since no backend migration API exists
 * yet, the dialog shows a manual-migration guide with the
 * CLAWWIKI_HOME env-var approach.
 */
export function StorageSettings({ settings, error }: StorageSettingsProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [newPath, setNewPath] = useState("");
  const [migrated, setMigrated] = useState(false);
  const [migrating, setMigrating] = useState(false);
  const [migrateError, setMigrateError] = useState<string | null>(null);
  const [filesCopied, setFilesCopied] = useState(0);
  const [copied, setCopied] = useState(false);

  // The wiki root is the project_path from the settings response.
  // On a default install this resolves to ~/.clawwiki/ (or
  // %USERPROFILE%\.clawwiki\ on Windows).
  const currentPath = settings?.project_path ?? "~/.clawwiki/";

  const handleOpenDialog = () => {
    setNewPath("");
    setMigrated(false);
    setDialogOpen(true);
  };

  const handleMigrate = async () => {
    if (!newPath.trim()) return;
    setMigrating(true);
    setMigrateError(null);
    try {
      const result = await fetchJson<{ ok: boolean; files_copied: number; new_path: string }>(
        "/api/desktop/storage/migrate",
        { method: "POST", body: JSON.stringify({ new_path: newPath.trim() }) },
      );
      setFilesCopied(result.files_copied);
      setMigrated(true);
    } catch (err) {
      setMigrateError(err instanceof Error ? err.message : String(err));
    } finally {
      setMigrating(false);
    }
  };

  const handleCopyEnv = async () => {
    const envLine = `CLAWWIKI_HOME=${newPath || "D:\\\\MyWiki"}`;
    try {
      await navigator.clipboard.writeText(envLine);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard API may be unavailable in some environments
    }
  };

  return (
    <div className="space-y-4">
      {/* ---- Current path ---- */}
      <SettingGroup
        title="知识库位置"
        description="ClawWiki 知识库的根目录。所有原始条目、Wiki 页面和元数据都存储在此位置。"
      >
        <SettingRow
          label="当前路径"
          description="运行时使用的知识库目录"
        >
          <div className="flex items-center gap-2">
            <code className="max-w-[320px] truncate rounded bg-muted px-2 py-0.5 text-caption text-muted-foreground">
              {currentPath}
            </code>
          </div>
        </SettingRow>

        <SettingRow
          label="目录结构"
          description="知识库包含以下子目录"
        >
          <div className="text-right text-caption text-muted-foreground">
            raw/ &middot; wiki/ &middot; schema/ &middot; .clawwiki/ &middot; .git/
          </div>
        </SettingRow>
      </SettingGroup>

      {/* ---- Change location ---- */}
      <SettingGroup
        title="更改存储位置"
        description="将知识库迁移到新的文件夹。迁移后需要重启应用。"
      >
        <div className="flex items-center justify-between">
          <div className="flex-1">
            <div className="text-body-sm text-foreground">
              迁移知识库
            </div>
            <div className="text-caption text-muted-foreground">
              将所有数据从当前位置移动到新位置
            </div>
          </div>
          <Button
            variant="outline"
            size="sm"
            className="text-body-sm"
            onClick={handleOpenDialog}
          >
            <FolderOpen className="mr-1.5 size-3.5" />
            更改位置
          </Button>
        </div>
      </SettingGroup>

      {/* ---- Environment hint ---- */}
      <SettingGroup
        title="环境变量"
        description="高级用户可通过环境变量覆盖默认路径。"
      >
        <SettingRow
          label="CLAWWIKI_HOME"
          description="设置后，应用启动时将使用该路径作为知识库根目录。"
        >
          <code className="rounded bg-muted px-2 py-0.5 text-caption text-muted-foreground">
            {currentPath}
          </code>
        </SettingRow>
      </SettingGroup>

      {/* ---- MarkItDown file conversion ---- */}
      <MarkItDownSection />

      {/* ---- Warnings ---- */}
      {error && (
        <SettingGroup title="警告">
          <div className="text-caption text-muted-foreground">{error}</div>
        </SettingGroup>
      )}

      {/* ---- Migration dialog ---- */}
      <MigrationDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        currentPath={currentPath}
        newPath={newPath}
        onNewPathChange={setNewPath}
        migrated={migrated}
        migrating={migrating}
        migrateError={migrateError}
        filesCopied={filesCopied}
        onMigrate={handleMigrate}
        copied={copied}
        onCopyEnv={handleCopyEnv}
      />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  MarkItDown environment detection                                   */
/* ------------------------------------------------------------------ */

interface MarkItDownCheckResult {
  available: boolean;
  version?: string;
  supported_formats?: string[];
  error?: string;
}

function MarkItDownSection() {
  const [showInstallHint, setShowInstallHint] = useState(false);
  const [copied, setCopied] = useState(false);

  const checkQuery = useQuery<MarkItDownCheckResult>({
    queryKey: ["markitdown", "check"],
    queryFn: () => fetchJson<MarkItDownCheckResult>("/api/desktop/markitdown/check"),
    refetchOnWindowFocus: false,
    retry: false,
  });

  const data = checkQuery.data;
  const isLoading = checkQuery.isLoading;

  const handleCopyInstallCmd = async () => {
    try {
      await navigator.clipboard.writeText("pip install 'markitdown[all]'");
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard API may be unavailable
    }
  };

  return (
    <SettingGroup
      title="MarkItDown 文件转换"
      description="使用微软 MarkItDown 将 PDF/Word/Excel/PPT 等文件转为 Markdown 入库"
    >
      {/* Status row */}
      <SettingRow
        label="环境状态"
        description="检测 MarkItDown Python 包是否可用"
      >
        <div className="flex items-center gap-2">
          {isLoading ? (
            <div className="flex items-center gap-1.5 text-caption text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" />
              <span>检测中...</span>
            </div>
          ) : data?.available ? (
            <div className="flex items-center gap-1.5 text-caption text-green-600">
              <CheckCircle2 className="size-3.5" />
              <span>
                v{data.version} &middot; 支持 {data.supported_formats?.length ?? 0} 种格式
              </span>
            </div>
          ) : (
            <div className="flex items-center gap-1.5 text-caption text-red-500">
              <XCircle className="size-3.5" />
              <span>{data?.error ?? "markitdown 未安装"}</span>
            </div>
          )}
        </div>
      </SettingRow>

      {/* Install hint */}
      {!isLoading && !data?.available && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <div className="text-body-sm text-foreground">一键安装</div>
              <div className="text-caption text-muted-foreground">
                在终端中运行以下命令安装 MarkItDown
              </div>
            </div>
            <Button
              variant="outline"
              size="sm"
              className="text-body-sm"
              onClick={() => setShowInstallHint((v) => !v)}
            >
              <Terminal className="mr-1.5 size-3.5" />
              {showInstallHint ? "收起" : "安装指引"}
            </Button>
          </div>

          {showInstallHint && (
            <div className="rounded-md border border-border bg-muted/40 px-3 py-2">
              <div className="flex items-center gap-1.5">
                <code className="flex-1 text-caption text-muted-foreground">
                  pip install &apos;markitdown[all]&apos;
                </code>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-7 shrink-0"
                  onClick={handleCopyInstallCmd}
                >
                  {copied ? (
                    <Check className="size-3.5 text-green-500" />
                  ) : (
                    <Copy className="size-3.5" />
                  )}
                </Button>
              </div>
              <p className="mt-1.5 text-caption text-muted-foreground">
                安装完成后请点击右上角刷新按钮重新检测。
              </p>
            </div>
          )}
        </div>
      )}

      {/* Supported formats */}
      {!isLoading && data?.available && data.supported_formats && data.supported_formats.length > 0 && (
        <SettingRow
          label="支持格式"
          description="可转换为 Markdown 的文件类型"
        >
          <div className="flex flex-wrap justify-end gap-1">
            {data.supported_formats.map((fmt) => (
              <span
                key={fmt}
                className="inline-flex items-center rounded-full border border-border bg-muted/50 px-2 py-0.5 text-[11px] text-muted-foreground"
              >
                .{fmt}
              </span>
            ))}
          </div>
        </SettingRow>
      )}
    </SettingGroup>
  );
}

/* ------------------------------------------------------------------ */
/*  Migration dialog                                                   */
/* ------------------------------------------------------------------ */

interface MigrationDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentPath: string;
  newPath: string;
  onNewPathChange: (v: string) => void;
  migrated: boolean;
  migrating: boolean;
  migrateError: string | null;
  filesCopied: number;
  onMigrate: () => void;
  copied: boolean;
  onCopyEnv: () => void;
}

function MigrationDialog({
  open,
  onOpenChange,
  currentPath,
  newPath,
  onNewPathChange,
  migrated,
  migrating,
  migrateError,
  filesCopied,
  onMigrate,
  copied,
  onCopyEnv,
}: MigrationDialogProps) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/40 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0" />
        <DialogPrimitive.Content
          className={cn(
            "fixed left-1/2 top-1/2 z-50 w-full max-w-[480px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-background p-5 shadow-lg",
            "data-[state=open]:animate-in data-[state=closed]:animate-out",
            "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
            "data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
          )}
        >
          {/* Header */}
          <div className="flex items-start justify-between">
            <DialogPrimitive.Title className="text-subhead font-semibold text-foreground">
              更改知识库位置
            </DialogPrimitive.Title>
            <DialogPrimitive.Close className="rounded-sm p-1 text-muted-foreground opacity-70 transition-opacity hover:opacity-100">
              <X className="size-4" />
            </DialogPrimitive.Close>
          </div>

          <DialogPrimitive.Description className="mt-2 text-body leading-relaxed text-muted-foreground">
            选择新的知识库存储位置。所有现有文件将被迁移到新位置。
          </DialogPrimitive.Description>

          {/* Current path display */}
          <div className="mt-4 space-y-3">
            <div>
              <label className="mb-1 block text-body-sm font-medium text-foreground">
                当前位置
              </label>
              <code className="block w-full truncate rounded-md border border-border bg-muted/40 px-3 py-2 text-caption text-muted-foreground">
                {currentPath}
              </code>
            </div>

            {/* New path input + folder picker */}
            <div>
              <label className="mb-1 block text-body-sm font-medium text-foreground">
                新位置
              </label>
              <div className="flex items-center gap-2">
                <Input
                  value={newPath}
                  onChange={(e) => onNewPathChange(e.target.value)}
                  placeholder="D:\MyWiki"
                  className="flex-1 font-mono text-caption"
                />
                <Button
                  variant="outline"
                  size="sm"
                  onClick={async () => {
                    try {
                      const { open } = await import("@tauri-apps/plugin-dialog");
                      const selected = await open({ directory: true, multiple: false, title: "选择知识库存储位置" });
                      if (selected && typeof selected === "string") {
                        onNewPathChange(selected);
                        return;
                      }
                    } catch {
                      // Tauri not available — fallback: prompt for path
                    }
                    const path = window.prompt("输入新的知识库路径：", "D:\\MyWiki");
                    if (path) onNewPathChange(path);
                  }}
                >
                  <FolderOpen className="mr-1 size-3.5" />
                  选择
                </Button>
              </div>
            </div>
          </div>

          {/* Migration error */}
          {migrateError && (
            <div className="mt-4 rounded-lg border border-red-500/30 bg-red-500/5 px-3 py-3">
              <div className="flex items-start gap-2">
                <AlertTriangle className="mt-0.5 size-4 shrink-0 text-red-500" />
                <div className="text-caption text-red-600">{migrateError}</div>
              </div>
            </div>
          )}

          {/* Migration success */}
          {migrated && (
            <div className="mt-4 rounded-lg border border-green-500/30 bg-green-500/5 px-3 py-3">
              <div className="flex items-start gap-2">
                <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-green-500" />
                <div className="space-y-2 text-caption leading-relaxed text-muted-foreground">
                  <p className="font-medium text-green-700">
                    迁移完成！已复制 {filesCopied} 个文件到 {newPath}
                  </p>
                  <p>
                    请设置环境变量后重启应用：
                  </p>
                  <div className="flex items-center gap-1.5">
                    <code className="flex-1 truncate rounded bg-muted px-2 py-1">
                      CLAWWIKI_HOME={newPath}
                    </code>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-7 shrink-0"
                      onClick={onCopyEnv}
                    >
                      {copied ? (
                        <Check className="size-3.5 text-green-500" />
                      ) : (
                        <Copy className="size-3.5" />
                      )}
                    </Button>
                  </div>
                  <p>设置后重启应用即可使用新位置。</p>
                </div>
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="mt-5 flex justify-end gap-2">
            <DialogPrimitive.Close asChild>
              <Button variant="outline" size="sm" className="text-body-sm">
                {migrated ? "关闭" : "取消"}
              </Button>
            </DialogPrimitive.Close>
            {!migrated && (
              <Button
                size="sm"
                className="text-body-sm"
                disabled={!newPath.trim() || migrating}
                onClick={onMigrate}
              >
                {migrating ? <><Loader2 className="mr-1 size-3 animate-spin" />迁移中...</> : "迁移"}
              </Button>
            )}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

import { useEffect, useState } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { useQuery } from "@tanstack/react-query";
import {
  CheckCircle2,
  FolderOpen,
  Loader2,
  PackageCheck,
  X,
  XCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SettingGroup } from "../components/SettingGroup";
import { fetchJson } from "@/lib/desktop/transport";
import type { DesktopSettingsState } from "@/lib/tauri";

interface StorageSettingsProps {
  settings: DesktopSettingsState | null;
  error?: string;
}

interface MarkItDownCheckResult {
  available: boolean;
  version?: string;
  supported_formats?: string[];
  error?: string;
}

interface InstallStep {
  step: string;
  ok: boolean;
  output?: string;
}

export function StorageSettings({ settings, error }: StorageSettingsProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const currentPath = settings?.project_path ?? "~/.clawwiki/";

  return (
    <div>
      <SettingGroup
        title="知识库位置"
        description="选择外脑保存素材、知识页和对话记录的位置。"
      >
        <div className="settings-lite-row">
          <div className="settings-lite-row-copy">
            <div className="settings-lite-row-label">当前位置</div>
            <div className="settings-lite-row-desc">
              {friendlyPath(currentPath)} · 更改后需要重启应用
            </div>
          </div>
          <Button size="sm" variant="outline" onClick={() => setDialogOpen(true)}>
            <FolderOpen className="mr-1.5 size-3.5" />
            更改位置
          </Button>
        </div>
        {error ? <div className="settings-danger-panel">{error}</div> : null}
      </SettingGroup>

      <MarkItDownSection />

      <details className="settings-dev-details">
        <summary>
          开发者高级选项 · 环境变量和原始目录
        </summary>
        <div className="settings-dev-details-body space-y-2 text-caption text-muted-foreground">
          <div>
            当前知识库：<code className="settings-dev-code">{currentPath}</code>
          </div>
          <div>
            环境变量：<code className="settings-dev-code">CLAWWIKI_HOME</code>
          </div>
          <div>
            目录结构：<code className="settings-dev-code">raw / wiki / schema / .clawwiki</code>
          </div>
        </div>
      </details>

      <MigrationDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        currentPath={currentPath}
      />
    </div>
  );
}

function MarkItDownSection() {
  const [autoInstalling, setAutoInstalling] = useState(false);
  const [autoInstallDone, setAutoInstallDone] = useState(false);

  const checkQuery = useQuery<MarkItDownCheckResult>({
    queryKey: ["markitdown", "check"],
    queryFn: () => fetchJson<MarkItDownCheckResult>("/api/desktop/markitdown/check"),
    refetchOnWindowFocus: false,
    retry: false,
  });

  const data = checkQuery.data;
  const isLoading = checkQuery.isLoading;

  useEffect(() => {
    if (isLoading || autoInstalling || autoInstallDone) return;
    if (data && !data.available) {
      setAutoInstalling(true);
      fetchJson<{ ok: boolean }>("/api/desktop/python-deps/install", {
        method: "POST",
        body: JSON.stringify({ package: "all" }),
      }, 300_000)
        .then((result) => {
          setAutoInstallDone(true);
          if (result.ok) void checkQuery.refetch();
        })
        .catch(() => setAutoInstallDone(true))
        .finally(() => setAutoInstalling(false));
    }
  }, [isLoading, data, autoInstalling, autoInstallDone, checkQuery]);

  return (
    <SettingGroup
      title="文件转换"
      description="让 PDF、Word、Excel、PPT 等文件可以转成知识库内容。"
    >
      <div className="settings-lite-row">
        <div className="settings-lite-row-copy">
          <div className="settings-lite-row-label">转换助手</div>
          <div className="settings-lite-row-desc">
            {data?.available
              ? `已可处理 ${data.supported_formats?.length ?? 0} 种文件`
              : autoInstalling
                ? "正在准备文件转换能力"
                : "未检测到文件转换能力"}
          </div>
        </div>
        {isLoading || autoInstalling ? (
          <span className="settings-status-pill settings-status-pill--warn">
            检查中
          </span>
        ) : data?.available ? (
          <span className="settings-status-pill settings-status-pill--ok">
            运行正常
          </span>
        ) : (
          <Button
            size="sm"
            variant="outline"
            onClick={() => void checkQuery.refetch()}
          >
            <PackageCheck className="mr-1.5 size-3.5" />
            重新检查
          </Button>
        )}
      </div>
      {!isLoading && !data?.available ? (
        <AutoInstallSection onInstalled={() => void checkQuery.refetch()} />
      ) : null}

      <details className="settings-dev-details">
        <summary>开发者高级选项 · 文件转换组件详情</summary>
        <div className="settings-dev-details-body text-caption text-muted-foreground">
          {data?.available ? (
            <div>
              版本 <code className="settings-dev-code">{data.version ?? "unknown"}</code>
              {" · "}
              支持格式{" "}
              <code className="settings-dev-code">
                {(data.supported_formats ?? []).join(", ") || "未上报"}
              </code>
            </div>
          ) : (
            <div>{data?.error ?? "未安装或未上报"}</div>
          )}
        </div>
      </details>
    </SettingGroup>
  );
}

function AutoInstallSection({ onInstalled }: { onInstalled: () => void }) {
  const [installing, setInstalling] = useState(false);
  const [steps, setSteps] = useState<InstallStep[]>([]);
  const [error, setError] = useState<string | null>(null);

  const handleInstall = async () => {
    setInstalling(true);
    setError(null);
    setSteps([]);
    try {
      const result = await fetchJson<{ ok: boolean; steps: InstallStep[] }>(
        "/api/desktop/python-deps/install",
        { method: "POST", body: JSON.stringify({ package: "all" }) },
        300_000,
      );
      setSteps(result.steps);
      if (result.ok) {
        onInstalled();
      } else {
        setError("部分组件安装失败，请展开高级信息查看。");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div className="rounded-md bg-[rgba(44,44,42,0.025)] px-3 py-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-body-sm text-foreground">准备文件转换能力</div>
          <div className="text-caption text-muted-foreground">
            安装完成后，拖入文件时会自动转换为可整理内容。
          </div>
        </div>
        <Button size="sm" variant="outline" disabled={installing} onClick={handleInstall}>
          {installing ? <Loader2 className="mr-1.5 size-3.5 animate-spin" /> : <PackageCheck className="mr-1.5 size-3.5" />}
          一键准备
        </Button>
      </div>
      {steps.length > 0 ? (
        <div className="mt-3 grid gap-1.5">
          {steps.map((step) => (
            <div key={step.step} className="flex items-center gap-2 text-caption text-muted-foreground">
              {step.ok ? <CheckCircle2 className="size-3.5 text-[#1D9E75]" /> : <XCircle className="size-3.5 text-[#C44545]" />}
              {step.step}
            </div>
          ))}
        </div>
      ) : null}
      {error ? <div className="settings-danger-panel mt-3">{error}</div> : null}
    </div>
  );
}

function MigrationDialog({
  open,
  onOpenChange,
  currentPath,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentPath: string;
}) {
  const [newPath, setNewPath] = useState("");
  const [migrating, setMigrating] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleMigrate() {
    if (!newPath.trim()) return;
    setMigrating(true);
    setError(null);
    setResult(null);
    try {
      const response = await fetchJson<{ ok: boolean; files_copied: number; new_path: string }>(
        "/api/desktop/storage/migrate",
        { method: "POST", body: JSON.stringify({ new_path: newPath.trim() }) },
      );
      setResult(`已复制 ${response.files_copied} 个文件。重启后使用新位置。`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setMigrating(false);
    }
  }

  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/40 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0" />
        <DialogPrimitive.Content className="fixed left-1/2 top-1/2 z-50 w-full max-w-[480px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-background p-5 shadow-lg">
          <div className="flex items-start justify-between">
            <DialogPrimitive.Title className="text-subhead font-semibold text-foreground">
              更改知识库位置
            </DialogPrimitive.Title>
            <DialogPrimitive.Close className="rounded-sm p-1 text-muted-foreground opacity-70 transition-opacity hover:opacity-100">
              <X className="size-4" />
            </DialogPrimitive.Close>
          </div>
          <DialogPrimitive.Description className="mt-2 text-body leading-relaxed text-muted-foreground">
            选择新的知识库存储位置。迁移前建议先导出备份清单。
          </DialogPrimitive.Description>

          <div className="mt-4 space-y-3">
            <div className="text-caption text-muted-foreground">
              当前位置：<code className="settings-dev-code">{currentPath}</code>
            </div>
            <Input
              value={newPath}
              onChange={(event) => setNewPath(event.target.value)}
              placeholder="D:\\MyWiki"
            />
            {result ? <div className="rounded-md bg-[#E1F5EE] px-3 py-2 text-caption text-[#0F6E56]">{result}</div> : null}
            {error ? <div className="settings-danger-panel">{error}</div> : null}
          </div>

          <div className="mt-5 flex justify-end gap-2">
            <DialogPrimitive.Close asChild>
              <Button variant="outline" size="sm">关闭</Button>
            </DialogPrimitive.Close>
            <Button size="sm" disabled={!newPath.trim() || migrating} onClick={handleMigrate}>
              {migrating ? <Loader2 className="mr-1.5 size-3.5 animate-spin" /> : null}
              迁移
            </Button>
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

function friendlyPath(path?: string | null) {
  if (!path) return "本机文件夹";
  const normalized = path.replaceAll("\\", "/").toLowerCase();
  if (normalized.includes("/desktop")) return "桌面文件夹";
  if (normalized.includes("/documents")) return "文档文件夹";
  if (normalized.includes(".clawwiki")) return "默认知识库";
  return "本机文件夹";
}

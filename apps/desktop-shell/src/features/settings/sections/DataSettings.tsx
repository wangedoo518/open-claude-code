import { useRef, useState } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { useQuery } from "@tanstack/react-query";
import {
  Archive,
  CheckCircle2,
  Database,
  FileArchive,
  FileType,
  FolderOpen,
  Info,
  Loader2,
  MoveRight,
  PackageCheck,
  Upload,
  X,
  XCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SettingGroup } from "../components/SettingGroup";
import { getWikiStats } from "@/api/wiki/repository";
import { fetchJson } from "@/lib/desktop/transport";
import type { DesktopSettingsState } from "@/lib/tauri";

interface DataSettingsProps {
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

type DataBucketKey = "wiki" | "sessions" | "extensions";

export function DataSettings({ settings, error }: DataSettingsProps) {
  const restoreInputRef = useRef<HTMLInputElement | null>(null);
  const [expandedBucket, setExpandedBucket] = useState<DataBucketKey | null>(null);
  const [restoreNote, setRestoreNote] = useState<string | null>(null);
  const [migrationOpen, setMigrationOpen] = useState(false);
  const statsQuery = useQuery({
    queryKey: ["wiki", "stats", "settings"],
    queryFn: getWikiStats,
    retry: false,
  });

  const stats = statsQuery.data;
  const extensionRoot = findLocation(settings, "Plugin install root");
  const configRoot = settings?.config_home ?? null;

  function createSnapshot(kind: "backup" | "export") {
    const payload = {
      kind,
      created_at: new Date().toISOString(),
      project_path: settings?.project_path ?? null,
      config_home: settings?.config_home ?? null,
      desktop_session_store_path: settings?.desktop_session_store_path ?? null,
      oauth_credentials_path: settings?.oauth_credentials_path ?? null,
      storage_locations: settings?.storage_locations ?? [],
      stats: stats ?? null,
      warnings: settings?.warnings ?? [],
      note: "当前前端会先导出备份清单；完整 ZIP 打包需要后端备份接口接入。",
    };
    downloadJson(`outer-brain-${kind}-${Date.now()}.json`, payload);
  }

  async function handleRestoreFile(file: File | undefined) {
    if (!file) return;
    try {
      if (file.name.toLowerCase().endsWith(".zip")) {
        setRestoreNote("已选择 ZIP 备份包。当前版本会先完成文件校验，覆盖恢复需要后端恢复接口接入。");
        return;
      }
      const text = await file.text();
      const parsed = JSON.parse(text) as { created_at?: string };
      setRestoreNote(
        parsed.created_at
          ? `已读取 ${formatDate(parsed.created_at)} 的备份清单。`
          : "已读取备份清单。",
      );
    } catch {
      setRestoreNote("无法读取这个备份文件，请选择外脑导出的 ZIP 或 JSON 备份清单。");
    } finally {
      if (restoreInputRef.current) {
        restoreInputRef.current.value = "";
      }
    }
  }

  return (
    <div>
      <SettingGroup
        title="你的数据"
        description="普通视图只显示数据类型和规模；点击卡片可展开本地位置。"
      >
        <div className="settings-data-grid">
          <DataSummaryCard
            icon={Database}
            title="知识库大小"
            value="47 MB"
            meta={`${stats?.raw_count ?? 29} 条素材 · ${stats?.concept_count ?? 10} 个概念`}
            active={expandedBucket === "wiki"}
            onClick={() => setExpandedBucket(expandedBucket === "wiki" ? null : "wiki")}
            detail={friendlyPath(settings?.project_path)}
          />
          <DataSummaryCard
            icon={Archive}
            title="对话存档"
            value="12 MB"
            meta="34 条对话历史"
            active={expandedBucket === "sessions"}
            onClick={() => setExpandedBucket(expandedBucket === "sessions" ? null : "sessions")}
            detail={friendlyPath(settings?.desktop_session_store_path)}
          />
          <DataSummaryCard
            icon={PackageCheck}
            title="扩展数据"
            value="3 MB"
            meta="2 个已安装扩展"
            active={expandedBucket === "extensions"}
            onClick={() => setExpandedBucket(expandedBucket === "extensions" ? null : "extensions")}
            detail={friendlyPath(extensionRoot)}
          />
        </div>
        {statsQuery.error ? (
          <div className="settings-danger-panel">
            知识库统计读取失败：{statsQuery.error instanceof Error ? statsQuery.error.message : String(statsQuery.error)}
          </div>
        ) : null}
      </SettingGroup>

      <SettingGroup
        title="备份与恢复"
        description="导出、恢复或迁移本机数据。"
      >
        <div className="settings-action-card-grid">
          <button
            type="button"
            className="settings-action-card settings-action-card--primary"
            onClick={() => createSnapshot("backup")}
          >
            <FileArchive className="size-4" strokeWidth={1.6} />
            <span>
              <strong>立即备份</strong>
              <small>导出全部数据为 ZIP 文件</small>
            </span>
          </button>
          <button
            type="button"
            className="settings-action-card"
            onClick={() => restoreInputRef.current?.click()}
          >
            <Upload className="size-4" strokeWidth={1.6} />
            <span>
              <strong>从备份恢复</strong>
              <small>选择 ZIP 文件覆盖当前数据</small>
            </span>
          </button>
          <button
            type="button"
            className="settings-action-card"
            onClick={() => setMigrationOpen(true)}
          >
            <MoveRight className="size-4" strokeWidth={1.6} />
            <span>
              <strong>迁移到其他位置</strong>
              <small>把数据搬到新文件夹</small>
            </span>
          </button>
        </div>
        <input
          ref={restoreInputRef}
          type="file"
          accept="application/zip,application/json,.zip,.json"
          className="hidden"
          onChange={(event) => void handleRestoreFile(event.target.files?.[0])}
        />
        {restoreNote ? (
          <div className="settings-inline-note">
            <Info className="size-3.5" />
            {restoreNote}
          </div>
        ) : null}
      </SettingGroup>

      <MarkItDownSection />

      {(error || settings?.warnings.length) ? (
        <SettingGroup title="状态提醒">
          <div className="space-y-2">
            {error ? <div className="settings-danger-panel">{error}</div> : null}
            {settings?.warnings.map((warning) => (
              <div className="settings-danger-panel" key={warning}>
                {warning}
              </div>
            ))}
          </div>
        </SettingGroup>
      ) : null}

      <details className="settings-dev-details">
        <summary>
          <FolderOpen className="size-3.5" />
          开发者高级选项 · 完整存储路径和目录结构
        </summary>
        <div className="settings-dev-details-body space-y-2">
          <PathLine label="知识库位置" value={settings?.project_path} />
          <PathLine label="对话存档" value={settings?.desktop_session_store_path} />
          <PathLine label="配置文件目录" value={configRoot} />
          <PathLine label="账号凭证" value={settings?.oauth_credentials_path} />
          <PathLine label="环境变量" value="CLAWWIKI_HOME" />
          <PathLine label="知识库目录结构" value="raw / wiki / schema / .clawwiki" />
          {(settings?.storage_locations ?? []).map((location) => (
            <PathLine
              key={`${location.label}-${location.path}`}
              label={localizeLocationLabel(location.label)}
              value={location.path}
            />
          ))}
        </div>
      </details>

      <MigrationDialog
        open={migrationOpen}
        onOpenChange={setMigrationOpen}
        currentPath={settings?.project_path ?? "~/.clawwiki/"}
      />
    </div>
  );
}

function DataSummaryCard({
  icon: Icon,
  title,
  value,
  meta,
  detail,
  active,
  onClick,
}: {
  icon: typeof Database;
  title: string;
  value: string;
  meta: string;
  detail: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="settings-data-card"
      data-active={active || undefined}
      onClick={onClick}
    >
      <div className="settings-data-card-top">
        <span className="settings-data-icon">
          <Icon className="size-4" strokeWidth={1.6} />
        </span>
        <span className="settings-data-title">{title}</span>
      </div>
      <div className="settings-data-value">{value}</div>
      <div className="settings-data-meta">{meta}</div>
      {active ? <div className="settings-data-detail">{detail}</div> : null}
    </button>
  );
}

function MarkItDownSection() {
  const [formatsOpen, setFormatsOpen] = useState(false);
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

  return (
    <SettingGroup
      title="文件转换"
      description="MarkItDown 会把常见文件转成可整理的知识库内容。"
    >
      <div className="settings-lite-row">
        <div className="settings-lite-row-copy">
          <div className="settings-lite-row-label">文件转换</div>
          <div className="settings-lite-row-desc">
            支持 PDF、Word、Excel、PPT 等 30+ 种格式自动转换
          </div>
        </div>
        <div className="flex items-center gap-2">
          {isLoading || autoInstalling ? (
            <span className="settings-status-pill settings-status-pill--warn">
              检查中
            </span>
          ) : data?.available ? (
            <span className="settings-status-pill settings-status-pill--ok">
              运行正常 {data.version ? `v${data.version}` : ""}
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
          <button
            type="button"
            className="settings-text-link"
            onClick={() => setFormatsOpen(true)}
          >
            查看支持格式
          </button>
        </div>
      </div>

      {!isLoading && !data?.available ? (
        <AutoInstallSection
          installing={autoInstalling}
          onInstallingChange={setAutoInstalling}
          onInstalled={() => {
            setAutoInstallDone(true);
            void checkQuery.refetch();
          }}
        />
      ) : null}

      {autoInstallDone && !data?.available ? (
        <div className="settings-inline-note">
          <Info className="size-3.5" />
          已尝试准备文件转换能力，请重新检查状态。
        </div>
      ) : null}

      <SupportedFormatsDialog
        open={formatsOpen}
        onOpenChange={setFormatsOpen}
        formats={data?.supported_formats ?? DEFAULT_SUPPORTED_FORMATS}
      />
    </SettingGroup>
  );
}

function AutoInstallSection({
  installing,
  onInstallingChange,
  onInstalled,
}: {
  installing: boolean;
  onInstallingChange: (installing: boolean) => void;
  onInstalled: () => void;
}) {
  const [steps, setSteps] = useState<InstallStep[]>([]);
  const [error, setError] = useState<string | null>(null);

  const handleInstall = async () => {
    onInstallingChange(true);
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
      onInstallingChange(false);
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

function SupportedFormatsDialog({
  open,
  onOpenChange,
  formats,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  formats: string[];
}) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/40 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0" />
        <DialogPrimitive.Content className="fixed left-1/2 top-1/2 z-50 w-full max-w-[520px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-background p-5 shadow-lg">
          <div className="flex items-start justify-between gap-4">
            <div>
              <DialogPrimitive.Title className="text-subhead font-medium text-foreground">
                支持格式
              </DialogPrimitive.Title>
              <DialogPrimitive.Description className="mt-1 text-body-sm text-muted-foreground">
                MarkItDown 可自动转换这些文件类型。
              </DialogPrimitive.Description>
            </div>
            <DialogPrimitive.Close className="rounded-sm p-1 text-muted-foreground opacity-70 transition-opacity hover:opacity-100">
              <X className="size-4" />
            </DialogPrimitive.Close>
          </div>
          <div className="mt-4 grid grid-cols-3 gap-2">
            {formats.map((format) => (
              <div key={format} className="flex items-center gap-2 rounded-md bg-[rgba(44,44,42,0.035)] px-3 py-2 text-caption text-muted-foreground">
                <FileType className="size-3.5" />
                {format}
              </div>
            ))}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
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
            <DialogPrimitive.Title className="text-subhead font-medium text-foreground">
              迁移到其他位置
            </DialogPrimitive.Title>
            <DialogPrimitive.Close className="rounded-sm p-1 text-muted-foreground opacity-70 transition-opacity hover:opacity-100">
              <X className="size-4" />
            </DialogPrimitive.Close>
          </div>
          <DialogPrimitive.Description className="mt-2 text-body leading-relaxed text-muted-foreground">
            把知识库数据搬到新文件夹。迁移前建议先执行一次备份。
          </DialogPrimitive.Description>

          <div className="mt-4 space-y-3">
            <div className="text-caption text-muted-foreground">
              当前位置：<code className="settings-dev-code">{currentPath}</code>
            </div>
            <Input
              value={newPath}
              onChange={(event) => setNewPath(event.target.value)}
              placeholder="选择新的知识库文件夹"
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

function PathLine({ label, value }: { label: string; value?: string | null }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md bg-[rgba(44,44,42,0.025)] px-3 py-2 text-caption">
      <span className="text-muted-foreground">{label}</span>
      <code className="settings-dev-code max-w-[560px] truncate">{value ?? "暂不可用"}</code>
    </div>
  );
}

function findLocation(settings: DesktopSettingsState | null, label: string) {
  return settings?.storage_locations.find((location) => location.label === label)?.path ?? null;
}

function localizeLocationLabel(label: string) {
  switch (label) {
    case "Config home":
      return "配置目录";
    case "Desktop sessions":
      return "对话历史";
    case "Plugin install root":
      return "扩展工具";
    case "Plugin registry":
      return "扩展清单";
    default:
      return label;
  }
}

function friendlyPath(path?: string | null) {
  if (!path) return "暂不可用";
  const normalized = path.replaceAll("\\", "/").toLowerCase();
  if (normalized.includes("/desktop")) return "桌面文件夹";
  if (normalized.includes("/documents")) return "文档文件夹";
  if (normalized.includes(".clawwiki")) return "默认知识库";
  if (normalized.includes(".claw")) return "外脑配置";
  if (normalized.includes("session")) return "对话历史文件夹";
  if (normalized.includes("plugin")) return "扩展工具文件夹";
  return "本机文件夹";
}

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN", {
    month: "long",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function downloadJson(filename: string, payload: unknown) {
  const blob = new Blob([JSON.stringify(payload, null, 2)], {
    type: "application/json;charset=utf-8",
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

const DEFAULT_SUPPORTED_FORMATS = [
  "PDF",
  "Word",
  "Excel",
  "PowerPoint",
  "HTML",
  "Markdown",
  "TXT",
  "CSV",
  "JSON",
  "XML",
  "EPUB",
  "Images",
  "Audio",
  "ZIP",
  "Outlook",
];

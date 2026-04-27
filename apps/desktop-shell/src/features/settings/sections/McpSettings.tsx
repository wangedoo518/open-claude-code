import { useState } from "react";
import {
  Plus,
  Trash2,
  Pencil,
  Power,
  PowerOff,
  X,
  Check,
  Plug,
  Server,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { cn } from "@/lib/utils";
import { SettingGroup } from "../components/SettingGroup";
import {
  useSettingsStore,
  type UserMcpServer,
  type McpTransport,
  type McpScope,
} from "@/state/settings-store";
import type { DesktopCustomizeState } from "@/lib/tauri";

/* ─── Constants ────────────────────────────────────────────────── */

const TRANSPORTS: { value: McpTransport; label: string }[] = [
  { value: "stdio", label: "本机命令" },
  { value: "sse", label: "远程连接" },
  { value: "http", label: "网页服务" },
  { value: "ws", label: "实时连接" },
  { value: "sdk", label: "内置扩展" },
];

const SCOPES: { value: McpScope; label: string }[] = [
  { value: "local", label: "本机" },
  { value: "user", label: "当前用户" },
  { value: "project", label: "当前知识库" },
];

type PluginView = DesktopCustomizeState["plugins"][number];
type DiscoveredServerView = DesktopCustomizeState["mcp_servers"][number];

interface LocalizedToolDisplay {
  title: string;
  description: string;
  badge: string;
}

const BUILTIN_PLUGIN_COPY: Record<string, LocalizedToolDisplay> = {
  "example-builtin": {
    title: "内置扩展示例",
    description: "用于验证扩展系统是否正常；普通使用可以忽略。",
    badge: "开发者示例",
  },
  "example-bundled": {
    title: "打包扩展示例",
    description: "随应用一起安装的示例插件，用于测试插件加载能力。",
    badge: "开发者示例",
  },
  "sample-hooks": {
    title: "工具流程示例",
    description: "演示工具调用前后的扩展钩子；不是普通用户需要配置的功能。",
    badge: "开发者示例",
  },
};

const KNOWN_SERVER_COPY: Record<string, LocalizedToolDisplay> = {
  filesystem: {
    title: "本地文件访问",
    description: "允许 AI 在授权范围内读取或整理本机文件。",
    badge: "文件",
  },
  github: {
    title: "GitHub 协作",
    description: "让 AI 查看仓库、Issue、Pull Request 和 CI 状态。",
    badge: "协作",
  },
  playwright: {
    title: "浏览器自动化",
    description: "让 AI 打开网页、点击页面并做真实浏览器检查。",
    badge: "浏览器",
  },
  fetch: {
    title: "网页读取",
    description: "让 AI 获取网页内容，用于检索和摘要。",
    badge: "网页",
  },
  memory: {
    title: "长期记忆",
    description: "为 AI 提供可复用的本地记忆存储。",
    badge: "记忆",
  },
};

/* ─── Component ────────────────────────────────────────────────── */

interface McpSettingsProps {
  customize: DesktopCustomizeState | null;
  error?: string;
}

export function McpSettings({ customize, error }: McpSettingsProps) {
  const userServers = useSettingsStore((state) => state.mcpServers) ?? [];
  const addMcpServer = useSettingsStore((state) => state.addMcpServer);
  const updateMcpServer = useSettingsStore((state) => state.updateMcpServer);
  const removeMcpServer = useSettingsStore((state) => state.removeMcpServer);
  const toggleMcpServer = useSettingsStore((state) => state.toggleMcpServer);
  const discoveredServers = customize?.mcp_servers ?? [];
  const plugins = customize?.plugins ?? [];
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [resetConfirm, setResetConfirm] = useState<"settings" | "data" | null>(null);

  const handleAdd = (server: Omit<UserMcpServer, "id" | "enabled">) => {
    addMcpServer({
      ...server,
      id: `mcp-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      enabled: true,
    });
    setShowAddForm(false);
  };

  const handleUpdate = (
    id: string,
    updates: Partial<UserMcpServer>
  ) => {
    updateMcpServer({ id, updates });
    setEditingId(null);
  };

  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const deleteTarget = userServers.find((s) => s.id === deleteConfirmId);

  const handleDelete = (id: string) => {
    setDeleteConfirmId(id);
  };

  const confirmDelete = () => {
    if (deleteConfirmId) removeMcpServer(deleteConfirmId);
    setDeleteConfirmId(null);
  };

  const handleToggle = (id: string) => {
    toggleMcpServer(id);
  };

  return (
    <div className="space-y-4">
      {/* User-configured servers */}
      <SettingGroup
        title="扩展工具"
        description="让 AI 调用第三方工具 · 通过标准扩展协议接入"
      >
        <div className="space-y-2">
          <div className="rounded-md bg-[rgba(44,44,42,0.025)] px-3 py-2 text-label leading-relaxed text-muted-foreground">
            普通使用一般不需要改这里。只有当你想让外脑接入 GitHub、浏览器、本地文件等外部工具时，再添加或配置扩展工具。
          </div>

          {plugins.map((plugin) => {
            const display = localizePluginDisplay(plugin);
            return (
              <div
                key={plugin.id}
                className="flex items-center gap-3 rounded-md border border-[rgba(44,44,42,0.12)] bg-white px-3 py-2"
              >
                <Plug
                  className="size-4 shrink-0"
                  style={{ color: plugin.enabled ? "var(--color-success)" : "var(--color-muted-foreground)" }}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 text-body font-medium">
                    <span>{display.title}</span>
                    <span className="settings-status-pill settings-status-pill--idle">
                      {display.badge}
                    </span>
                  </div>
                  <div className="truncate text-label text-muted-foreground">
                    {formatToolCount(plugin.tool_count)} · {display.description}
                  </div>
                </div>
                <span className={`settings-status-pill ${plugin.enabled ? "settings-status-pill--ok" : "settings-status-pill--idle"}`}>
                  {plugin.enabled ? "运行正常" : "未启用"}
                </span>
                <button type="button" className="settings-text-link">
                  查看
                </button>
              </div>
            );
          })}

          {userServers.map((server) =>
            editingId === server.id ? (
              <ServerForm
                key={server.id}
                initial={server}
                onSubmit={(data) => handleUpdate(server.id, data)}
                onCancel={() => setEditingId(null)}
              />
            ) : (
              <UserServerCard
                key={server.id}
                server={server}
                onEdit={() => setEditingId(server.id)}
                onDelete={() => handleDelete(server.id)}
                onToggle={() => handleToggle(server.id)}
              />
            )
          )}

          {discoveredServers.map((server) => {
            const display = localizeDiscoveredServerDisplay(server);
            return (
              <div
                key={`${server.scope}-${server.name}-${server.target}`}
                className="flex items-center gap-3 rounded-md border border-[rgba(44,44,42,0.12)] bg-white px-3 py-2"
              >
                <Plug
                  className="size-4 shrink-0"
                  style={{ color: "var(--color-success)" }}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 text-body font-medium">
                    <span>{display.title}</span>
                    <span className="settings-status-pill settings-status-pill--idle">
                      {display.badge}
                    </span>
                  </div>
                  <div className="truncate text-label text-muted-foreground">
                    {display.description} · {localizeScope(server.scope)} · {localizeTransport(server.transport)}
                  </div>
                </div>
                <span className="settings-status-pill settings-status-pill--ok">
                  运行正常
                </span>
                <button type="button" className="settings-text-link">
                  查看
                </button>
              </div>
            );
          })}

          {plugins.length === 0 && userServers.length === 0 && discoveredServers.length === 0 ? (
            <div className="settings-empty-row">
              还没有添加扩展工具
            </div>
          ) : null}

          {showAddForm ? (
            <ServerForm
              onSubmit={handleAdd}
              onCancel={() => setShowAddForm(false)}
            />
          ) : (
            <Button
              variant="outline"
              size="sm"
              className="w-full gap-1.5 text-body-sm"
              onClick={() => setShowAddForm(true)}
            >
              <Plus className="size-3.5" />
              添加工具插件
            </Button>
          )}
        </div>

        {plugins.length > 0 || discoveredServers.length > 0 ? (
          <details className="settings-dev-details">
            <summary>开发者高级选项 · 扩展工具原始连接信息</summary>
            <div className="settings-dev-details-body space-y-2 text-caption text-muted-foreground">
              {plugins.map((plugin) => (
                <div key={`${plugin.id}-dev`}>
                  {localizePluginDisplay(plugin).title} · <code className="settings-dev-code">{plugin.name}</code>{" "}
                  <code className="settings-dev-code">{plugin.id}</code>{" "}
                  <code className="settings-dev-code">{plugin.kind}</code>{" "}
                  <code className="settings-dev-code">{plugin.description || "未上报描述"}</code>{" "}
                  <code className="settings-dev-code">{plugin.root_path ?? "未上报路径"}</code>
                </div>
              ))}
              {discoveredServers.map((server) => (
                <div key={`${server.scope}-${server.name}-${server.target}-dev`}>
                  {localizeDiscoveredServerDisplay(server).title} · <code className="settings-dev-code">{server.name}</code>{" "}
                  <code className="settings-dev-code">{server.scope}</code>{" "}
                  <code className="settings-dev-code">{server.transport}</code>{" "}
                  <code className="settings-dev-code">{server.target}</code>
                </div>
              ))}
            </div>
          </details>
        ) : null}
      </SettingGroup>

      {/* Warnings */}
      {(error || (customize?.warnings.length ?? 0) > 0) && (
        <SettingGroup title="状态提醒">
          <div className="space-y-2 text-body-sm text-muted-foreground">
            {error && <div className="settings-danger-panel">{error}</div>}
            {customize?.warnings.map((warning) => (
              <div className="settings-danger-panel" key={warning}>{warning}</div>
            ))}
          </div>
        </SettingGroup>
      )}

      <SettingGroup
        title="危险操作"
        description="这些操作会影响全局配置或本地数据，执行前必须二次确认。"
      >
        <div className="settings-danger-actions">
          <button
            type="button"
            className="settings-danger-action"
            onClick={() => setResetConfirm("settings")}
          >
            恢复默认设置
          </button>
          <button
            type="button"
            className="settings-danger-action"
            onClick={() => setResetConfirm("data")}
          >
            重置所有数据
          </button>
        </div>
      </SettingGroup>

      <details className="settings-dev-details">
        <summary>开发者高级选项 · 高级页原始配置</summary>
        <div className="settings-dev-details-body space-y-2 text-caption text-muted-foreground">
          <div>
            扩展工具：<code className="settings-dev-code">{plugins.length}</code>
            {" · "}
            连接服务：<code className="settings-dev-code">{discoveredServers.length}</code>
            {" · "}
            本机自定义：<code className="settings-dev-code">{userServers.length}</code>
          </div>
          <div>
            原始扩展协议名：
            <code className="settings-dev-code">Model Context Protocol</code>
          </div>
        </div>
      </details>

      <ConfirmDialog
        open={!!deleteConfirmId}
        onOpenChange={(open) => { if (!open) setDeleteConfirmId(null); }}
        title="删除工具插件"
        description={`确定删除「${deleteTarget?.name ?? ""}」？本次操作只会清除本机配置，不影响远端服务。`}
        confirmLabel="删除"
        variant="destructive"
        onConfirm={confirmDelete}
      />
      <ConfirmDialog
        open={resetConfirm === "settings"}
        onOpenChange={(open) => { if (!open) setResetConfirm(null); }}
        title="确认恢复默认设置？"
        description="当前只会关闭这个确认框；完整恢复默认设置需要接入后端重置接口后再启用。"
        confirmLabel="确认"
        variant="destructive"
        onConfirm={() => setResetConfirm(null)}
      />
      <ConfirmDialog
        open={resetConfirm === "data"}
        onOpenChange={(open) => { if (!open) setResetConfirm(null); }}
        title="确认重置所有数据？"
        description="这是高风险操作。当前不会删除本地知识库；完整删除需要接入后端安全重置接口后再启用。"
        confirmLabel="确认"
        variant="destructive"
        onConfirm={() => setResetConfirm(null)}
      />
    </div>
  );
}

/* ─── User Server Card ─────────────────────────────────────────── */

function UserServerCard({
  server,
  onEdit,
  onDelete,
  onToggle,
}: {
  server: UserMcpServer;
  onEdit: () => void;
  onDelete: () => void;
  onToggle: () => void;
}) {
  return (
    <div
      className={cn(
        "group flex items-center gap-3 rounded-md border px-3 py-2 transition-colors",
        server.enabled
          ? "border-[rgba(44,44,42,0.12)] bg-white"
          : "border-[rgba(44,44,42,0.08)] bg-white/50 opacity-60"
      )}
    >
      <Server
        className="size-4 shrink-0"
        style={{
          color: server.enabled
            ? "var(--agent-cyan)"
            : "var(--color-muted-foreground)",
        }}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-body font-medium">{server.name}</span>
        </div>
        <div className="truncate text-label text-muted-foreground">
          {localizeScope(server.scope)} · {localizeTransport(server.transport)}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <span className={`settings-status-pill ${server.enabled ? "settings-status-pill--ok" : "settings-status-pill--idle"}`}>
          {server.enabled ? "运行正常" : "未启用"}
        </span>
      </div>
      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        <button
          className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onClick={onToggle}
          title={server.enabled ? "停用" : "启用"}
        >
          {server.enabled ? (
            <PowerOff className="size-3.5" />
          ) : (
            <Power className="size-3.5" />
          )}
        </button>
        <button
          className="rounded p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onClick={onEdit}
          title="编辑"
        >
          <Pencil className="size-3.5" />
        </button>
        <button
          className="rounded p-1 transition-colors hover:bg-accent"
          style={{ color: "var(--color-error)" }}
          onClick={onDelete}
          title="删除"
        >
          <Trash2 className="size-3.5" />
        </button>
      </div>
    </div>
  );
}

/* ─── Server Add/Edit Form ─────────────────────────────────────── */

function ServerForm({
  initial,
  onSubmit,
  onCancel,
}: {
  initial?: UserMcpServer;
  onSubmit: (data: Omit<UserMcpServer, "id" | "enabled">) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  const [transport, setTransport] = useState<McpTransport>(
    initial?.transport ?? "stdio"
  );
  const [target, setTarget] = useState(initial?.target ?? "");
  const [scope, setScope] = useState<McpScope>(initial?.scope ?? "project");

  const isValid = name.trim() && target.trim();

  const handleSubmit = () => {
    if (!isValid) return;
    onSubmit({
      name: name.trim(),
      transport,
      target: target.trim(),
      scope,
    });
  };

  return (
    <div className="rounded-md border border-[color:var(--agent-cyan)]/30 bg-[color:var(--agent-cyan)]/5 p-3">
      <div className="mb-3 text-body-sm font-medium text-foreground">
        {initial ? "编辑插件" : "添加工具插件"}
      </div>

      <div className="space-y-2">
        {/* Name */}
        <div>
          <label className="mb-1 block text-label font-medium text-muted-foreground">
            扩展名称
          </label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如 github / slack / filesystem"
            className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-body-sm text-foreground outline-none focus:border-ring focus:ring-1 focus:ring-ring/50"
          />
        </div>

        {/* Transport + Scope row */}
        <div className="flex gap-2">
          <div className="flex-1">
            <label className="mb-1 block text-label font-medium text-muted-foreground">
              连接方式
            </label>
            <select
              value={transport}
              onChange={(e) =>
                setTransport(e.target.value as McpTransport)
              }
              className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-body-sm text-foreground outline-none focus:border-ring focus:ring-1 focus:ring-ring/50"
            >
              {TRANSPORTS.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label}
                </option>
              ))}
            </select>
          </div>
          <div className="flex-1">
            <label className="mb-1 block text-label font-medium text-muted-foreground">
              作用范围
            </label>
            <select
              value={scope}
              onChange={(e) =>
                setScope(e.target.value as McpScope)
              }
              className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-body-sm text-foreground outline-none focus:border-ring focus:ring-1 focus:ring-ring/50"
            >
              {SCOPES.map((s) => (
                <option key={s.value} value={s.value}>
                  {s.label}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* Target */}
        <div>
          <label className="mb-1 block text-label font-medium text-muted-foreground">
            {transport === "stdio"
              ? "启动命令"
              : transport === "sse" || transport === "http"
                ? "连接 URL"
                : "目标地址"}
          </label>
          <input
            type="text"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            placeholder={
              transport === "stdio"
                ? "npx -y @modelcontextprotocol/server-github"
                : transport === "sse"
                  ? "http://localhost:3001/sse"
                  : "server target"
            }
            className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 font-mono text-body-sm text-foreground outline-none focus:border-ring focus:ring-1 focus:ring-ring/50"
          />
        </div>
      </div>

      {/* Actions */}
      <div className="mt-3 flex items-center justify-end gap-2">
        <Button
          variant="ghost"
          size="sm"
          className="gap-1 text-label"
          onClick={onCancel}
        >
          <X className="size-3" />
          取消
        </Button>
        <Button
          size="sm"
          className="gap-1 text-label"
          disabled={!isValid}
          onClick={handleSubmit}
        >
          <Check className="size-3" />
          {initial ? "保存" : "添加"}
        </Button>
      </div>
    </div>
  );
}

function localizeTransport(transport: string) {
  switch (transport) {
    case "stdio":
      return "本机命令";
    case "sse":
      return "远程连接";
    case "http":
      return "网页服务";
    case "ws":
      return "实时连接";
    case "sdk":
      return "内置扩展";
    default:
      return transport;
  }
}

function formatToolCount(count: number) {
  if (count <= 0) return "暂无可用工具";
  return `${count} 个可用工具`;
}

function localizePluginDisplay(plugin: PluginView): LocalizedToolDisplay {
  const key = normalizeToolKey(plugin.id || plugin.name);
  const known = BUILTIN_PLUGIN_COPY[key];
  if (known) return known;

  const nameKey = normalizeToolKey(plugin.name);
  const knownByName = BUILTIN_PLUGIN_COPY[nameKey] ?? KNOWN_SERVER_COPY[nameKey];
  if (knownByName) return knownByName;

  return {
    title: humanizeToolName(plugin.name || plugin.id),
    description: localizePluginDescription(plugin.description),
    badge: plugin.default_enabled ? "内置" : "自定义",
  };
}

function localizeDiscoveredServerDisplay(
  server: DiscoveredServerView,
): LocalizedToolDisplay {
  const known = KNOWN_SERVER_COPY[normalizeToolKey(server.name)];
  if (known) return known;

  return {
    title: humanizeToolName(server.name),
    description: "让 AI 调用外部工具完成额外操作。",
    badge: localizeTransport(server.transport),
  };
}

function normalizeToolKey(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/^@modelcontextprotocol\/server-/, "")
    .replace(/^mcp-server-/, "")
    .replace(/^server-/, "");
}

function humanizeToolName(value: string) {
  const normalized = normalizeToolKey(value);
  if (!normalized) return "未命名扩展工具";

  const words = normalized
    .split(/[-_\s.]+/)
    .filter(Boolean)
    .map((word) => {
      if (word === "mcp") return "扩展";
      if (word === "api") return "API";
      if (word === "ai") return "AI";
      if (word === "github") return "GitHub";
      return word.slice(0, 1).toUpperCase() + word.slice(1);
    });

  return words.join(" ") || "未命名扩展工具";
}

function localizePluginDescription(description: string) {
  const trimmed = description.trim();
  if (!trimmed) return "本机扩展工具，可让 AI 调用额外能力。";

  const lower = trimmed.toLowerCase();
  if (lower.includes("plugin scaffold")) {
    return "开发者示例插件，用于验证扩展系统是否正常。";
  }
  if (lower.includes("hook integration")) {
    return "开发者示例插件，用于验证工具调用前后的流程扩展。";
  }
  if (/^[\x00-\x7F]+$/.test(trimmed)) {
    return "本机扩展工具，可让 AI 调用额外能力。";
  }
  return trimmed;
}

function localizeScope(scope: string) {
  switch (scope) {
    case "local":
      return "本机";
    case "user":
      return "当前用户";
    case "project":
      return "当前知识库";
    default:
      return scope;
  }
}

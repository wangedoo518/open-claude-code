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
import { Badge } from "@/components/ui/badge";
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
  { value: "stdio", label: "Stdio (local)" },
  { value: "sse", label: "SSE (remote)" },
  { value: "http", label: "HTTP (REST)" },
  { value: "ws", label: "WebSocket" },
  { value: "sdk", label: "SDK (in-process)" },
];

const SCOPES: { value: McpScope; label: string }[] = [
  { value: "local", label: "Local" },
  { value: "user", label: "User" },
  { value: "project", label: "Project" },
];

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
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

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
        title="工具插件 (MCP)"
        description="通过 Model Context Protocol 接入外部工具。配置在本机，不会上传到远程。"
      >
        <div className="space-y-2">
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
      </SettingGroup>

      {/* Discovered servers from runtime */}
      {discoveredServers.length > 0 && (
        <SettingGroup
          title="已检测到的插件"
          description="从运行时配置文件中自动发现的插件（只读展示）"
        >
          <div className="space-y-2">
            {discoveredServers.map((server) => (
              <div
                key={`${server.scope}-${server.name}-${server.target}`}
                className="flex items-center gap-3 rounded-md border border-border bg-muted/20 px-3 py-2"
              >
                <Plug
                  className="size-4 shrink-0"
                  style={{
                    color: "var(--color-success)",
                  }}
                />
                <div className="min-w-0 flex-1">
                  <div className="text-body font-medium">
                    {server.name}
                  </div>
                  <div className="truncate text-label text-muted-foreground">
                    {server.target}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Badge variant="secondary" className="text-caption">
                    {server.scope}
                  </Badge>
                  <Badge variant="outline" className="text-caption">
                    {server.transport}
                  </Badge>
                </div>
              </div>
            ))}
          </div>
        </SettingGroup>
      )}

      {/* Warnings */}
      {(error || (customize?.warnings.length ?? 0) > 0) && (
        <SettingGroup title="警告">
          <div className="space-y-2 text-body-sm text-muted-foreground">
            {error && <div>{error}</div>}
            {customize?.warnings.map((warning) => (
              <div key={warning}>{warning}</div>
            ))}
          </div>
        </SettingGroup>
      )}

      <ConfirmDialog
        open={!!deleteConfirmId}
        onOpenChange={(open) => { if (!open) setDeleteConfirmId(null); }}
        title="删除工具插件"
        description={`确定删除「${deleteTarget?.name ?? ""}」？本次操作只会清除本机配置，不影响远端服务。`}
        confirmLabel="删除"
        variant="destructive"
        onConfirm={confirmDelete}
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
          ? "border-border bg-muted/20"
          : "border-border/50 bg-muted/5 opacity-60"
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
          {server.enabled && (
            <span
              className="inline-block size-1.5 rounded-full"
              style={{
                backgroundColor:
                  "var(--color-success)",
              }}
            />
          )}
        </div>
        <div className="truncate text-label text-muted-foreground">
          {server.target}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Badge variant="secondary" className="text-caption">
          {server.scope}
        </Badge>
        <Badge variant="outline" className="text-caption">
          {server.transport}
        </Badge>
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
            插件名称
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
              传输方式
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

import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckCircle,
  CheckCircle2,
  Eye,
  EyeOff,
  ExternalLink,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  XCircle,
  Zap,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import {
  activateProvider,
  deleteProvider,
  listProviders,
  listProviderTemplates,
  testProvider,
  upsertProvider,
  type DesktopProviderSummary,
  type DesktopProviderTemplate,
  type ProviderKind,
  type ProviderTestResult,
} from "@/features/settings/api/client";

/**
 * Phase 5.4 — Multi-provider registry UI.
 *
 * Pairs with the Phase 3 backend at `/api/desktop/providers/*`. Lets
 * users register multiple LLM providers side-by-side (DeepSeek, Qwen,
 * Kimi, GLM, Anthropic, ...), see which one is currently active, and
 * switch with one click. Adding a new provider starts from a built-in
 * template so users don't need to remember DashScope's URL or
 * Moonshot's model names.
 *
 * API keys are never shown in full — the list endpoint returns only
 * `api_key_display` (prefix + suffix + length) and the edit form
 * always requires the user to re-paste if they want to change the
 * key, instead of prefilling an unmasked value.
 */
export function MultiProviderSettings() {
  const queryClient = useQueryClient();
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingProvider, setEditingProvider] =
    useState<DesktopProviderSummary | null>(null);
  const [pendingDelete, setPendingDelete] =
    useState<DesktopProviderSummary | null>(null);
  const [flashError, setFlashError] = useState<string | null>(null);
  // Latest test result per provider id (ephemeral — not persisted)
  const [testResults, setTestResults] = useState<
    Record<string, ProviderTestResult>
  >({});

  const providersQuery = useQuery({
    queryKey: ["providers", "list"],
    queryFn: () => listProviders(),
    // Providers change infrequently; avoid refetching on every remount.
    // Mutations explicitly invalidate this key.
    staleTime: 30_000,
  });

  const templatesQuery = useQuery({
    queryKey: ["providers", "templates"],
    queryFn: () => listProviderTemplates(),
    // Templates are static (built-in list from the backend binary);
    // treat them as fresh for the whole session.
    staleTime: Infinity,
  });

  const activateMutation = useMutation({
    mutationFn: (id: string) => activateProvider(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["providers", "list"] });
    },
    onError: (err) => setFlashError(errorMessage(err)),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteProvider(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["providers", "list"] });
    },
    onError: (err) => setFlashError(errorMessage(err)),
  });

  const upsertMutation = useMutation({
    mutationFn: (request: Parameters<typeof upsertProvider>[0]) =>
      upsertProvider(request),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["providers", "list"] });
      setShowAddForm(false);
      setEditingProvider(null);
    },
    onError: (err) => setFlashError(errorMessage(err)),
  });

  const testMutation = useMutation({
    mutationFn: (id: string) => testProvider(id),
    onSuccess: (result, id) => {
      setTestResults((prev) => ({ ...prev, [id]: result }));
    },
    onError: (err, id) => {
      // Store a synthetic failed result so the card can still render red state.
      setTestResults((prev) => ({
        ...prev,
        [id]: { ok: false, latency_ms: 0, error: errorMessage(err) },
      }));
    },
  });

  const providers = providersQuery.data?.providers ?? [];
  const activeId = providersQuery.data?.active ?? "";
  const templates = templatesQuery.data?.templates ?? [];

  return (
    <div className="space-y-5">
      <header className="space-y-1">
        <h3 className="text-subhead font-semibold text-foreground">
          LLM Providers
        </h3>
        <p className="text-caption text-muted-foreground">
          注册多个 LLM 厂商（Anthropic / DeepSeek / Qwen / Kimi / GLM ...）
          并在它们之间切换。API Key 存储在项目本地的{" "}
          <code className="rounded bg-muted px-1 py-0.5 text-label">
            .claw/providers.json
          </code>
          ，不会上传到任何远程服务。
        </p>
      </header>

      {flashError && (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-body-sm text-destructive">
          {flashError}
          <button
            className="ml-2 underline"
            onClick={() => setFlashError(null)}
          >
            关闭
          </button>
        </div>
      )}

      {providersQuery.isLoading ? (
        <SectionLoading />
      ) : providers.length === 0 ? (
        <EmptyState onAddClick={() => setShowAddForm(true)} />
      ) : (
        <div className="space-y-2">
          {providers.map((p) => (
            <ProviderCard
              key={p.id}
              provider={p}
              isActive={p.id === activeId}
              testResult={testResults[p.id]}
              onActivate={() => {
                setFlashError(null);
                activateMutation.mutate(p.id);
              }}
              onDelete={() => {
                setFlashError(null);
                setPendingDelete(p);
              }}
              onEdit={() => {
                setFlashError(null);
                setEditingProvider(p);
                setShowAddForm(false);
              }}
              onTest={() => {
                setFlashError(null);
                testMutation.mutate(p.id);
              }}
              activating={
                activateMutation.isPending &&
                activateMutation.variables === p.id
              }
              deleting={
                deleteMutation.isPending &&
                deleteMutation.variables === p.id
              }
              testing={
                testMutation.isPending && testMutation.variables === p.id
              }
            />
          ))}
        </div>
      )}

      <Separator />

      <div className="flex items-center justify-between">
        <div className="text-body-sm font-semibold text-foreground">
          {editingProvider ? `编辑 Provider：${editingProvider.id}` : "添加新 Provider"}
        </div>
        {!showAddForm && !editingProvider && (
          <Button
            size="sm"
            variant="outline"
            onClick={() => setShowAddForm(true)}
          >
            <Plus className="mr-1.5 size-3.5" />
            从模板添加
          </Button>
        )}
      </div>

      {(showAddForm || editingProvider) && (
        <ProviderForm
          key={editingProvider?.id ?? "new"}
          mode={editingProvider ? "edit" : "add"}
          templates={templates}
          editingProvider={editingProvider}
          onCancel={() => {
            setShowAddForm(false);
            setEditingProvider(null);
            setFlashError(null);
          }}
          onSubmit={(request) => {
            setFlashError(null);
            upsertMutation.mutate(request);
          }}
          submitting={upsertMutation.isPending}
        />
      )}

      <div className="text-caption text-muted-foreground">
        <RefreshCw className="mr-1 inline size-3" />
        切换激活的 provider 会在下一次 agentic turn 时立即生效，无需重启 server。
      </div>

      <ConfirmDialog
        open={!!pendingDelete}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
        title="删除 Provider"
        description={
          pendingDelete
            ? `确定要删除 provider "${pendingDelete.id}" 吗？对应的 API key 会从 .claw/providers.json 中一并清除，此操作不可撤销。`
            : ""
        }
        confirmLabel="删除"
        cancelLabel="取消"
        variant="destructive"
        onConfirm={() => {
          if (pendingDelete) {
            deleteMutation.mutate(pendingDelete.id);
            setPendingDelete(null);
          }
        }}
      />
    </div>
  );
}

// ── Sub-components ───────────────────────────────────────────────

function SectionLoading() {
  return (
    <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/20 px-4 py-3 text-body-sm text-muted-foreground">
      <Loader2 className="size-4 animate-spin" />
      <span>加载 providers 配置...</span>
    </div>
  );
}

function EmptyState({ onAddClick }: { onAddClick: () => void }) {
  return (
    <div className="rounded-lg border border-dashed border-border bg-muted/10 px-5 py-8 text-center">
      <p className="mb-3 text-body-sm text-muted-foreground">
        还没有配置任何 provider。从下方模板添加你的第一个 LLM 厂商。
      </p>
      <Button size="sm" onClick={onAddClick}>
        <Plus className="mr-1.5 size-3.5" />
        添加 Provider
      </Button>
    </div>
  );
}

function ProviderCard({
  provider,
  isActive,
  testResult,
  onActivate,
  onDelete,
  onEdit,
  onTest,
  activating,
  deleting,
  testing,
}: {
  provider: DesktopProviderSummary;
  isActive: boolean;
  testResult: ProviderTestResult | undefined;
  onActivate: () => void;
  onDelete: () => void;
  onEdit: () => void;
  onTest: () => void;
  activating: boolean;
  deleting: boolean;
  testing: boolean;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border bg-background p-3 transition-colors",
        isActive
          ? "border-primary/50 bg-primary/5 shadow-sm"
          : "border-border hover:border-border/80"
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-body font-semibold text-foreground">
              {provider.display_name || provider.id}
            </span>
            <span className="rounded bg-muted px-1.5 py-0.5 text-caption uppercase text-muted-foreground">
              {provider.kind === "anthropic" ? "Anthropic" : "OpenAI compat"}
            </span>
            {isActive && (
              <span className="flex items-center gap-1 rounded bg-primary/10 px-1.5 py-0.5 text-caption font-semibold text-primary">
                <CheckCircle2 className="size-3" />
                ACTIVE
              </span>
            )}
            <TestResultBadge result={testResult} testing={testing} />
          </div>
          <div className="text-caption text-muted-foreground">
            <code className="rounded bg-muted px-1 py-0.5">{provider.id}</code>
            {" · "}
            <span>{provider.model}</span>
            {" · "}
            <span>max_tokens={provider.max_tokens}</span>
          </div>
          <div className="text-caption text-muted-foreground">
            <span>{provider.base_url}</span>
          </div>
          <div className="text-caption text-muted-foreground">
            <span className="font-mono">{provider.api_key_display}</span>
          </div>
        </div>
        <div className="flex flex-col gap-1.5">
          {!isActive && (
            <Button
              size="sm"
              variant="outline"
              onClick={onActivate}
              disabled={activating}
            >
              {activating ? (
                <Loader2 className="mr-1 size-3 animate-spin" />
              ) : (
                <Zap className="mr-1 size-3" />
              )}
              激活
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={onTest}
            disabled={testing}
            title="发送一个极小的 ping 请求（约 20 tokens），验证 API key / base_url / 模型名是否有效。"
          >
            {testing ? (
              <Loader2 className="mr-1 size-3 animate-spin" />
            ) : (
              <Zap className="mr-1 size-3" />
            )}
            测试
          </Button>
          <Button size="sm" variant="ghost" onClick={onEdit}>
            <Pencil className="mr-1 size-3" />
            编辑
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={onDelete}
            disabled={deleting}
            className="text-destructive hover:bg-destructive/10 hover:text-destructive"
          >
            {deleting ? (
              <Loader2 className="mr-1 size-3 animate-spin" />
            ) : (
              <Trash2 className="mr-1 size-3" />
            )}
            删除
          </Button>
        </div>
      </div>
    </div>
  );
}

function TestResultBadge({
  result,
  testing,
}: {
  result: ProviderTestResult | undefined;
  testing: boolean;
}) {
  if (testing) {
    return (
      <span className="flex items-center gap-1 rounded bg-muted px-1.5 py-0.5 text-caption text-muted-foreground">
        <Loader2 className="size-3 animate-spin" />
        测试中…
      </span>
    );
  }
  if (!result) return null;
  if (result.ok) {
    return (
      <span
        className="flex items-center gap-1 rounded px-1.5 py-0.5 text-caption font-semibold"
        style={{
          backgroundColor:
            "color-mix(in srgb, var(--color-success) 12%, transparent)",
          color: "var(--color-success)",
        }}
        title={
          result.model_echo
            ? `模型回显：${result.model_echo}`
            : "连接测试成功"
        }
      >
        <CheckCircle className="size-3" />
        {result.latency_ms}ms
      </span>
    );
  }
  return (
    <span
      className="flex items-center gap-1 rounded px-1.5 py-0.5 text-caption font-semibold"
      style={{
        backgroundColor:
          "color-mix(in srgb, var(--color-error) 12%, transparent)",
        color: "var(--color-error)",
      }}
      title={result.error ?? "连接测试失败"}
    >
      <XCircle className="size-3" />
      失败
    </span>
  );
}

function ProviderForm({
  mode,
  templates,
  editingProvider,
  onCancel,
  onSubmit,
  submitting,
}: {
  mode: "add" | "edit";
  templates: DesktopProviderTemplate[];
  editingProvider: DesktopProviderSummary | null;
  onCancel: () => void;
  onSubmit: (request: Parameters<typeof upsertProvider>[0]) => void;
  submitting: boolean;
}) {
  const isEdit = mode === "edit" && editingProvider !== null;

  // In edit mode, figure out which built-in template (if any) the existing
  // provider came from, so the hint link still shows correctly.
  const defaultTemplateId = useMemo(() => {
    if (isEdit) {
      const matchingTemplate = templates.find(
        (t) => t.kind === editingProvider!.kind
      );
      return matchingTemplate?.id ?? templates[0]?.id ?? "";
    }
    return templates[0]?.id ?? "";
  }, [isEdit, editingProvider, templates]);

  const [templateId, setTemplateId] = useState<string>(defaultTemplateId);
  const [customId, setCustomId] = useState(
    isEdit ? editingProvider!.id : ""
  );
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [customModel, setCustomModel] = useState(
    isEdit ? editingProvider!.model : ""
  );
  const [customBaseUrl, setCustomBaseUrl] = useState(
    isEdit ? editingProvider!.base_url : ""
  );

  const selectedTemplate = useMemo(
    () => templates.find((t) => t.id === templateId),
    [templates, templateId]
  );

  // Prefill the form whenever the selected template changes (including the
  // first render when templates finish loading). Only fills empty fields so
  // that the user's in-progress edits aren't clobbered. In edit mode we
  // never auto-fill — the fields come from the existing provider.
  useEffect(() => {
    if (isEdit) return;
    if (!selectedTemplate) return;
    setCustomId((prev) => (prev === "" ? selectedTemplate.id : prev));
    setCustomModel((prev) =>
      prev === "" ? selectedTemplate.default_model : prev
    );
    setCustomBaseUrl((prev) =>
      prev === "" ? selectedTemplate.base_url : prev
    );
  }, [selectedTemplate, isEdit]);

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!selectedTemplate) return;
    if (!customId.trim()) return;
    if (!customModel.trim()) return;
    // In add mode the api key is required. In edit mode an empty api_key
    // means "keep the existing one" — the backend will merge with the
    // current value.
    if (!isEdit && !apiKey.trim()) return;

    const kind: ProviderKind = isEdit ? editingProvider!.kind : selectedTemplate.kind;
    onSubmit({
      id: customId.trim(),
      entry: {
        kind,
        display_name:
          isEdit && editingProvider!.display_name
            ? editingProvider!.display_name
            : selectedTemplate.display_name,
        base_url:
          kind === "openai_compat"
            ? customBaseUrl.trim() || selectedTemplate.base_url
            : undefined,
        // Empty string signals the backend to keep the previous api_key.
        api_key: apiKey,
        model: customModel.trim(),
        max_tokens: isEdit
          ? editingProvider!.max_tokens
          : selectedTemplate.max_tokens,
      },
    });
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="space-y-3 rounded-lg border border-border bg-muted/10 p-4"
    >
      {!isEdit && (
        <div className="space-y-1">
          <label className="text-caption font-semibold text-muted-foreground">
            选择厂商模板
          </label>
          <Select
            value={templateId}
            onValueChange={(value) => {
              setTemplateId(value);
              setCustomId("");
              setCustomModel("");
              setCustomBaseUrl("");
            }}
          >
            <SelectTrigger>
              <SelectValue placeholder="选择厂商模板" />
            </SelectTrigger>
            <SelectContent>
              {templates.map((t) => (
                <SelectItem key={t.id} value={t.id}>
                  {t.display_name} — {t.description}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {selectedTemplate && (
        <div className="flex items-center gap-1 text-caption">
          <ExternalLink className="size-3" />
          <a
            href={selectedTemplate.api_key_url}
            target="_blank"
            rel="noreferrer"
            className="text-primary hover:underline"
          >
            前往 {selectedTemplate.display_name} 创建 API Key
          </a>
        </div>
      )}

      <div className="space-y-1">
        <label className="text-caption font-semibold text-muted-foreground">
          Provider ID（本地别名）
        </label>
        <Input
          value={customId}
          onChange={(e) => setCustomId(e.target.value)}
          placeholder="deepseek-prod"
          readOnly={isEdit}
          disabled={isEdit}
        />
        {isEdit && (
          <p className="text-caption text-muted-foreground">
            Provider ID 是主键，保存后不可修改。
          </p>
        )}
      </div>

      <div className="space-y-1">
        <label className="text-caption font-semibold text-muted-foreground">
          API Key
        </label>
        <div className="relative">
          <Input
            type={showKey ? "text" : "password"}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={isEdit ? "留空则保留当前 key" : "sk-..."}
            autoComplete="off"
            className="pr-9"
          />
          <button
            type="button"
            className="absolute right-2 top-1/2 flex size-6 -translate-y-1/2 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={() => setShowKey((prev) => !prev)}
            aria-label={showKey ? "隐藏 API Key" : "显示 API Key"}
            tabIndex={-1}
          >
            {showKey ? (
              <EyeOff className="size-3.5" />
            ) : (
              <Eye className="size-3.5" />
            )}
          </button>
        </div>
        <p className="text-caption text-muted-foreground">
          {isEdit
            ? "留空不修改；填写新值则覆盖。所有 key 保存在本机 .claw/providers.json。"
            : "将保存到本机 .claw/providers.json，不会上传到任何远程服务。"}
        </p>
      </div>

      <div className="space-y-1">
        <label className="text-caption font-semibold text-muted-foreground">
          模型
        </label>
        <Input
          value={customModel}
          onChange={(e) => setCustomModel(e.target.value)}
          placeholder={selectedTemplate?.default_model ?? "model-id"}
        />
      </div>

      {(isEdit
        ? editingProvider!.kind === "openai_compat"
        : selectedTemplate?.kind === "openai_compat") && (
        <div className="space-y-1">
          <label className="text-caption font-semibold text-muted-foreground">
            Base URL
          </label>
          <Input
            value={customBaseUrl}
            onChange={(e) => setCustomBaseUrl(e.target.value)}
            placeholder={selectedTemplate?.base_url}
          />
        </div>
      )}

      <div className="flex items-center justify-end gap-2 pt-1">
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onClick={onCancel}
          disabled={submitting}
        >
          取消
        </Button>
        <Button type="submit" size="sm" disabled={submitting}>
          {submitting && <Loader2 className="mr-1 size-3 animate-spin" />}
          {isEdit ? "保存修改" : "保存并激活"}
        </Button>
      </div>
    </form>
  );
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

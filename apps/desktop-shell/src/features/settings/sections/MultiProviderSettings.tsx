import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckCircle2,
  ExternalLink,
  Loader2,
  Plus,
  RefreshCw,
  Trash2,
  Zap,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import {
  activateProvider,
  deleteProvider,
  listProviders,
  listProviderTemplates,
  upsertProvider,
  type DesktopProviderSummary,
  type DesktopProviderTemplate,
  type ProviderKind,
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
  const [flashError, setFlashError] = useState<string | null>(null);

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
    },
    onError: (err) => setFlashError(errorMessage(err)),
  });

  const providers = providersQuery.data?.providers ?? [];
  const activeId = providersQuery.data?.active ?? "";
  const templates = templatesQuery.data?.templates ?? [];

  return (
    <div className="space-y-5">
      <header className="space-y-1">
        <h3 className="text-head-sm font-semibold">LLM Providers</h3>
        <p className="text-caption text-muted-foreground">
          注册多个 LLM 厂商（Anthropic / DeepSeek / Qwen / Kimi / GLM ...）
          并在它们之间切换。API Key 存储在项目本地的{" "}
          <code className="rounded bg-muted px-1 py-0.5 text-[11px]">
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
              onActivate={() => {
                setFlashError(null);
                activateMutation.mutate(p.id);
              }}
              onDelete={() => {
                setFlashError(null);
                if (
                  window.confirm(
                    `确定要删除 provider "${p.id}" 吗？API key 会一并清除。`
                  )
                ) {
                  deleteMutation.mutate(p.id);
                }
              }}
              activating={
                activateMutation.isPending &&
                activateMutation.variables === p.id
              }
              deleting={
                deleteMutation.isPending &&
                deleteMutation.variables === p.id
              }
            />
          ))}
        </div>
      )}

      <Separator />

      <div className="flex items-center justify-between">
        <div className="text-body-sm font-medium">添加新 Provider</div>
        {!showAddForm && (
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

      {showAddForm && (
        <AddProviderForm
          templates={templates}
          onCancel={() => {
            setShowAddForm(false);
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
  onActivate,
  onDelete,
  activating,
  deleting,
}: {
  provider: DesktopProviderSummary;
  isActive: boolean;
  onActivate: () => void;
  onDelete: () => void;
  activating: boolean;
  deleting: boolean;
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
          <div className="flex items-center gap-2">
            <span className="text-body font-medium">
              {provider.display_name || provider.id}
            </span>
            <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase text-muted-foreground">
              {provider.kind === "anthropic" ? "Anthropic" : "OpenAI compat"}
            </span>
            {isActive && (
              <span className="flex items-center gap-1 rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">
                <CheckCircle2 className="size-3" />
                ACTIVE
              </span>
            )}
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

function AddProviderForm({
  templates,
  onCancel,
  onSubmit,
  submitting,
}: {
  templates: DesktopProviderTemplate[];
  onCancel: () => void;
  onSubmit: (request: Parameters<typeof upsertProvider>[0]) => void;
  submitting: boolean;
}) {
  const [templateId, setTemplateId] = useState<string>(templates[0]?.id ?? "");
  const [customId, setCustomId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [customModel, setCustomModel] = useState("");
  const [customBaseUrl, setCustomBaseUrl] = useState("");

  // When template changes, prefill the form from its defaults
  const selectedTemplate = useMemo(
    () => templates.find((t) => t.id === templateId),
    [templates, templateId]
  );

  // Prefill the form whenever the selected template changes (including the
  // first render when templates finish loading). We always overwrite the
  // custom fields because the <select> onChange handler above already clears
  // them — so this effect is the single source of "what the template
  // defaults look like". Depending on `selectedTemplate` (not just
  // `templateId`) avoids a stale-closure bug where the prefill could miss
  // the initial `templates` load race.
  useEffect(() => {
    if (!selectedTemplate) return;
    setCustomId((prev) => (prev === "" ? selectedTemplate.id : prev));
    setCustomModel((prev) =>
      prev === "" ? selectedTemplate.default_model : prev
    );
    setCustomBaseUrl((prev) =>
      prev === "" ? selectedTemplate.base_url : prev
    );
  }, [selectedTemplate]);

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!selectedTemplate) return;
    if (!customId.trim()) return;
    if (!apiKey.trim()) return;
    if (!customModel.trim()) return;

    const kind: ProviderKind = selectedTemplate.kind;
    onSubmit({
      id: customId.trim(),
      entry: {
        kind,
        display_name: selectedTemplate.display_name,
        base_url:
          kind === "openai_compat"
            ? customBaseUrl.trim() || selectedTemplate.base_url
            : undefined,
        api_key: apiKey,
        model: customModel.trim(),
        max_tokens: selectedTemplate.max_tokens,
      },
    });
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="space-y-3 rounded-lg border border-border bg-muted/10 p-4"
    >
      <div className="space-y-1">
        <label className="text-caption font-medium text-muted-foreground">
          选择厂商模板
        </label>
        <select
          className="w-full rounded-md border border-input bg-background px-3 py-1.5 text-body-sm outline-none focus:border-ring"
          value={templateId}
          onChange={(e) => {
            setTemplateId(e.target.value);
            setCustomId("");
            setCustomModel("");
            setCustomBaseUrl("");
          }}
        >
          {templates.map((t) => (
            <option key={t.id} value={t.id}>
              {t.display_name} — {t.description}
            </option>
          ))}
        </select>
      </div>

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
        <label className="text-caption font-medium text-muted-foreground">
          Provider ID（本地别名）
        </label>
        <Input
          value={customId}
          onChange={(e) => setCustomId(e.target.value)}
          placeholder="deepseek-prod"
        />
      </div>

      <div className="space-y-1">
        <label className="text-caption font-medium text-muted-foreground">
          API Key
        </label>
        <Input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-..."
          autoComplete="off"
        />
        <p className="text-caption text-muted-foreground">
          将保存到 <code>.claw/providers.json</code>，你的本机。
        </p>
      </div>

      <div className="space-y-1">
        <label className="text-caption font-medium text-muted-foreground">
          模型
        </label>
        <Input
          value={customModel}
          onChange={(e) => setCustomModel(e.target.value)}
          placeholder={selectedTemplate?.default_model ?? "model-id"}
        />
      </div>

      {selectedTemplate?.kind === "openai_compat" && (
        <div className="space-y-1">
          <label className="text-caption font-medium text-muted-foreground">
            Base URL
          </label>
          <Input
            value={customBaseUrl}
            onChange={(e) => setCustomBaseUrl(e.target.value)}
            placeholder={selectedTemplate.base_url}
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
          保存并激活
        </Button>
      </div>
    </form>
  );
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

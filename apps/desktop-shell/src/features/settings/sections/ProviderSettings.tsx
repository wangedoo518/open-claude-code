import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  CheckCircle2,
  Cloud,
  Download,
  ExternalLink,
  Loader2,
  LogIn,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  UserRound,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import {
  activateCodexAuthProfile,
  beginCodexLogin,
  getCodexAuthOverview,
  getCodexRuntime,
  getManagedProviders,
  getProviderPresets,
  importCodexAuthProfile,
  openDashboardUrl,
  pollCodexLogin,
  refreshCodexAuthProfile,
  removeCodexAuthProfile,
  syncManagedProvider,
  upsertManagedProvider,
  type DesktopCodexAuthOverview,
  type DesktopCodexAuthSource,
  type DesktopCodexLoginSessionSnapshot,
  type DesktopCodexProfileSummary,
  type DesktopCustomizeState,
  type DesktopManagedProvider,
  type DesktopProviderModel,
  type DesktopProviderPreset,
} from "@/lib/tauri";

const OPENAI_PROVIDER_PRESET_ID = "codex-openai";

interface ProviderSettingsProps {
  customize: DesktopCustomizeState | null;
  error?: string;
}

interface Notice {
  tone: "info" | "success" | "error";
  message: string;
}

type BusyAction =
  | "initializing"
  | "toggle-enabled"
  | "sync"
  | "refresh"
  | "import-auth"
  | "login"
  | "activate-profile"
  | "refresh-profile"
  | "remove-profile"
  | "set-default-model"
  | null;

export function ProviderSettings({ customize, error }: ProviderSettingsProps) {
  const [notice, setNotice] = useState<Notice | null>(null);
  const [busyAction, setBusyAction] = useState<BusyAction>(null);
  const [initializingProvider, setInitializingProvider] = useState(false);
  const [codexLoginSession, setCodexLoginSession] =
    useState<DesktopCodexLoginSessionSnapshot | null>(null);
  const codexLoginStatusRef = useRef<string | null>(null);

  const presetsQuery = useQuery({
    queryKey: ["provider-presets"],
    queryFn: async () => (await getProviderPresets()).presets,
    refetchOnWindowFocus: false,
  });

  const providersQuery = useQuery({
    queryKey: ["managed-providers"],
    queryFn: async () => (await getManagedProviders()).providers,
    refetchOnWindowFocus: false,
  });

  const codexRuntimeQuery = useQuery({
    queryKey: ["codex-runtime"],
    queryFn: async () => (await getCodexRuntime()).runtime,
    refetchOnWindowFocus: false,
  });

  const codexAuthOverviewQuery = useQuery({
    queryKey: ["codex-auth-overview"],
    queryFn: async () => (await getCodexAuthOverview()).overview,
    refetchOnWindowFocus: false,
  });

  const openAiPreset = useMemo(
    () =>
      (presetsQuery.data ?? []).find((preset) => preset.id === OPENAI_PROVIDER_PRESET_ID) ?? null,
    [presetsQuery.data]
  );

  const managedOpenAiProvider = useMemo(
    () =>
      (providersQuery.data ?? []).find((provider) => isOpenAiProvider(provider)) ?? null,
    [providersQuery.data]
  );

  const codexRuntime = codexRuntimeQuery.data ?? null;
  const codexAuthOverview = codexAuthOverviewQuery.data ?? null;
  const displayProvider = managedOpenAiProvider ?? openAiPreset;
  const displayProviderEnabled = managedOpenAiProvider?.enabled ?? true;

  const activeProfile = useMemo(
    () => resolveCurrentCodexProfile(codexAuthOverview),
    [codexAuthOverview]
  );

  const openAiLiveProvider = useMemo(
    () =>
      managedOpenAiProvider
        ? codexRuntime?.live_providers.find((provider) => provider.id === managedOpenAiProvider.id) ??
          null
        : null,
    [codexRuntime, managedOpenAiProvider]
  );

  const syncState = useMemo(() => {
    if (!managedOpenAiProvider) {
      return {
        label: "未写入",
        applied: false,
        live: false,
      };
    }
    const live = Boolean(openAiLiveProvider);
    const applied = codexRuntime?.active_provider_key === managedOpenAiProvider.id;
    if (applied) {
      return { label: "已同步生效", applied: true, live: true };
    }
    if (live) {
      return { label: "已写入 Codex", applied: false, live: true };
    }
    return { label: "未写入", applied: false, live: false };
  }, [codexRuntime, managedOpenAiProvider, openAiLiveProvider]);

  const displayModels = useMemo(
    () => managedOpenAiProvider?.models ?? openAiPreset?.models ?? [],
    [managedOpenAiProvider, openAiPreset]
  );

  const groupedModels = useMemo(() => groupOpenAiModels(displayModels), [displayModels]);

  const currentDefaultModelId =
    syncState.applied && codexRuntime?.model
      ? codexRuntime.model
      : displayModels[0]?.model_id ?? null;

  const diagnostics = useMemo(() => {
    const messages: Array<{ tone: "success" | "warning"; message: string }> = [];
    if (!activeProfile && !codexAuthOverview?.has_chatgpt_tokens) {
      messages.push({
        tone: "warning",
        message: "当前尚未连接 OpenAI 账号，请先使用 ChatGPT 登录。",
      });
    }
    if (managedOpenAiProvider && !managedOpenAiProvider.enabled) {
      messages.push({
        tone: "warning",
        message: "OpenAI 服务当前已停用，启用后才能同步到 Codex。",
      });
    }
    if (managedOpenAiProvider && !syncState.live) {
      messages.push({
        tone: "warning",
        message: "当前 OpenAI 配置尚未写入 Codex。",
      });
    }
    if (managedOpenAiProvider && syncState.live && !syncState.applied) {
      messages.push({
        tone: "warning",
        message: "Codex 当前正在使用其他 provider，如需切换请重新同步 OpenAI。",
      });
    }
    if (
      activeProfile &&
      managedOpenAiProvider?.enabled &&
      syncState.applied &&
      currentDefaultModelId
    ) {
      messages.push({
        tone: "success",
        message: `当前已使用 ${activeProfile.display_label} 连接 OpenAI，并将默认模型设为 ${currentDefaultModelId}。`,
      });
    }
    return messages;
  }, [
    activeProfile,
    codexAuthOverview?.has_chatgpt_tokens,
    currentDefaultModelId,
    managedOpenAiProvider,
    syncState.applied,
    syncState.live,
  ]);

  const warningBanner = useMemo(() => {
    if (!activeProfile && !codexAuthOverview?.has_chatgpt_tokens) {
      return "当前尚未连接 OpenAI 账号，请先使用 ChatGPT 登录。";
    }
    if (managedOpenAiProvider && !syncState.live) {
      return "当前 OpenAI 账号已连接，但尚未写入 Codex 配置。";
    }
    if (managedOpenAiProvider && syncState.live && !syncState.applied) {
      return "当前 OpenAI 配置已写入 Codex，但尚未作为当前生效 provider。";
    }
    return null;
  }, [activeProfile, codexAuthOverview?.has_chatgpt_tokens, managedOpenAiProvider, syncState]);

  async function refreshPageData() {
    await Promise.all([
      presetsQuery.refetch(),
      providersQuery.refetch(),
      codexRuntimeQuery.refetch(),
      codexAuthOverviewQuery.refetch(),
    ]);
  }

  const ensureOpenAiProvider = useCallback(async (): Promise<DesktopManagedProvider> => {
    if (managedOpenAiProvider) {
      return managedOpenAiProvider;
    }
    if (!openAiPreset) {
      throw new Error("OpenAI 官方服务配置尚未加载完成。");
    }
    const response = await upsertManagedProvider(toManagedProviderPayload(openAiPreset));
    return response.provider;
  }, [managedOpenAiProvider, openAiPreset]);

  useEffect(() => {
    if (
      initializingProvider ||
      providersQuery.isLoading ||
      providersQuery.isRefetching ||
      providersQuery.error ||
      !openAiPreset ||
      managedOpenAiProvider
    ) {
      return;
    }
    let cancelled = false;
    setInitializingProvider(true);
    void ensureOpenAiProvider()
      .then(async () => {
        await refreshPageData();
      })
      .catch((providerError) => {
        if (!cancelled) {
          const message =
            providerError instanceof Error
              ? providerError.message
              : "初始化 OpenAI 官方服务失败。";
          setNotice({ tone: "error", message });
        }
      })
      .finally(() => {
        if (!cancelled) {
          setInitializingProvider(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    ensureOpenAiProvider,
    initializingProvider,
    managedOpenAiProvider,
    openAiPreset,
    providersQuery.error,
    providersQuery.isLoading,
    providersQuery.isRefetching,
  ]);

  useEffect(() => {
    if (!codexLoginSession || codexLoginSession.status !== "pending") {
      return;
    }
    const timeoutId = window.setTimeout(async () => {
      try {
        const response = await pollCodexLogin(codexLoginSession.session_id);
        setCodexLoginSession(response.session);
      } catch (pollError) {
        const message =
          pollError instanceof Error ? pollError.message : "轮询 Codex 登录状态失败。";
        setNotice({ tone: "error", message });
      }
    }, 1500);
    return () => window.clearTimeout(timeoutId);
  }, [codexLoginSession]);

  useEffect(() => {
    if (!codexLoginSession) return;
    if (codexLoginStatusRef.current === codexLoginSession.status) {
      return;
    }
    codexLoginStatusRef.current = codexLoginSession.status;
    if (codexLoginSession.status === "completed") {
      void refreshPageData();
      setNotice({
        tone: "success",
        message: codexLoginSession.profile
          ? `已使用 ${codexLoginSession.profile.display_label} 完成 OpenAI 登录。`
          : "已完成 OpenAI 登录。",
      });
    }
    if (codexLoginSession.status === "failed" && codexLoginSession.error) {
      setNotice({ tone: "error", message: codexLoginSession.error });
    }
  }, [codexLoginSession]);

  async function handleRefresh() {
    setBusyAction("refresh");
    try {
      await refreshPageData();
      setNotice({ tone: "success", message: "已刷新 OpenAI 与 Codex 当前状态。" });
    } catch (refreshError) {
      const message =
        refreshError instanceof Error ? refreshError.message : "刷新状态失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleToggleEnabled() {
    setBusyAction("toggle-enabled");
    try {
      const provider = await ensureOpenAiProvider();
      await upsertManagedProvider(
        toManagedProviderPayload(provider, { enabled: !provider.enabled })
      );
      await refreshPageData();
      setNotice({
        tone: "success",
        message: provider.enabled ? "已停用 OpenAI 服务。" : "已启用 OpenAI 服务。",
      });
    } catch (toggleError) {
      const message =
        toggleError instanceof Error ? toggleError.message : "更新服务状态失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleSync() {
    setBusyAction("sync");
    try {
      const provider = await ensureOpenAiProvider();
      if (!provider.enabled) {
        throw new Error("请先启用 OpenAI 服务，再同步到 Codex。");
      }
      const response = await syncManagedProvider(provider.id, { set_primary: false });
      await refreshPageData();
      setNotice({
        tone: "success",
        message: `已将 OpenAI 官方配置同步到 Codex（${response.result.config_path}）。`,
      });
    } catch (syncError) {
      const message =
        syncError instanceof Error ? syncError.message : "同步到 Codex 失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleImportCurrentAuth() {
    setBusyAction("import-auth");
    try {
      await importCodexAuthProfile();
      await refreshPageData();
      setNotice({
        tone: "success",
        message: "已导入当前本机的 Codex 登录态。",
      });
    } catch (authError) {
      const message =
        authError instanceof Error ? authError.message : "导入当前登录态失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleBeginLogin() {
    setBusyAction("login");
    try {
      const response = await beginCodexLogin();
      setCodexLoginSession(response.session);
      await openDashboardUrl(response.session.authorize_url);
      setNotice({
        tone: "info",
        message: "已打开浏览器授权页，请完成 ChatGPT 登录。",
      });
    } catch (loginError) {
      const message =
        loginError instanceof Error ? loginError.message : "打开 ChatGPT 登录失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleActivateProfile(profileId: string) {
    setBusyAction("activate-profile");
    try {
      await activateCodexAuthProfile(profileId);
      await refreshPageData();
      setNotice({
        tone: "success",
        message: "已将该账号设为当前 OpenAI 登录账号。",
      });
    } catch (activateError) {
      const message =
        activateError instanceof Error ? activateError.message : "切换账号失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRefreshProfile(profileId: string) {
    setBusyAction("refresh-profile");
    try {
      await refreshCodexAuthProfile(profileId);
      await refreshPageData();
      setNotice({ tone: "success", message: "已刷新该账号的登录状态。" });
    } catch (refreshError) {
      const message =
        refreshError instanceof Error ? refreshError.message : "刷新登录状态失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRemoveProfile(profileId: string) {
    if (!window.confirm("确定要移除这个 OpenAI 登录账号吗？")) {
      return;
    }
    setBusyAction("remove-profile");
    try {
      await removeCodexAuthProfile(profileId);
      await refreshPageData();
      setNotice({ tone: "success", message: "已移除该账号。" });
    } catch (removeError) {
      const message =
        removeError instanceof Error ? removeError.message : "移除账号失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleSetDefaultModel(modelId: string) {
    setBusyAction("set-default-model");
    try {
      const provider = await ensureOpenAiProvider();
      const reorderedModels = prioritizeModel(provider.models, modelId);
      await upsertManagedProvider(
        toManagedProviderPayload(provider, {
          models: reorderedModels,
        })
      );
      await refreshPageData();
      setNotice({
        tone: "success",
        message: `已将 ${modelId} 设为默认模型，请继续同步到 Codex。`,
      });
    } catch (modelError) {
      const message =
        modelError instanceof Error ? modelError.message : "设置默认模型失败。";
      setNotice({ tone: "error", message });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleOpenWebsite() {
    const website = displayProvider?.website_url ?? openAiPreset?.website_url ?? null;
    if (!website) return;
    try {
      await openDashboardUrl(website);
    } catch (openError) {
      const message =
        openError instanceof Error ? openError.message : "打开官网失败。";
      setNotice({ tone: "error", message });
    }
  }

  const pageError =
    errorMessage(providersQuery.error) ??
    errorMessage(presetsQuery.error) ??
    errorMessage(codexRuntimeQuery.error) ??
    errorMessage(codexAuthOverviewQuery.error) ??
    error;

  return (
    <div className="space-y-4">
      {(notice || pageError) && (
        <StatusBanner
          tone={notice?.tone ?? "error"}
          message={notice?.message ?? pageError ?? ""}
        />
      )}

      <div className="grid gap-4 xl:grid-cols-[300px_minmax(0,1fr)]">
        <section className="overflow-hidden rounded-2xl border border-border bg-background">
          <div className="border-b border-border px-5 py-4">
            <div className="text-base font-semibold text-foreground">模型服务</div>
            <p className="mt-1 text-sm leading-6 text-muted-foreground">
              当前版本仅支持 OpenAI 官方渠道，并通过 Codex 登录态完成授权。
            </p>
          </div>

          <ScrollArea className="h-[min(72vh,860px)]">
            <div className="space-y-4 p-4">
              <button
                type="button"
                className={cn(
                  "w-full rounded-2xl border px-4 py-4 text-left transition",
                  "border-primary bg-primary/5"
                )}
              >
                <div className="flex items-start gap-3">
                  <div className="flex size-11 shrink-0 items-center justify-center rounded-full bg-foreground text-sm font-semibold text-background">
                    AI
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <div className="text-base font-semibold text-foreground">OpenAI</div>
                      <Badge variant="outline">官方</Badge>
                      <Badge variant={displayProviderEnabled ? "default" : "secondary"}>
                        {displayProviderEnabled ? "已启用" : "已停用"}
                      </Badge>
                    </div>
                    <div className="mt-2 text-sm text-muted-foreground">
                      {activeProfile ? `已连接 ${activeProfile.display_label}` : "尚未登录"}
                    </div>
                    <div className="mt-3 flex flex-wrap gap-1.5">
                      <Badge variant="outline">{syncState.label}</Badge>
                      {currentDefaultModelId ? (
                        <Badge variant="outline">{currentDefaultModelId}</Badge>
                      ) : null}
                    </div>
                  </div>
                </div>
              </button>

              <div className="rounded-2xl border border-dashed border-border bg-muted/20 p-4 text-sm leading-6 text-muted-foreground">
                不支持第三方兼容渠道、不支持 API 密钥录入、不支持自定义 Provider。
              </div>
            </div>
          </ScrollArea>
        </section>

        <section className="rounded-2xl border border-border bg-background p-5">
          {displayProvider ? (
            <div className="space-y-5">
              <div className="flex flex-col gap-4 border-b border-border pb-5 md:flex-row md:items-start md:justify-between">
                <div>
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-2xl font-semibold text-foreground">OpenAI</h3>
                    <Badge variant="outline">官方</Badge>
                    <Badge variant="outline">Codex 登录</Badge>
                    {displayProviderEnabled ? (
                      <Badge variant="default">已启用</Badge>
                    ) : (
                      <Badge variant="secondary">已停用</Badge>
                    )}
                  </div>
                  <p className="mt-2 max-w-3xl text-sm leading-6 text-muted-foreground">
                    OpenAI 官方 Responses 服务，Warwolf 仅通过 Codex 登录态将配置同步到
                    `~/.codex`。
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button variant="outline" onClick={() => void handleOpenWebsite()}>
                    <ExternalLink className="size-4" />
                    官网
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => void handleRefresh()}
                    disabled={busyAction !== null}
                  >
                    {busyAction === "refresh" ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <RefreshCw className="size-4" />
                    )}
                    刷新状态
                  </Button>
                  <Button
                    onClick={() => void handleSync()}
                    disabled={busyAction !== null || !displayProviderEnabled}
                  >
                    {busyAction === "sync" ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <Cloud className="size-4" />
                    )}
                    同步到 Codex
                  </Button>
                  <button
                    type="button"
                    className={cn(
                      "inline-flex h-9 items-center rounded-full border px-4 text-sm font-medium transition-colors",
                      displayProviderEnabled
                        ? "border-primary bg-primary/10 text-foreground"
                        : "border-border text-muted-foreground"
                    )}
                    onClick={() => void handleToggleEnabled()}
                    disabled={busyAction !== null}
                  >
                    {busyAction === "toggle-enabled" ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : displayProviderEnabled ? (
                      "已启用"
                    ) : (
                      "已停用"
                    )}
                  </button>
                </div>
              </div>

              {warningBanner ? <NoticeBar message={warningBanner} /> : null}

              <SectionCard
                title="账号连接"
                description="仅支持通过 Codex 登录态连接 OpenAI，不支持手动填写 API 密钥。"
              >
                <div className="grid gap-4 md:grid-cols-2">
                  <MetricCard
                    label="当前状态"
                    value={activeProfile ? "已登录" : "未登录"}
                    hint={
                      activeProfile
                        ? `当前账号：${activeProfile.display_label}`
                        : "请先完成 ChatGPT 登录"
                    }
                    tone={activeProfile ? "success" : "warning"}
                  />
                  <MetricCard
                    label="同步状态"
                    value={syncState.label}
                    hint={
                      syncState.applied
                        ? "OpenAI 已作为当前 Codex provider 生效"
                        : "同步后将写入 ~/.codex/config.toml"
                    }
                    tone={syncState.applied ? "success" : "warning"}
                  />
                </div>

                <div className="flex flex-wrap gap-2">
                  <Button onClick={() => void handleBeginLogin()} disabled={busyAction !== null}>
                    {busyAction === "login" ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <LogIn className="size-4" />
                    )}
                    使用 ChatGPT 登录
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => void handleImportCurrentAuth()}
                    disabled={busyAction !== null}
                  >
                    {busyAction === "import-auth" ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <Download className="size-4" />
                    )}
                    导入当前 Codex 登录态
                  </Button>
                  {codexLoginSession?.status === "pending" ? (
                    <Button
                      variant="outline"
                      onClick={() => void openDashboardUrl(codexLoginSession.authorize_url)}
                      disabled={busyAction !== null}
                    >
                      <ExternalLink className="size-4" />
                      重新打开授权页
                    </Button>
                  ) : null}
                </div>

                {codexLoginSession ? (
                  <div className="rounded-2xl border border-border bg-muted/10 px-4 py-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <div className="text-sm font-medium text-foreground">最近一次登录流程</div>
                      <Badge variant="outline">{formatLoginSessionStatus(codexLoginSession.status)}</Badge>
                    </div>
                    <div className="mt-2 text-xs text-muted-foreground">
                      回调地址：{codexLoginSession.redirect_uri}
                    </div>
                    {codexLoginSession.profile ? (
                      <div className="mt-2 text-sm text-foreground">
                        已导入账号：{codexLoginSession.profile.display_label}
                      </div>
                    ) : null}
                    {codexLoginSession.error ? (
                      <div className="mt-2 text-sm text-destructive">
                        {codexLoginSession.error}
                      </div>
                    ) : null}
                  </div>
                ) : null}

                {codexAuthOverview ? (
                  <div className="space-y-3">
                    {codexAuthOverview.profiles.map((profile) => (
                      <ProfileCard
                        key={profile.id}
                        profile={profile}
                        busyAction={busyAction}
                        onActivate={() => void handleActivateProfile(profile.id)}
                        onRefresh={() => void handleRefreshProfile(profile.id)}
                        onRemove={() => void handleRemoveProfile(profile.id)}
                      />
                    ))}
                  </div>
                ) : (
                  <LoadingBlock label="正在读取 OpenAI 登录状态..." />
                )}
              </SectionCard>

              <SectionCard
                title="服务配置"
                description="这一区域仅展示当前 OpenAI 官方服务的只读配置，不支持手动修改。"
              >
                <div className="grid gap-4 md:grid-cols-2">
                  <InfoField label="鉴权方式" value="Codex 登录" />
                  <InfoField label="协议" value="OpenAI Responses" />
                  <InfoField label="API 地址" value={displayProvider.base_url} />
                  <InfoField label="官方站点" value={displayProvider.website_url ?? "https://platform.openai.com"} />
                  <InfoField label="Codex 配置文件" value={codexRuntime?.config_path ?? "~/.codex/config.toml"} />
                  <InfoField label="授权文件" value={codexRuntime?.auth_path ?? "~/.codex/auth.json"} />
                </div>
              </SectionCard>

              <SectionCard
                title="模型"
                description="使用官方 OpenAI 模型目录，只允许设置默认模型，不允许手动新增、删除或编辑。"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="text-sm text-muted-foreground">
                    默认模型：{currentDefaultModelId ?? "尚未设置"}
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void handleRefresh()}
                    disabled={busyAction !== null}
                  >
                    <RefreshCw className="size-4" />
                    刷新模型状态
                  </Button>
                </div>

                <div className="space-y-4">
                  {groupedModels.map((group) => (
                    <div key={group.label} className="overflow-hidden rounded-2xl border border-border">
                      <div className="border-b border-border bg-muted/20 px-4 py-3">
                        <div className="text-sm font-semibold text-foreground">{group.label}</div>
                      </div>
                      <div className="divide-y divide-border">
                        {group.models.map((model) => {
                          const isDefault = currentDefaultModelId === model.model_id;
                          return (
                            <div
                              key={model.model_id}
                              className="flex flex-col gap-3 px-4 py-4 md:flex-row md:items-center md:justify-between"
                            >
                              <div className="min-w-0">
                                <div className="flex flex-wrap items-center gap-2">
                                  <div className="text-sm font-medium text-foreground">
                                    {model.display_name}
                                  </div>
                                  {isDefault ? <Badge variant="default">默认模型</Badge> : null}
                                </div>
                                <div className="mt-2 text-xs text-muted-foreground">
                                  {model.model_id}
                                </div>
                                <div className="mt-3 flex flex-wrap gap-1.5">
                                  {formatCapabilityTags(model.capability_tags).map((tag) => (
                                    <Badge key={tag} variant="outline">
                                      {tag}
                                    </Badge>
                                  ))}
                                </div>
                              </div>
                              <div className="flex shrink-0 items-center gap-2">
                                <Button
                                  size="sm"
                                  variant={isDefault ? "secondary" : "outline"}
                                  disabled={busyAction !== null || isDefault}
                                  onClick={() => void handleSetDefaultModel(model.model_id)}
                                >
                                  {busyAction === "set-default-model" ? (
                                    <Loader2 className="size-4 animate-spin" />
                                  ) : isDefault ? (
                                    <CheckCircle2 className="size-4" />
                                  ) : null}
                                  {isDefault ? "当前默认" : "设为默认模型"}
                                </Button>
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  ))}
                </div>
              </SectionCard>

              <SectionCard
                title="诊断"
                description="聚合 OpenAI 登录、Codex 同步和当前使用状态，帮助你快速判断是否可用。"
              >
                <div className="grid gap-4 md:grid-cols-2">
                  <InfoField label="当前账号" value={activeProfile?.display_label ?? "未登录"} />
                  <InfoField
                    label="账号套餐"
                    value={activeProfile?.chatgpt_plan_type ?? codexRuntime?.auth_plan_type ?? "未知"}
                  />
                  <InfoField label="当前 Provider" value={codexRuntime?.active_provider_key ?? "未设置"} />
                  <InfoField label="当前模型" value={codexRuntime?.model ?? customize?.model_label ?? "未设置"} />
                </div>
                <div className="space-y-2">
                  {diagnostics.length > 0 ? (
                    diagnostics.map((item) => (
                      <DiagnosticRow
                        key={`${item.tone}-${item.message}`}
                        tone={item.tone}
                        message={item.message}
                      />
                    ))
                  ) : (
                    <DiagnosticRow
                      tone="warning"
                      message="正在等待 OpenAI 与 Codex 状态返回。"
                    />
                  )}
                </div>
              </SectionCard>
            </div>
          ) : (
            <EmptyState
              title="正在准备 OpenAI 官方服务"
              body="Warwolf 会自动初始化 OpenAI Provider，并读取当前 Codex 登录与模型状态。"
            />
          )}
        </section>
      </div>
    </div>
  );
}

function StatusBanner({
  tone,
  message,
}: {
  tone: "info" | "success" | "error";
  message: string;
}) {
  const toneClassName =
    tone === "success"
      ? "border-emerald-500/30 bg-emerald-500/10 text-foreground"
      : tone === "info"
        ? "border-border bg-muted/20 text-foreground"
        : "border-destructive/30 bg-destructive/10 text-foreground";
  return (
    <div className={cn("rounded-2xl border px-4 py-3 text-sm", toneClassName)}>{message}</div>
  );
}

function NoticeBar({ message }: { message: string }) {
  return (
    <div className="rounded-2xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-foreground">
      {message}
    </div>
  );
}

function SectionCard({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="rounded-2xl border border-border bg-muted/10 p-4">
      <div className="mb-4">
        <div className="text-sm font-semibold text-foreground">{title}</div>
        <div className="mt-1 text-xs leading-5 text-muted-foreground">{description}</div>
      </div>
      <div className="space-y-4">{children}</div>
    </section>
  );
}

function MetricCard({
  label,
  value,
  hint,
  tone,
}: {
  label: string;
  value: string;
  hint: string;
  tone: "success" | "warning";
}) {
  return (
    <div
      className={cn(
        "rounded-2xl border px-4 py-4",
        tone === "success"
          ? "border-emerald-500/30 bg-emerald-500/10"
          : "border-amber-500/30 bg-amber-500/10"
      )}
    >
      <div className="text-xs uppercase tracking-[0.14em] text-muted-foreground">{label}</div>
      <div className="mt-2 text-lg font-semibold text-foreground">{value}</div>
      <div className="mt-2 text-xs leading-5 text-muted-foreground">{hint}</div>
    </div>
  );
}

function InfoField({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-2xl border border-border bg-background px-4 py-3">
      <div className="text-xs uppercase tracking-[0.14em] text-muted-foreground">{label}</div>
      <div className="mt-2 break-all text-sm text-foreground">{value}</div>
    </div>
  );
}

function ProfileCard({
  profile,
  busyAction,
  onActivate,
  onRefresh,
  onRemove,
}: {
  profile: DesktopCodexProfileSummary;
  busyAction: BusyAction;
  onActivate: () => void;
  onRefresh: () => void;
  onRemove: () => void;
}) {
  const isCurrent = profile.active && profile.applied_to_codex;
  return (
    <div
      className={cn(
        "rounded-2xl border bg-background px-4 py-4",
        isCurrent
          ? "border-emerald-500/35 bg-emerald-500/10"
          : profile.active
            ? "border-primary/35 bg-primary/5"
            : profile.applied_to_codex
              ? "border-sky-500/35 bg-sky-500/10"
              : "border-border"
      )}
    >
      <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <div className="flex items-center gap-2 text-sm font-medium text-foreground">
              <UserRound className="size-4" />
              {profile.display_label}
            </div>
            <Badge variant="outline">{formatCodexAuthSource(profile.auth_source)}</Badge>
            {profile.chatgpt_plan_type ? (
              <Badge variant="outline">{profile.chatgpt_plan_type}</Badge>
            ) : null}
            {profile.active ? <Badge variant="default">当前账号</Badge> : null}
            {profile.applied_to_codex ? (
              <Badge variant={isCurrent ? "default" : "outline"}>Codex 生效中</Badge>
            ) : null}
          </div>
          <div className="mt-2 text-xs text-muted-foreground">{profile.email}</div>
          <div className="mt-2 text-xs text-muted-foreground">
            最近更新：{formatEpoch(profile.updated_at_epoch)}
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant={isCurrent ? "secondary" : "outline"}
            disabled={busyAction !== null || isCurrent}
            onClick={onActivate}
          >
            {busyAction === "activate-profile" ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <CheckCircle2 className="size-4" />
            )}
            {isCurrent ? "当前使用中" : "设为当前账号"}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busyAction !== null}
            onClick={onRefresh}
          >
            {busyAction === "refresh-profile" ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <RefreshCw className="size-4" />
            )}
            刷新令牌
          </Button>
          <Button
            size="sm"
            variant="ghost"
            disabled={busyAction !== null}
            onClick={onRemove}
          >
            {busyAction === "remove-profile" ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Trash2 className="size-4" />
            )}
            移除
          </Button>
        </div>
      </div>
    </div>
  );
}

function DiagnosticRow({
  tone,
  message,
}: {
  tone: "success" | "warning";
  message: string;
}) {
  return (
    <div
      className={cn(
        "flex items-start gap-3 rounded-2xl border px-4 py-3 text-sm text-foreground",
        tone === "success"
          ? "border-emerald-500/30 bg-emerald-500/10"
          : "border-amber-500/30 bg-amber-500/10"
      )}
    >
      {tone === "success" ? (
        <ShieldCheck className="mt-0.5 size-4 shrink-0" />
      ) : (
        <ShieldAlert className="mt-0.5 size-4 shrink-0" />
      )}
      <div>{message}</div>
    </div>
  );
}

function LoadingBlock({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 rounded-2xl border border-border bg-muted/10 px-4 py-3 text-sm text-muted-foreground">
      <Loader2 className="size-4 animate-spin" />
      <span>{label}</span>
    </div>
  );
}

function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex min-h-[420px] items-center justify-center rounded-2xl border border-dashed border-border bg-muted/10 px-8">
      <div className="max-w-md text-center">
        <div className="text-lg font-semibold text-foreground">{title}</div>
        <div className="mt-2 text-sm leading-6 text-muted-foreground">{body}</div>
      </div>
    </div>
  );
}

function resolveCurrentCodexProfile(overview: DesktopCodexAuthOverview | null) {
  if (!overview) return null;
  return (
    overview.profiles.find((profile) => profile.active && profile.applied_to_codex) ??
    overview.profiles.find((profile) => profile.active) ??
    overview.profiles.find((profile) => profile.applied_to_codex) ??
    null
  );
}

function isOpenAiProvider(provider: DesktopManagedProvider) {
  return (
    provider.id === OPENAI_PROVIDER_PRESET_ID ||
    provider.preset_id === OPENAI_PROVIDER_PRESET_ID ||
    provider.provider_type === "codex_openai"
  );
}

function toManagedProviderPayload(
  source: DesktopProviderPreset | DesktopManagedProvider,
  overrides?: Partial<{
    enabled: boolean;
    models: DesktopProviderModel[];
  }>
) {
  const isManagedProvider = isManagedProviderSource(source);
  return {
    id: isManagedProvider ? source.id : OPENAI_PROVIDER_PRESET_ID,
    name: source.name,
    runtime_target: source.runtime_target,
    category: source.category,
    provider_type: source.provider_type,
    billing_category: source.billing_category,
    protocol: source.protocol,
    base_url: source.base_url,
    enabled: overrides?.enabled ?? (isManagedProvider ? source.enabled : true),
    official_verified: source.official_verified,
    preset_id: isManagedProvider
      ? source.preset_id ?? (source.id === OPENAI_PROVIDER_PRESET_ID ? source.id : null)
      : source.id === OPENAI_PROVIDER_PRESET_ID
        ? source.id
        : null,
    website_url: source.website_url ?? null,
    description: source.description ?? null,
    models: overrides?.models ?? source.models,
  };
}

function isManagedProviderSource(
  source: DesktopProviderPreset | DesktopManagedProvider
): source is DesktopManagedProvider {
  return "api_key_masked" in source;
}

function prioritizeModel(models: DesktopProviderModel[], modelId: string) {
  const selected = models.find((model) => model.model_id === modelId);
  if (!selected) {
    throw new Error(`未找到模型：${modelId}`);
  }
  return [selected, ...models.filter((model) => model.model_id !== modelId)];
}

function groupOpenAiModels(models: DesktopProviderModel[]) {
  const groups = new Map<string, DesktopProviderModel[]>();
  for (const model of models) {
    const label = modelGroupLabel(model);
    const current = groups.get(label) ?? [];
    current.push(model);
    groups.set(label, current);
  }
  return ["GPT 5", "GPT 5.1", "GPT 图像", "其他模型"]
    .filter((label) => groups.has(label))
    .map((label) => ({
      label,
      models: groups.get(label) ?? [],
    }));
}

function modelGroupLabel(model: DesktopProviderModel) {
  const haystack = `${model.model_id} ${model.display_name}`.toLowerCase();
  if (haystack.includes("image")) return "GPT 图像";
  if (haystack.includes("gpt-5.1") || haystack.includes("gpt 5.1")) return "GPT 5.1";
  if (haystack.includes("gpt-5") || haystack.includes("gpt 5")) return "GPT 5";
  return "其他模型";
}

function formatCapabilityTags(tags: string[]) {
  if (tags.length === 0) return ["通用"];
  return tags.map((tag) => {
    switch (tag) {
      case "general":
        return "对话";
      case "reasoning":
        return "推理";
      case "coding":
        return "代码";
      case "image":
        return "图像";
      default:
        return tag;
    }
  });
}

function formatCodexAuthSource(source: DesktopCodexAuthSource) {
  return source === "browser_login" ? "浏览器登录" : "导入 auth.json";
}

function formatLoginSessionStatus(status: DesktopCodexLoginSessionSnapshot["status"]) {
  switch (status) {
    case "pending":
      return "等待授权中";
    case "completed":
      return "已完成";
    case "failed":
      return "失败";
    case "cancelled":
      return "已取消";
    default:
      return status;
  }
}

function formatEpoch(epoch: number | null) {
  if (!epoch) return "未知";
  return new Date(epoch * 1000).toLocaleString();
}

function errorMessage(value: unknown) {
  return value instanceof Error ? value.message : undefined;
}

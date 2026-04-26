import { useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronDown,
  Code2,
  Edit3,
  ExternalLink,
  Eye,
  EyeOff,
  Info,
  Loader2,
  Plus,
  RefreshCw,
  TestTube2,
  Trash2,
  UserRound,
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
import {
  activateProvider,
  deleteProvider,
  getSettings,
  listProviders,
  listProviderTemplates,
  testProvider,
  upsertProvider,
  type DesktopProviderSummary,
  type DesktopProviderTemplate,
  type ProviderKind,
  type ProviderTestResult,
} from "@/api/desktop/settings";
import { settingsKeys } from "../api/query";
import {
  activateCodexAuthProfile,
  beginCodexLogin,
  beginManagedAuthLogin,
  getCodexAuthOverview,
  getCodexRuntime,
  getManagedAuthAccounts,
  getManagedAuthProviders,
  importCodexAuthProfile,
  openDashboardUrl,
  pollCodexLogin,
  pollManagedAuthLogin,
  refreshCodexAuthProfile,
  refreshManagedAuthAccount,
  removeCodexAuthProfile,
  removeManagedAuthAccount,
  setManagedAuthDefaultAccount,
  type DesktopCodexAuthOverview,
  type DesktopCodexLoginSessionSnapshot,
  type DesktopCodexProfileSummary,
  type DesktopManagedAuthAccount,
  type DesktopManagedAuthLoginSessionSnapshot,
} from "@/lib/tauri";
import { SubscriptionCodexPool } from "./private-cloud/SubscriptionCodexPool";

const CODEX_PROVIDER_ID = "codex-openai";
const QWEN_PROVIDER_ID = "qwen-code";

type BusyAction =
  | "refresh"
  | "codex-import"
  | "codex-login"
  | "codex-activate"
  | "codex-refresh"
  | "codex-remove"
  | "qwen-login"
  | "qwen-set-default"
  | "qwen-refresh"
  | "qwen-remove"
  | null;

interface Notice {
  tone: "info" | "success" | "error";
  message: string;
}

export function AccountModelSettings({
  privateCloudEnabled,
  error,
}: {
  privateCloudEnabled: boolean;
  error?: string;
}) {
  const queryClient = useQueryClient();
  const [notice, setNotice] = useState<Notice | null>(null);
  const [busyAction, setBusyAction] = useState<BusyAction>(null);
  const [codexLoginSession, setCodexLoginSession] =
    useState<DesktopCodexLoginSessionSnapshot | null>(null);
  const [qwenLoginSession, setQwenLoginSession] =
    useState<DesktopManagedAuthLoginSessionSnapshot | null>(null);
  const [removeCodexProfileId, setRemoveCodexProfileId] =
    useState<string | null>(null);
  const [removeQwenAccountId, setRemoveQwenAccountId] =
    useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingProvider, setEditingProvider] =
    useState<DesktopProviderSummary | null>(null);
  const [pendingDelete, setPendingDelete] =
    useState<DesktopProviderSummary | null>(null);
  const [providerError, setProviderError] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, ProviderTestResult>>({});
  const codexLoginStatusRef = useRef<string | null>(null);
  const qwenLoginStatusRef = useRef<string | null>(null);

  const managedProvidersQuery = useQuery({
    queryKey: settingsKeys.managedAuthProviders(),
    queryFn: async () => (await getManagedAuthProviders()).providers,
    refetchOnWindowFocus: false,
  });

  const codexRuntimeQuery = useQuery({
    queryKey: settingsKeys.codexRuntime(),
    queryFn: async () => (await getCodexRuntime()).runtime,
    refetchOnWindowFocus: false,
  });

  const codexAuthOverviewQuery = useQuery({
    queryKey: settingsKeys.codexAuthOverview(),
    queryFn: async () => (await getCodexAuthOverview()).overview,
    refetchOnWindowFocus: false,
  });

  const qwenAuthQuery = useQuery({
    queryKey: settingsKeys.managedAuthAccounts(QWEN_PROVIDER_ID),
    queryFn: async () => await getManagedAuthAccounts(QWEN_PROVIDER_ID),
    refetchOnWindowFocus: false,
  });

  const settingsQuery = useQuery({
    queryKey: ["desktop", "settings"],
    queryFn: getSettings,
    staleTime: 5 * 60 * 1000,
  });
  const projectPath = settingsQuery.data?.settings?.project_path;

  const providersQuery = useQuery({
    queryKey: ["providers", "list", projectPath ?? ""],
    queryFn: () => listProviders(projectPath),
    staleTime: 30_000,
    enabled: settingsQuery.isSuccess,
  });

  const templatesQuery = useQuery({
    queryKey: ["providers", "templates"],
    queryFn: () => listProviderTemplates(),
    staleTime: Infinity,
  });

  const activateMutation = useMutation({
    mutationFn: (id: string) => activateProvider(id, projectPath),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ["providers", "list", projectPath ?? ""],
      });
      setNotice({ tone: "success", message: "已切换当前服务" });
    },
    onError: (err) => setProviderError(errorMessage(err)),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteProvider(id, projectPath),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ["providers", "list", projectPath ?? ""],
      });
      setNotice({ tone: "success", message: "服务已删除" });
    },
    onError: (err) => setProviderError(errorMessage(err)),
  });

  const upsertMutation = useMutation({
    mutationFn: (request: Parameters<typeof upsertProvider>[0]) =>
      upsertProvider({ ...request, project_path: projectPath }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ["providers", "list", projectPath ?? ""],
      });
      setShowAddForm(false);
      setEditingProvider(null);
      setNotice({ tone: "success", message: "服务配置已保存" });
    },
    onError: (err) => setProviderError(errorMessage(err)),
  });

  const testMutation = useMutation({
    mutationFn: (id: string) => testProvider(id, projectPath),
    onSuccess: (result, id) => {
      setTestResults((prev) => ({ ...prev, [id]: result }));
    },
    onError: (err, id) => {
      setTestResults((prev) => ({
        ...prev,
        [id]: { ok: false, latency_ms: 0, error: errorMessage(err) },
      }));
    },
  });

  const codexProvider = useMemo(
    () =>
      (managedProvidersQuery.data ?? []).find(
        (provider) => provider.id === CODEX_PROVIDER_ID,
      ) ?? null,
    [managedProvidersQuery.data],
  );
  const qwenProvider = qwenAuthQuery.data?.provider ?? null;
  const codexRuntime = codexRuntimeQuery.data ?? null;
  const codexOverview = codexAuthOverviewQuery.data ?? null;
  const qwenAccounts = qwenAuthQuery.data?.accounts ?? [];
  const providers = providersQuery.data?.providers ?? [];
  const activeProviderId = providersQuery.data?.active ?? "";
  const templates = templatesQuery.data?.templates ?? [];

  const activeCodexProfile = useMemo(
    () => resolveCurrentCodexProfile(codexOverview),
    [codexOverview],
  );
  const activeQwenAccount = useMemo(
    () => resolveCurrentManagedAuthAccount(qwenAccounts),
    [qwenAccounts],
  );

  async function refreshOfficialData() {
    await Promise.all([
      managedProvidersQuery.refetch(),
      codexRuntimeQuery.refetch(),
      codexAuthOverviewQuery.refetch(),
      qwenAuthQuery.refetch(),
    ]);
  }

  useEffect(() => {
    if (!codexLoginSession || codexLoginSession.status !== "pending") return;
    const timeoutId = window.setTimeout(async () => {
      try {
        const response = await pollCodexLogin(codexLoginSession.session_id);
        setCodexLoginSession(response.session);
      } catch (pollError) {
        setNotice({
          tone: "error",
          message: errorMessage(pollError) ?? "Codex 登录状态轮询失败",
        });
      }
    }, 1500);
    return () => window.clearTimeout(timeoutId);
  }, [codexLoginSession]);

  useEffect(() => {
    if (!qwenLoginSession || qwenLoginSession.status !== "pending") return;
    const timeoutId = window.setTimeout(async () => {
      try {
        const response = await pollManagedAuthLogin(
          qwenLoginSession.provider_id,
          qwenLoginSession.session_id,
        );
        setQwenLoginSession(response.session);
      } catch (pollError) {
        setNotice({
          tone: "error",
          message: errorMessage(pollError) ?? "Qwen 登录状态轮询失败",
        });
      }
    }, 1800);
    return () => window.clearTimeout(timeoutId);
  }, [qwenLoginSession]);

  useEffect(() => {
    if (!codexLoginSession) return;
    if (codexLoginStatusRef.current === codexLoginSession.status) return;
    codexLoginStatusRef.current = codexLoginSession.status;
    if (codexLoginSession.status === "completed") {
      void refreshOfficialData();
      setNotice({
        tone: "success",
        message: codexLoginSession.profile
          ? `OpenAI 已连接：${codexLoginSession.profile.display_label}`
          : "OpenAI 登录已完成",
      });
    }
    if (codexLoginSession.status === "failed" && codexLoginSession.error) {
      setNotice({ tone: "error", message: codexLoginSession.error });
    }
  }, [codexLoginSession]);

  useEffect(() => {
    if (!qwenLoginSession) return;
    if (qwenLoginStatusRef.current === qwenLoginSession.status) return;
    qwenLoginStatusRef.current = qwenLoginSession.status;
    if (qwenLoginSession.status === "completed") {
      void refreshOfficialData();
      setNotice({
        tone: "success",
        message: qwenLoginSession.account
          ? `Qwen 已连接：${qwenLoginSession.account.display_label}`
          : "Qwen 登录已完成",
      });
    }
    if (qwenLoginSession.status === "failed" && qwenLoginSession.error) {
      setNotice({ tone: "error", message: qwenLoginSession.error });
    }
  }, [qwenLoginSession]);

  async function handleRefresh() {
    setBusyAction("refresh");
    try {
      await refreshOfficialData();
      setNotice({ tone: "success", message: "账户与模型状态已刷新" });
    } catch (refreshError) {
      setNotice({
        tone: "error",
        message: errorMessage(refreshError) ?? "刷新状态失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleImportCodexAuth() {
    setBusyAction("codex-import");
    try {
      await importCodexAuthProfile();
      await refreshOfficialData();
      setNotice({ tone: "success", message: "当前 Codex 登录态已导入" });
    } catch (authError) {
      setNotice({
        tone: "error",
        message: errorMessage(authError) ?? "导入 Codex 登录态失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleBeginCodexLogin() {
    setBusyAction("codex-login");
    try {
      const response = await beginCodexLogin();
      setCodexLoginSession(response.session);
      await openDashboardUrl(response.session.authorize_url);
      setNotice({ tone: "info", message: "已打开 OpenAI 授权页。" });
    } catch (loginError) {
      setNotice({
        tone: "error",
        message: errorMessage(loginError) ?? "OpenAI 登录启动失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleActivateCodexProfile(profileId: string) {
    setBusyAction("codex-activate");
    try {
      await activateCodexAuthProfile(profileId);
      await refreshOfficialData();
      setNotice({ tone: "success", message: "OpenAI 当前账号已切换" });
    } catch (activateError) {
      setNotice({
        tone: "error",
        message: errorMessage(activateError) ?? "切换 OpenAI 当前账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRefreshCodexProfile(profileId: string) {
    setBusyAction("codex-refresh");
    try {
      await refreshCodexAuthProfile(profileId);
      await refreshOfficialData();
      setNotice({ tone: "success", message: "OpenAI 账号令牌已刷新" });
    } catch (refreshError) {
      setNotice({
        tone: "error",
        message: errorMessage(refreshError) ?? "刷新 OpenAI 账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRemoveCodexProfile(profileId: string) {
    setBusyAction("codex-remove");
    try {
      await removeCodexAuthProfile(profileId);
      await refreshOfficialData();
      setNotice({ tone: "success", message: "OpenAI 账号已移除" });
    } catch (removeError) {
      setNotice({
        tone: "error",
        message: errorMessage(removeError) ?? "移除 OpenAI 账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleBeginQwenLogin() {
    setBusyAction("qwen-login");
    try {
      const response = await beginManagedAuthLogin(QWEN_PROVIDER_ID);
      setQwenLoginSession(response.session);
      if (response.session.authorize_url) {
        await openDashboardUrl(response.session.authorize_url);
      }
      setNotice({ tone: "info", message: "已打开 Qwen 授权页。" });
    } catch (loginError) {
      setNotice({
        tone: "error",
        message: errorMessage(loginError) ?? "Qwen 登录启动失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleSetDefaultQwenAccount(accountId: string) {
    setBusyAction("qwen-set-default");
    try {
      await setManagedAuthDefaultAccount(QWEN_PROVIDER_ID, accountId);
      await refreshOfficialData();
      setNotice({ tone: "success", message: "Qwen 当前账号已切换" });
    } catch (switchError) {
      setNotice({
        tone: "error",
        message: errorMessage(switchError) ?? "切换 Qwen 当前账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRefreshQwenAccount(accountId: string) {
    setBusyAction("qwen-refresh");
    try {
      await refreshManagedAuthAccount(QWEN_PROVIDER_ID, accountId);
      await refreshOfficialData();
      setNotice({ tone: "success", message: "Qwen 账号令牌已刷新" });
    } catch (refreshError) {
      setNotice({
        tone: "error",
        message: errorMessage(refreshError) ?? "刷新 Qwen 账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRemoveQwenAccount(accountId: string) {
    setBusyAction("qwen-remove");
    try {
      await removeManagedAuthAccount(QWEN_PROVIDER_ID, accountId);
      await refreshOfficialData();
      setNotice({ tone: "success", message: "Qwen 账号已移除" });
    } catch (removeError) {
      setNotice({
        tone: "error",
        message: errorMessage(removeError) ?? "移除 Qwen 账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <div className="account-model-page">
      {error ? <NoticeBanner tone="error" message={error} /> : null}
      {providerError ? (
        <NoticeBanner tone="error" message={providerError} onClose={() => setProviderError(null)} />
      ) : null}
      {notice ? (
        <NoticeBanner tone={notice.tone} message={notice.message} onClose={() => setNotice(null)} />
      ) : null}

      <section className="account-model-section">
        <div className="account-model-kicker">官方账号</div>
        <div className="account-model-stack">
          <OfficialAccountRow
            logo="openai"
            name="OpenAI"
            status={activeCodexProfile ? "已登录" : "未登录"}
            statusTone={activeCodexProfile ? "active" : "warning"}
            description={`支持 ${compactModelList(
              codexProvider?.models.map((model) => model.model_id) ?? [
                "GPT-5.5",
                "GPT-5.4",
              ],
            )} · OAuth 登录`}
            detail={activeCodexProfile?.display_label}
            actionLabel={activeCodexProfile ? "↗ 管理" : "↗ 登录"}
            actionTone="primary"
            loading={busyAction === "codex-login"}
            disabled={busyAction !== null}
            onAction={() =>
              activeCodexProfile
                ? void openDashboardUrl(codexProvider?.website_url ?? "https://platform.openai.com")
                : void handleBeginCodexLogin()
            }
          />
          <OfficialAccountRow
            logo="qwen"
            name="Qwen Code"
            status={activeQwenAccount ? "已登录" : "未登录"}
            statusTone={activeQwenAccount ? "active" : "warning"}
            description="阿里云通义千问 · coder-model · OAuth 登录"
            detail={activeQwenAccount?.display_label}
            actionLabel={activeQwenAccount ? "↗ 管理" : "↗ 登录"}
            actionTone="secondary"
            loading={busyAction === "qwen-login"}
            disabled={busyAction !== null}
            onAction={() =>
              activeQwenAccount
                ? void openDashboardUrl(qwenProvider?.website_url ?? "https://chat.qwen.ai")
                : void handleBeginQwenLogin()
            }
          />
        </div>
      </section>

      <section className="account-model-section">
        <div className="account-model-section-head">
          <div>
            <div className="account-model-kicker">自定义服务</div>
            <p className="account-model-section-copy">
              通过本地密钥接入第三方 OpenAI 兼容服务
            </p>
          </div>
          <button
            type="button"
            className="account-model-add-button"
            onClick={() => {
              setProviderError(null);
              setEditingProvider(null);
              setShowAddForm((prev) => !prev);
            }}
          >
            <Plus className="size-3.5" strokeWidth={1.6} />
            添加
          </button>
        </div>

        {providersQuery.isLoading ? (
          <SectionLoading label="加载服务配置…" />
        ) : providers.length === 0 ? (
          <div className="account-model-empty">
            还没有自定义服务。可以先添加 DeepSeek、Moonshot 或其它 OpenAI 兼容服务。
          </div>
        ) : (
          <div className="account-model-provider-list">
            {providers.map((provider) => (
              <ProviderCompactRow
                key={provider.id}
                provider={provider}
                isActive={provider.id === activeProviderId}
                testResult={testResults[provider.id]}
                testing={testMutation.isPending && testMutation.variables === provider.id}
                activating={
                  activateMutation.isPending && activateMutation.variables === provider.id
                }
                deleting={deleteMutation.isPending && deleteMutation.variables === provider.id}
                onActivate={() => {
                  setProviderError(null);
                  activateMutation.mutate(provider.id);
                }}
                onTest={() => {
                  setProviderError(null);
                  testMutation.mutate(provider.id);
                }}
                onEdit={() => {
                  setProviderError(null);
                  setEditingProvider(provider);
                  setShowAddForm(false);
                }}
                onDelete={() => {
                  setProviderError(null);
                  setPendingDelete(provider);
                }}
              />
            ))}
          </div>
        )}

        {(showAddForm || editingProvider) && (
          <ProviderForm
            key={editingProvider?.id ?? "new"}
            mode={editingProvider ? "edit" : "add"}
            templates={templates}
            editingProvider={editingProvider}
            submitting={upsertMutation.isPending}
            onCancel={() => {
              setShowAddForm(false);
              setEditingProvider(null);
              setProviderError(null);
            }}
            onSubmit={(request) => {
              setProviderError(null);
              upsertMutation.mutate(request);
            }}
          />
        )}

        <div className="account-model-security-note">
          <Info className="size-3.5" strokeWidth={1.6} aria-hidden="true" />
          <span>
            密钥仅存储在本地配置文件 · 切换当前服务会在下一次对话即时生效
          </span>
        </div>
      </section>

      <details className="account-model-dev">
        <summary>
          <Code2 className="size-3.5" strokeWidth={1.6} aria-hidden="true" />
          <span>开发者高级选项</span>
          <em>· Codex OAuth 同步、本地配置文件路径</em>
          <ChevronDown className="account-model-dev-chevron size-3.5" strokeWidth={1.6} />
        </summary>
        <div className="account-model-dev-body">
          <div className="account-model-dev-actions">
            <button type="button" onClick={() => void handleRefresh()} disabled={busyAction !== null}>
              {busyAction === "refresh" ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
              刷新状态
            </button>
            <button type="button" onClick={() => void handleImportCodexAuth()} disabled={busyAction !== null}>
              {busyAction === "codex-import" ? <Loader2 className="size-3.5 animate-spin" /> : <UserRound className="size-3.5" />}
              导入 Codex 登录态
            </button>
            <button type="button" onClick={() => void openDashboardUrl("https://platform.openai.com")}>
              <ExternalLink className="size-3.5" />
              OpenAI 官网
            </button>
            <button type="button" onClick={() => void openDashboardUrl("https://chat.qwen.ai")}>
              <ExternalLink className="size-3.5" />
              Qwen 官网
            </button>
          </div>

          {codexLoginSession ? (
            <LoginSessionInline
              title="OpenAI 登录会话"
              status={formatLoginStatus(codexLoginSession.status)}
              detail={
                codexLoginSession.profile?.display_label ??
                codexLoginSession.error ??
                codexLoginSession.authorize_url
              }
            />
          ) : null}
          {qwenLoginSession ? (
            <LoginSessionInline
              title="Qwen 登录会话"
              status={formatLoginStatus(qwenLoginSession.status)}
              detail={
                qwenLoginSession.account?.display_label ??
                qwenLoginSession.error ??
                qwenLoginSession.user_code ??
                qwenLoginSession.verification_uri ??
                ""
              }
            />
          ) : null}

          <div className="account-model-dev-grid">
            <PathField label="Codex 配置" value={codexRuntime?.config_path} />
            <PathField label="Codex Auth" value={codexRuntime?.auth_path} />
            <PathField label="Qwen 配置" value={qwenProvider?.runtime.config_path} />
            <PathField label="Qwen Auth" value={qwenProvider?.runtime.auth_path} />
            <PathField
              label="服务配置文件"
              value={projectPath ? `${projectPath}\\.claw\\providers.json` : ".claw/providers.json"}
            />
          </div>

          <CompactAccountList
            title="OpenAI OAuth 账号"
            empty="还没有 OpenAI OAuth 账号。"
            profiles={codexOverview?.profiles ?? []}
            busyAction={busyAction}
            onActivate={handleActivateCodexProfile}
            onRefresh={handleRefreshCodexProfile}
            onRemove={setRemoveCodexProfileId}
          />
          <CompactManagedAccountList
            title="Qwen OAuth 账号"
            empty="还没有 Qwen OAuth 账号。"
            accounts={qwenAccounts}
            busyAction={busyAction}
            onActivate={handleSetDefaultQwenAccount}
            onRefresh={handleRefreshQwenAccount}
            onRemove={setRemoveQwenAccountId}
          />

          {privateCloudEnabled ? <SubscriptionCodexPool /> : null}
        </div>
      </details>

      <ConfirmDialog
        open={Boolean(removeCodexProfileId)}
        onOpenChange={(open) => {
          if (!open) setRemoveCodexProfileId(null);
        }}
        title="移除 OpenAI 账号"
        description="移除后，该账号不会再作为当前 OpenAI OAuth 账号。"
        confirmLabel="移除"
        cancelLabel="取消"
        variant="destructive"
        onConfirm={() => {
          if (removeCodexProfileId) {
            void handleRemoveCodexProfile(removeCodexProfileId);
          }
        }}
      />

      <ConfirmDialog
        open={Boolean(removeQwenAccountId)}
        onOpenChange={(open) => {
          if (!open) setRemoveQwenAccountId(null);
        }}
        title="移除 Qwen 账号"
        description="移除后，该账号不会再作为当前 Qwen OAuth 账号。"
        confirmLabel="移除"
        cancelLabel="取消"
        variant="destructive"
        onConfirm={() => {
          if (removeQwenAccountId) {
            void handleRemoveQwenAccount(removeQwenAccountId);
          }
        }}
      />

      <ConfirmDialog
        open={Boolean(pendingDelete)}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
        title="删除服务"
        description={
          pendingDelete
            ? `确定要删除「${pendingDelete.display_name || pendingDelete.id}」吗？对应的本地密钥会一并清除。`
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

function OfficialAccountRow({
  logo,
  name,
  status,
  statusTone,
  description,
  detail,
  actionLabel,
  actionTone,
  loading,
  disabled,
  onAction,
}: {
  logo: "openai" | "qwen";
  name: string;
  status: string;
  statusTone: "active" | "warning" | "idle";
  description: string;
  detail?: string;
  actionLabel: string;
  actionTone: "primary" | "secondary";
  loading: boolean;
  disabled: boolean;
  onAction: () => void;
}) {
  return (
    <div className="account-model-official-row">
      <div className={`account-model-logo account-model-logo--${logo}`}>
        {logo === "openai" ? "AI" : "QW"}
      </div>
      <div className="account-model-row-main">
        <div className="account-model-row-title">
          <span>{name}</span>
          <Pill tone={statusTone}>{status}</Pill>
        </div>
        <p className="account-model-row-description">
          {description}
          {detail ? <span> · {detail}</span> : null}
        </p>
      </div>
      <button
        type="button"
        className={`account-model-button account-model-button--${actionTone}`}
        onClick={onAction}
        disabled={disabled}
      >
        {loading ? <Loader2 className="size-3.5 animate-spin" /> : null}
        {actionLabel}
      </button>
    </div>
  );
}

function ProviderCompactRow({
  provider,
  isActive,
  testResult,
  testing,
  activating,
  deleting,
  onActivate,
  onTest,
  onEdit,
  onDelete,
}: {
  provider: DesktopProviderSummary;
  isActive: boolean;
  testResult?: ProviderTestResult;
  testing: boolean;
  activating: boolean;
  deleting: boolean;
  onActivate: () => void;
  onTest: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className="account-model-provider-row"
      data-active={isActive || undefined}
    >
      <div className="account-model-row-main">
        <div className="account-model-row-title">
          <span>{provider.display_name || provider.id}</span>
          <button
            type="button"
            className="account-model-pill-button"
            onClick={isActive || activating ? undefined : onActivate}
            disabled={isActive || activating}
            title={isActive ? "当前使用中" : "切换为使用中"}
          >
            {activating ? <Loader2 className="size-3 animate-spin" /> : null}
            <Pill tone={isActive ? "active" : "idle"}>
              {isActive ? "使用中" : "待用"}
            </Pill>
          </button>
          <Pill tone="tech">
            {provider.kind === "anthropic" ? "Anthropic" : "OpenAI 兼容"}
          </Pill>
          <TestResultPill result={testResult} testing={testing} />
        </div>
        <p className="account-model-provider-meta">
          {provider.model} · 输出上限 {provider.max_tokens} ·{" "}
          {provider.api_key_display ? "密钥已保存" : "未保存密钥"}
        </p>
      </div>
      <div className="account-model-provider-actions">
        <IconButton
          label="测试连接"
          onClick={onTest}
          disabled={testing}
          icon={testing ? <Loader2 className="size-3.5 animate-spin" /> : <TestTube2 className="size-3.5" />}
        />
        <IconButton
          label="编辑服务"
          onClick={onEdit}
          icon={<Edit3 className="size-3.5" />}
        />
        <IconButton
          label="删除服务"
          onClick={onDelete}
          disabled={deleting}
          icon={deleting ? <Loader2 className="size-3.5 animate-spin" /> : <Trash2 className="size-3.5" />}
        />
      </div>
    </div>
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
  const defaultTemplateId = useMemo(() => {
    if (isEdit) {
      const matchingTemplate = templates.find(
        (template) => template.kind === editingProvider.kind,
      );
      return matchingTemplate?.id ?? templates[0]?.id ?? "";
    }
    return templates[0]?.id ?? "";
  }, [isEdit, editingProvider, templates]);
  const [templateId, setTemplateId] = useState(defaultTemplateId);
  const [customId, setCustomId] = useState(isEdit ? editingProvider.id : "");
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [customModel, setCustomModel] = useState(
    isEdit ? editingProvider.model : "",
  );
  const [customBaseUrl, setCustomBaseUrl] = useState(
    isEdit ? editingProvider.base_url : "",
  );

  const selectedTemplate = useMemo(
    () => templates.find((template) => template.id === templateId),
    [templates, templateId],
  );

  useEffect(() => {
    if (isEdit || !selectedTemplate) return;
    setCustomId((prev) => (prev === "" ? selectedTemplate.id : prev));
    setCustomModel((prev) =>
      prev === "" ? selectedTemplate.default_model : prev,
    );
    setCustomBaseUrl((prev) =>
      prev === "" ? selectedTemplate.base_url : prev,
    );
  }, [selectedTemplate, isEdit]);

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!selectedTemplate || !customId.trim() || !customModel.trim()) return;
    if (!isEdit && !apiKey.trim()) return;
    const kind: ProviderKind = isEdit
      ? editingProvider.kind
      : selectedTemplate.kind;
    onSubmit({
      id: customId.trim(),
      entry: {
        kind,
        display_name:
          isEdit && editingProvider.display_name
            ? editingProvider.display_name
            : selectedTemplate.display_name,
        base_url:
          kind === "openai_compat"
            ? customBaseUrl.trim() || selectedTemplate.base_url
            : undefined,
        api_key: apiKey,
        model: customModel.trim(),
        max_tokens: isEdit
          ? editingProvider.max_tokens
          : selectedTemplate.max_tokens,
      },
    });
  }

  return (
    <form className="account-model-provider-form" onSubmit={handleSubmit}>
      <div className="account-model-form-title">
        {isEdit ? `编辑服务：${editingProvider.id}` : "添加自定义服务"}
      </div>
      {!isEdit ? (
        <label>
          <span>厂商模板</span>
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
              {templates.map((template) => (
                <SelectItem key={template.id} value={template.id}>
                  {template.display_name} · {template.description}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
      ) : null}
      <div className="account-model-form-grid">
        <label>
          <span>服务别名</span>
          <Input
            value={customId}
            onChange={(event) => setCustomId(event.target.value)}
            placeholder="deepseek-prod"
            readOnly={isEdit}
            disabled={isEdit}
          />
        </label>
        <label>
          <span>模型</span>
          <Input
            value={customModel}
            onChange={(event) => setCustomModel(event.target.value)}
            placeholder={selectedTemplate?.default_model ?? "model-id"}
          />
        </label>
      </div>
      {(isEdit
        ? editingProvider.kind === "openai_compat"
        : selectedTemplate?.kind === "openai_compat") ? (
        <label>
          <span>服务地址</span>
          <Input
            value={customBaseUrl}
            onChange={(event) => setCustomBaseUrl(event.target.value)}
            placeholder="服务接口地址"
          />
        </label>
      ) : null}
      <label>
        <span>本地密钥</span>
        <div className="account-model-key-field">
          <Input
            type={showKey ? "text" : "password"}
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
            placeholder={isEdit ? "留空则保留当前密钥" : "输入服务密钥"}
            autoComplete="off"
          />
          <button
            type="button"
            onClick={() => setShowKey((prev) => !prev)}
            aria-label={showKey ? "隐藏密钥" : "显示密钥"}
          >
            {showKey ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
          </button>
        </div>
      </label>
      <div className="account-model-form-actions">
        <Button type="button" size="sm" variant="ghost" onClick={onCancel} disabled={submitting}>
          取消
        </Button>
        <Button type="submit" size="sm" disabled={submitting}>
          {submitting ? <Loader2 className="mr-1 size-3 animate-spin" /> : null}
          {isEdit ? "保存修改" : "保存并激活"}
        </Button>
      </div>
    </form>
  );
}

function Pill({
  tone,
  children,
}: {
  tone: "active" | "warning" | "idle" | "tech";
  children: ReactNode;
}) {
  return <span className={`account-model-pill account-model-pill--${tone}`}>{children}</span>;
}

function TestResultPill({
  result,
  testing,
}: {
  result?: ProviderTestResult;
  testing: boolean;
}) {
  if (testing) {
    return <Pill tone="idle">测试中</Pill>;
  }
  if (!result) return null;
  return <Pill tone={result.ok ? "active" : "warning"}>{result.ok ? `${result.latency_ms}ms` : "失败"}</Pill>;
}

function IconButton({
  label,
  icon,
  disabled,
  onClick,
}: {
  label: string;
  icon: ReactNode;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="account-model-icon-button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      {icon}
    </button>
  );
}

function NoticeBanner({
  tone,
  message,
  onClose,
}: Notice & {
  onClose?: () => void;
}) {
  return (
    <div className={`account-model-notice account-model-notice--${tone}`}>
      <span>{message}</span>
      {onClose ? (
        <button type="button" onClick={onClose}>
          关闭
        </button>
      ) : null}
    </div>
  );
}

function SectionLoading({ label }: { label: string }) {
  return (
    <div className="account-model-empty">
      <Loader2 className="size-3.5 animate-spin" />
      {label}
    </div>
  );
}

function LoginSessionInline({
  title,
  status,
  detail,
}: {
  title: string;
  status: string;
  detail: string;
}) {
  return (
    <div className="account-model-mini-row">
      <span>{title}</span>
      <Pill tone={status === "已完成" ? "active" : status === "失败" ? "warning" : "idle"}>
        {status}
      </Pill>
      <code>{detail}</code>
    </div>
  );
}

function PathField({ label, value }: { label: string; value?: string | null }) {
  return (
    <div className="account-model-path">
      <span>{label}</span>
      <code>{value || "未上报"}</code>
    </div>
  );
}

function CompactAccountList({
  title,
  empty,
  profiles,
  busyAction,
  onActivate,
  onRefresh,
  onRemove,
}: {
  title: string;
  empty: string;
  profiles: DesktopCodexProfileSummary[];
  busyAction: BusyAction;
  onActivate: (id: string) => Promise<void>;
  onRefresh: (id: string) => Promise<void>;
  onRemove: (id: string) => void;
}) {
  return (
    <div className="account-model-account-list">
      <div className="account-model-account-list-title">{title}</div>
      {profiles.length === 0 ? (
        <div className="account-model-empty">{empty}</div>
      ) : (
        profiles.map((profile) => (
          <div className="account-model-mini-row" key={profile.id}>
            <span>{profile.display_label}</span>
            {profile.active ? <Pill tone="active">当前</Pill> : <Pill tone="idle">备用</Pill>}
            <code>{profile.email}</code>
            <div className="account-model-mini-actions">
              <button
                type="button"
                onClick={() => void onActivate(profile.id)}
                disabled={busyAction !== null || profile.active}
              >
                使用
              </button>
              <button
                type="button"
                onClick={() => void onRefresh(profile.id)}
                disabled={busyAction !== null}
              >
                刷新
              </button>
              <button
                type="button"
                onClick={() => onRemove(profile.id)}
                disabled={busyAction !== null}
              >
                移除
              </button>
            </div>
          </div>
        ))
      )}
    </div>
  );
}

function CompactManagedAccountList({
  title,
  empty,
  accounts,
  busyAction,
  onActivate,
  onRefresh,
  onRemove,
}: {
  title: string;
  empty: string;
  accounts: DesktopManagedAuthAccount[];
  busyAction: BusyAction;
  onActivate: (id: string) => Promise<void>;
  onRefresh: (id: string) => Promise<void>;
  onRemove: (id: string) => void;
}) {
  return (
    <div className="account-model-account-list">
      <div className="account-model-account-list-title">{title}</div>
      {accounts.length === 0 ? (
        <div className="account-model-empty">{empty}</div>
      ) : (
        accounts.map((account) => (
          <div className="account-model-mini-row" key={account.id}>
            <span>{account.display_label}</span>
            {account.is_default ? <Pill tone="active">当前</Pill> : <Pill tone="idle">备用</Pill>}
            <code>{account.email ?? account.status}</code>
            <div className="account-model-mini-actions">
              <button
                type="button"
                onClick={() => void onActivate(account.id)}
                disabled={busyAction !== null || account.is_default}
              >
                使用
              </button>
              <button
                type="button"
                onClick={() => void onRefresh(account.id)}
                disabled={busyAction !== null}
              >
                刷新
              </button>
              <button
                type="button"
                onClick={() => onRemove(account.id)}
                disabled={busyAction !== null}
              >
                移除
              </button>
            </div>
          </div>
        ))
      )}
    </div>
  );
}

function resolveCurrentCodexProfile(overview: DesktopCodexAuthOverview | null) {
  if (!overview) return null;
  return (
    overview.profiles.find((profile) => profile.active) ??
    overview.profiles.find((profile) => profile.applied_to_codex) ??
    null
  );
}

function resolveCurrentManagedAuthAccount(accounts: DesktopManagedAuthAccount[]) {
  return (
    accounts.find((account) => account.is_default) ??
    accounts.find((account) => account.applied_to_runtime) ??
    null
  );
}

function formatLoginStatus(status: string) {
  switch (status) {
    case "pending":
      return "等待确认";
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

function compactModelList(models: string[]) {
  if (models.length === 0) return "未提供模型";
  if (models.length <= 2) return models.join(" / ");
  return `${models.slice(0, 2).join(" / ")} 等 ${models.length} 个模型`;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "操作失败";
}

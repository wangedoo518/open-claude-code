import type { ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ExternalLink,
  Loader2,
  LogIn,
  RefreshCw,
  Trash2,
  UserRound,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
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
  type DesktopCustomizeState,
  type DesktopManagedAuthAccount,
  type DesktopManagedAuthLoginSessionSnapshot,
} from "@/lib/tauri";

const CODEX_PROVIDER_ID = "codex-openai";
const QWEN_PROVIDER_ID = "qwen-code";

interface ProviderSettingsProps {
  customize: DesktopCustomizeState | null;
  error?: string;
}

interface Notice {
  tone: "info" | "success" | "error";
  message: string;
}

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

export function ProviderSettings({ error }: ProviderSettingsProps) {
  const [notice, setNotice] = useState<Notice | null>(null);
  const [busyAction, setBusyAction] = useState<BusyAction>(null);
  const [codexLoginSession, setCodexLoginSession] =
    useState<DesktopCodexLoginSessionSnapshot | null>(null);
  const [qwenLoginSession, setQwenLoginSession] =
    useState<DesktopManagedAuthLoginSessionSnapshot | null>(null);
  const [removeCodexProfileId, setRemoveCodexProfileId] = useState<string | null>(null);
  const [removeQwenAccountId, setRemoveQwenAccountId] = useState<string | null>(null);
  const codexLoginStatusRef = useRef<string | null>(null);
  const qwenLoginStatusRef = useRef<string | null>(null);

  const providersQuery = useQuery({
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

  const codexProvider = useMemo(
    () =>
      (providersQuery.data ?? []).find((provider) => provider.id === CODEX_PROVIDER_ID) ??
      null,
    [providersQuery.data]
  );
  const qwenProvider = qwenAuthQuery.data?.provider ?? null;
  const codexRuntime = codexRuntimeQuery.data ?? null;
  const codexOverview = codexAuthOverviewQuery.data ?? null;
  const qwenAccounts = qwenAuthQuery.data?.accounts ?? [];

  const activeCodexProfile = useMemo(
    () => resolveCurrentCodexProfile(codexOverview),
    [codexOverview]
  );
  const activeQwenAccount = useMemo(
    () => resolveCurrentManagedAuthAccount(qwenAccounts),
    [qwenAccounts]
  );
  const qwenAvailableModels = useMemo(
    () => qwenProvider?.models.map((model) => model.model_id) ?? [],
    [qwenProvider]
  );

  async function refreshPageData() {
    await Promise.all([
      providersQuery.refetch(),
      codexRuntimeQuery.refetch(),
      codexAuthOverviewQuery.refetch(),
      qwenAuthQuery.refetch(),
    ]);
  }

  useEffect(() => {
    if (!codexLoginSession || codexLoginSession.status !== "pending") {
      return;
    }
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
    if (!qwenLoginSession || qwenLoginSession.status !== "pending") {
      return;
    }
    const timeoutId = window.setTimeout(async () => {
      try {
        const response = await pollManagedAuthLogin(
          qwenLoginSession.provider_id,
          qwenLoginSession.session_id
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
    if (!codexLoginSession) {
      return;
    }
    if (codexLoginStatusRef.current === codexLoginSession.status) {
      return;
    }
    codexLoginStatusRef.current = codexLoginSession.status;
    if (codexLoginSession.status === "completed") {
      void refreshPageData();
      setNotice({
        tone: "success",
        message: codexLoginSession.profile
          ? `Codex OAuth 已连接：${codexLoginSession.profile.display_label}`
          : "Codex OAuth 登录已完成",
      });
    }
    if (codexLoginSession.status === "failed" && codexLoginSession.error) {
      setNotice({ tone: "error", message: codexLoginSession.error });
    }
  }, [codexLoginSession]);

  useEffect(() => {
    if (!qwenLoginSession) {
      return;
    }
    if (qwenLoginStatusRef.current === qwenLoginSession.status) {
      return;
    }
    qwenLoginStatusRef.current = qwenLoginSession.status;
    if (qwenLoginSession.status === "completed") {
      void refreshPageData();
      setNotice({
        tone: "success",
        message: qwenLoginSession.account
          ? `Qwen OAuth 已连接：${qwenLoginSession.account.display_label}`
          : "Qwen OAuth 登录已完成",
      });
    }
    if (qwenLoginSession.status === "failed" && qwenLoginSession.error) {
      setNotice({ tone: "error", message: qwenLoginSession.error });
    }
  }, [qwenLoginSession]);

  async function handleRefresh() {
    setBusyAction("refresh");
    try {
      await refreshPageData();
      setNotice({ tone: "success", message: "模型服务状态已刷新" });
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
      await refreshPageData();
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
      setNotice({
        tone: "info",
        message: "已打开 Codex OAuth 授权页，请在浏览器完成登录。",
      });
    } catch (loginError) {
      setNotice({
        tone: "error",
        message: errorMessage(loginError) ?? "Codex OAuth 登录启动失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleActivateCodexProfile(profileId: string) {
    setBusyAction("codex-activate");
    try {
      await activateCodexAuthProfile(profileId);
      await refreshPageData();
      setNotice({ tone: "success", message: "Codex 当前账号已切换" });
    } catch (activateError) {
      setNotice({
        tone: "error",
        message: errorMessage(activateError) ?? "切换 Codex 当前账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRefreshCodexProfile(profileId: string) {
    setBusyAction("codex-refresh");
    try {
      await refreshCodexAuthProfile(profileId);
      await refreshPageData();
      setNotice({ tone: "success", message: "Codex 账号令牌已刷新" });
    } catch (refreshError) {
      setNotice({
        tone: "error",
        message: errorMessage(refreshError) ?? "刷新 Codex 账号失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRemoveCodexProfile(profileId: string) {
    setBusyAction("codex-remove");
    try {
      await removeCodexAuthProfile(profileId);
      await refreshPageData();
      setNotice({ tone: "success", message: "Codex 账号已移除" });
    } catch (removeError) {
      setNotice({
        tone: "error",
        message: errorMessage(removeError) ?? "移除 Codex 账号失败",
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
      setNotice({
        tone: "info",
        message: "已打开 Qwen OAuth 授权页，请在浏览器完成登录。",
      });
    } catch (loginError) {
      setNotice({
        tone: "error",
        message: errorMessage(loginError) ?? "Qwen OAuth 登录启动失败",
      });
    } finally {
      setBusyAction(null);
    }
  }

  async function handleSetDefaultQwenAccount(accountId: string) {
    setBusyAction("qwen-set-default");
    try {
      await setManagedAuthDefaultAccount(QWEN_PROVIDER_ID, accountId);
      await refreshPageData();
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
      await refreshPageData();
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
      await refreshPageData();
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
    <div className="flex h-full min-h-0 gap-5">
      <aside className="w-[300px] shrink-0 rounded-2xl border border-border bg-background p-4">
        <div className="mb-4 text-sm font-medium text-foreground">模型服务</div>
        <ScrollArea className="h-[calc(100vh-260px)] pr-3">
          <div className="space-y-3">
            <ProviderSummaryCard
              title="OpenAI"
              subtitle={
                activeCodexProfile
                  ? `已连接 ${activeCodexProfile.display_label}`
                  : "尚未登录"
              }
              badges={[
                "官方",
                codexRuntime?.active_provider_key === CODEX_PROVIDER_ID ? "已写入 ~/.codex" : "未写入 ~/.codex",
                codexProvider?.default_model_id ?? codexRuntime?.model ?? "gpt-5.4",
              ]}
            />
            <ProviderSummaryCard
              title="Qwen Code"
              subtitle={
                activeQwenAccount
                  ? `当前账号：${activeQwenAccount.display_label}`
                  : "尚未登录"
              }
              badges={[
                "官方 OAuth",
                qwenProvider?.runtime.synced ? "已写入 ~/.qwen" : "未写入 ~/.qwen",
                ...(qwenAvailableModels.length > 0 ? qwenAvailableModels : ["coder-model"]),
              ]}
              accent="qwen"
            />
          </div>
        </ScrollArea>
      </aside>

      <div className="min-w-0 flex-1 overflow-y-auto">
        <div className="space-y-5">
          {error ? <NoticeBanner tone="error" message={error} /> : null}
          {notice ? <NoticeBanner tone={notice.tone} message={notice.message} /> : null}

          <SectionCard
            title="OpenAI"
            description="仅通过 Codex OAuth 管理账号，并把当前账号同步给 ~/.codex。"
            actions={
              <>
                <Button
                  variant="outline"
                  onClick={() => void openDashboardUrl(codexProvider?.website_url ?? "https://platform.openai.com")}
                >
                  <ExternalLink className="size-4" />
                  官网
                </Button>
                <Button variant="outline" onClick={() => void handleRefresh()} disabled={busyAction !== null}>
                  {busyAction === "refresh" ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <RefreshCw className="size-4" />
                  )}
                  刷新状态
                </Button>
              </>
            }
          >
            <div className="grid gap-4 md:grid-cols-2">
              <InfoField
                label="当前状态"
                value={activeCodexProfile ? "已登录" : "未登录"}
                hint={activeCodexProfile ? `当前账号：${activeCodexProfile.display_label}` : "尚未检测到 Codex OAuth 账号"}
                tone={activeCodexProfile ? "success" : "warning"}
              />
              <InfoField
                label="同步状态"
                value={codexRuntime?.active_provider_key === CODEX_PROVIDER_ID ? "已写入" : "未写入"}
                hint={codexRuntime?.config_path ? `同步目标：${codexRuntime.config_path}` : undefined}
                tone={codexRuntime?.active_provider_key === CODEX_PROVIDER_ID ? "success" : "warning"}
              />
              <InfoField
                label="默认模型"
                value={codexProvider?.default_model_id ?? codexRuntime?.model ?? "gpt-5.4"}
              />
              <InfoField
                label="可用模型"
                value={compactModelList(codexProvider?.models.map((model) => model.model_id) ?? [])}
              />
            </div>

            <div className="mt-4 flex flex-wrap gap-2">
              <Button onClick={() => void handleBeginCodexLogin()} disabled={busyAction !== null}>
                {busyAction === "codex-login" ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <LogIn className="size-4" />
                )}
                使用 ChatGPT 登录
              </Button>
              <Button variant="outline" onClick={() => void handleImportCodexAuth()} disabled={busyAction !== null}>
                {busyAction === "codex-import" ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <RefreshCw className="size-4" />
                )}
                导入当前 Codex 登录态
              </Button>
            </div>

            {codexLoginSession ? (
              <LoginSessionCard
                title="Codex OAuth 登录会话"
                status={formatLoginStatus(codexLoginSession.status)}
                lines={[
                  codexLoginSession.authorize_url ? `授权地址：${codexLoginSession.authorize_url}` : null,
                  codexLoginSession.profile ? `已导入账号：${codexLoginSession.profile.display_label}` : null,
                  codexLoginSession.error ? `错误：${codexLoginSession.error}` : null,
                ]}
              />
            ) : null}

            <div className="mt-5 space-y-3">
              {codexOverview?.profiles.length ? (
                codexOverview.profiles.map((profile) => (
                  <CodexProfileCard
                    key={profile.id}
                    profile={profile}
                    busyAction={busyAction}
                    onActivate={() => void handleActivateCodexProfile(profile.id)}
                    onRefresh={() => void handleRefreshCodexProfile(profile.id)}
                    onRemove={() => setRemoveCodexProfileId(profile.id)}
                  />
                ))
              ) : (
                <EmptyBlock label="还没有 Codex OAuth 账号，可直接浏览器登录或导入当前 ~/.codex/auth.json。" />
              )}
            </div>
          </SectionCard>

          <SectionCard
            title="Qwen Code"
            description="仅通过 Qwen OAuth 管理账号，并把当前账号同步给 ~/.qwen。"
            actions={
              <>
                <Button
                  variant="outline"
                  onClick={() => void openDashboardUrl(qwenProvider?.website_url ?? "https://chat.qwen.ai")}
                >
                  <ExternalLink className="size-4" />
                  官网
                </Button>
                <Button variant="outline" onClick={() => void handleRefresh()} disabled={busyAction !== null}>
                  {busyAction === "refresh" ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <RefreshCw className="size-4" />
                  )}
                  刷新状态
                </Button>
              </>
            }
          >
            <div className="grid gap-4 md:grid-cols-2">
              <InfoField
                label="当前状态"
                value={activeQwenAccount ? "已登录" : "未登录"}
                hint={activeQwenAccount ? `当前账号：${activeQwenAccount.display_label}` : "尚未检测到 Qwen OAuth 账号"}
                tone={activeQwenAccount ? "success" : "warning"}
              />
              <InfoField
                label="同步状态"
                value={qwenProvider?.runtime.synced ? "已写入" : "未写入"}
                hint={qwenProvider?.runtime.auth_path ? `同步目标：${qwenProvider.runtime.auth_path}` : undefined}
                tone={qwenProvider?.runtime.synced ? "success" : "warning"}
              />
              <InfoField
                label="默认模型"
                value={qwenProvider?.default_model_id ?? "coder-model"}
              />
              <InfoField
                label="可用模型"
                value={compactModelList(qwenAvailableModels)}
              />
            </div>

            <div className="mt-4 flex flex-wrap gap-2">
              <Button onClick={() => void handleBeginQwenLogin()} disabled={busyAction !== null}>
                {busyAction === "qwen-login" ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <LogIn className="size-4" />
                )}
                浏览器登录
              </Button>
            </div>

            {qwenLoginSession ? (
              <LoginSessionCard
                title="Qwen OAuth 登录会话"
                status={formatLoginStatus(qwenLoginSession.status)}
                lines={[
                  qwenLoginSession.user_code ? `用户代码：${qwenLoginSession.user_code}` : null,
                  qwenLoginSession.verification_uri ? `验证地址：${qwenLoginSession.verification_uri}` : null,
                  qwenLoginSession.account ? `已导入账号：${qwenLoginSession.account.display_label}` : null,
                  qwenLoginSession.error ? `错误：${qwenLoginSession.error}` : null,
                ]}
              />
            ) : null}

            <div className="mt-5 space-y-3">
              {qwenAccounts.length ? (
                qwenAccounts.map((account) => (
                  <ManagedAuthAccountCard
                    key={account.id}
                    account={account}
                    busyAction={busyAction}
                    onActivate={() => void handleSetDefaultQwenAccount(account.id)}
                    onRefresh={() => void handleRefreshQwenAccount(account.id)}
                    onRemove={() => setRemoveQwenAccountId(account.id)}
                  />
                ))
              ) : (
                <EmptyBlock label="还没有 Qwen OAuth 账号，完成一次浏览器登录后会显示在这里。" />
              )}
            </div>
          </SectionCard>
        </div>
      </div>

      <ConfirmDialog
        open={Boolean(removeCodexProfileId)}
        onOpenChange={(open) => {
          if (!open) {
            setRemoveCodexProfileId(null);
          }
        }}
        title="移除 Codex 账号"
        description="移除后，该账号不会再作为 Warwolf 的当前 Codex OAuth 账号。"
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
          if (!open) {
            setRemoveQwenAccountId(null);
          }
        }}
        title="移除 Qwen 账号"
        description="移除后，该账号不会再作为 Warwolf 的当前 Qwen OAuth 账号。"
        confirmLabel="移除"
        cancelLabel="取消"
        variant="destructive"
        onConfirm={() => {
          if (removeQwenAccountId) {
            void handleRemoveQwenAccount(removeQwenAccountId);
          }
        }}
      />
    </div>
  );
}

function ProviderSummaryCard({
  title,
  subtitle,
  badges,
  accent = "openai",
}: {
  title: string;
  subtitle: string;
  badges: string[];
  accent?: "openai" | "qwen";
}) {
  return (
    <div className="rounded-2xl border border-border bg-background p-4">
      <div className="flex items-start gap-3">
        <div
          className={cn(
            "flex size-11 shrink-0 items-center justify-center rounded-full text-sm font-semibold text-white",
            accent === "qwen" ? "bg-[#FF6A00]" : "bg-black"
          )}
        >
          {accent === "qwen" ? "QW" : "AI"}
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-base font-semibold text-foreground">{title}</div>
          <div className="mt-1 text-sm text-muted-foreground">{subtitle}</div>
          <div className="mt-3 flex flex-wrap gap-1.5">
            {badges.map((badge) => (
              <Badge key={badge} variant="outline">
                {badge}
              </Badge>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function SectionCard({
  title,
  description,
  actions,
  children,
}: {
  title: string;
  description: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="rounded-2xl border border-border bg-background p-5">
      <div className="flex flex-col gap-4 border-b border-border pb-5 md:flex-row md:items-start md:justify-between">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-2xl font-semibold text-foreground">{title}</h3>
            <Badge variant="outline">官方 OAuth</Badge>
          </div>
          <p className="mt-2 max-w-3xl text-sm leading-6 text-muted-foreground">
            {description}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">{actions}</div>
      </div>
      <div className="mt-5">{children}</div>
    </section>
  );
}

function NoticeBanner({ tone, message }: Notice) {
  return (
    <div
      className={cn(
        "rounded-xl border px-4 py-3 text-sm",
        tone === "success" &&
          "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
        tone === "info" &&
          "border-blue-500/30 bg-blue-500/10 text-blue-700 dark:text-blue-300",
        tone === "error" &&
          "border-red-500/30 bg-red-500/10 text-red-700 dark:text-red-300"
      )}
    >
      {message}
    </div>
  );
}

function InfoField({
  label,
  value,
  hint,
  tone = "default",
}: {
  label: string;
  value: string;
  hint?: string;
  tone?: "default" | "success" | "warning";
}) {
  return (
    <div className="rounded-2xl border border-border bg-muted/20 p-4">
      <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </div>
      <div
        className={cn(
          "mt-2 text-lg font-semibold",
          tone === "success" && "text-emerald-600 dark:text-emerald-300",
          tone === "warning" && "text-amber-600 dark:text-amber-300",
          tone === "default" && "text-foreground"
        )}
      >
        {value}
      </div>
      {hint ? <div className="mt-1 text-sm text-muted-foreground">{hint}</div> : null}
    </div>
  );
}

function LoginSessionCard({
  title,
  status,
  lines,
}: {
  title: string;
  status: string;
  lines: Array<string | null>;
}) {
  return (
    <div className="mt-4 rounded-2xl border border-border bg-muted/20 p-4">
      <div className="flex items-center gap-2">
        <div className="text-sm font-semibold text-foreground">{title}</div>
        <Badge variant="outline">{status}</Badge>
      </div>
      <div className="mt-3 space-y-1.5 text-sm text-muted-foreground">
        {lines.filter(Boolean).map((line) => (
          <div key={line}>{line}</div>
        ))}
      </div>
    </div>
  );
}

function CodexProfileCard({
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
  return (
    <div className="rounded-2xl border border-border bg-background p-4">
      <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <UserRound className="size-4 text-muted-foreground" />
            <div className="font-medium text-foreground">{profile.display_label}</div>
            {profile.active ? <Badge variant="default">当前账号</Badge> : null}
            {profile.applied_to_codex ? <Badge variant="outline">Codex 生效中</Badge> : null}
            {profile.chatgpt_plan_type ? (
              <Badge variant="outline">{profile.chatgpt_plan_type}</Badge>
            ) : null}
          </div>
          <div className="mt-2 text-sm text-muted-foreground">{profile.email}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            更新时间：{formatTimestamp(profile.updated_at_epoch)}
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={onActivate} disabled={busyAction !== null || profile.active}>
            当前使用中
          </Button>
          <Button variant="outline" size="sm" onClick={onRefresh} disabled={busyAction !== null}>
            {busyAction === "codex-refresh" ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <RefreshCw className="size-4" />
            )}
            刷新令牌
          </Button>
          <Button variant="ghost" size="sm" onClick={onRemove} disabled={busyAction !== null}>
            <Trash2 className="size-4" />
            移除
          </Button>
        </div>
      </div>
    </div>
  );
}

function ManagedAuthAccountCard({
  account,
  busyAction,
  onActivate,
  onRefresh,
  onRemove,
}: {
  account: DesktopManagedAuthAccount;
  busyAction: BusyAction;
  onActivate: () => void;
  onRefresh: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="rounded-2xl border border-border bg-background p-4">
      <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <UserRound className="size-4 text-muted-foreground" />
            <div className="font-medium text-foreground">{account.display_label}</div>
            {account.is_default ? <Badge variant="default">当前账号</Badge> : null}
            {account.applied_to_runtime ? <Badge variant="outline">运行中</Badge> : null}
            {account.plan_label ? <Badge variant="outline">{account.plan_label}</Badge> : null}
            <Badge variant="outline">{formatManagedAuthStatus(account.status)}</Badge>
          </div>
          <div className="mt-2 text-sm text-muted-foreground">{account.email ?? "未提供邮箱"}</div>
          <div className="mt-1 text-xs text-muted-foreground">
            更新时间：{formatTimestamp(account.updated_at_epoch)}
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={onActivate} disabled={busyAction !== null || account.is_default}>
            当前使用中
          </Button>
          <Button variant="outline" size="sm" onClick={onRefresh} disabled={busyAction !== null}>
            {busyAction === "qwen-refresh" ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <RefreshCw className="size-4" />
            )}
            刷新令牌
          </Button>
          <Button variant="ghost" size="sm" onClick={onRemove} disabled={busyAction !== null}>
            <Trash2 className="size-4" />
            移除
          </Button>
        </div>
      </div>
    </div>
  );
}

function EmptyBlock({ label }: { label: string }) {
  return (
    <div className="rounded-2xl border border-dashed border-border bg-muted/20 p-6 text-sm text-muted-foreground">
      {label}
    </div>
  );
}

function resolveCurrentCodexProfile(overview: DesktopCodexAuthOverview | null) {
  if (!overview) {
    return null;
  }
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

function formatManagedAuthStatus(status: string) {
  switch (status) {
    case "ready":
      return "可用";
    case "expiring":
      return "即将过期";
    case "expired":
      return "已过期";
    case "needs_reauth":
      return "需要重新授权";
    default:
      return status;
  }
}

function compactModelList(models: string[]) {
  if (models.length === 0) {
    return "未提供";
  }
  if (models.length <= 4) {
    return models.join(", ");
  }
  return `${models.slice(0, 4).join(", ")} 等 ${models.length} 个`;
}

function formatTimestamp(epochSeconds: number | null | undefined) {
  if (!epochSeconds) {
    return "--";
  }
  return new Date(epochSeconds * 1000).toLocaleString("zh-CN");
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return null;
}

/**
 * WeChat Bridge · 个微 iLink 漏斗
 *
 * S5 real implementation. This page does NOT configure an
 * enterprise-WeChat outbound bot + cloud ingest microservice.
 * Instead it surfaces the existing personal-WeChat iLink pipeline
 * that earlier implementation phases already wired:
 *
 *   WeChat user (phone)
 *     → ClawBot plugin → ilinkai.weixin.qq.com
 *     → desktop-core::wechat_ilink::monitor (long-poll)
 *     → DesktopAgentHandler::on_message
 *       ├── wiki_store::write_raw_entry  (S5.1 — NEW)
 *       ├── append_inbox_pending          (S5.1 — NEW)
 *       └── DesktopState::append_user_message (Phase 2b, reply)
 *
 * This page manages the **left edge** of that pipeline: QR login,
 * account list, and per-account delete. The heavy lifting lives in
 * Rust; the page is a thin React Query dashboard over the 5 existing
 * wechat HTTP routes preserved in S0.4.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Loader2,
  Link2,
  Plus,
  QrCode,
  RefreshCw,
  Trash2,
  Wifi,
  WifiOff,
  AlertTriangle,
  CheckCircle2,
  Circle,
  XCircle,
  MinusCircle,
  Rocket,
} from "lucide-react";
import {
  listWeChatAccounts,
  startWeChatLogin,
  getWeChatLoginStatus,
  cancelWeChatLogin,
  deleteWeChatAccount,
  loadKefuConfig,
  saveKefuConfig,
  createKefuAccount,
  getKefuContactUrl,
  getKefuStatus,
  startKefuMonitor,
  stopKefuMonitor,
  startKefuPipeline,
  getKefuPipelineStatus,
  cancelKefuPipeline,
  type WeChatAccountSummary,
  type WeChatAccountStatus,
  type WeChatLoginStartResponse,
  type WeChatLoginStatusResponse,
} from "@/features/settings/api/client";

const wechatKeys = {
  accounts: () => ["wechat", "accounts"] as const,
};

const TERMINAL_LOGIN_STATUSES = [
  "confirmed",
  "failed",
  "cancelled",
  "expired",
] as const;

/**
 * Review nit #13: single source of truth for the terminal-status
 * check so the cast doesn't get re-written at every call site.
 */
function isTerminalLoginStatus(status: string): boolean {
  return (TERMINAL_LOGIN_STATUSES as readonly string[]).includes(status);
}

/**
 * Review nit #16: make sure the QR image src is a `data:` URL before
 * we drop it into an `<img>` tag. An `<img>` can't execute JavaScript
 * from its `src`, but a compromised backend could return a `data:`
 * URL with a `text/html` content-type that some UAs try to sniff as
 * HTML; rejecting anything that doesn't start with `data:image/`
 * removes the edge case.
 */
function isSafeQrDataUrl(src: string): boolean {
  return src.startsWith("data:image/");
}

export function WeChatBridgePage() {
  const queryClient = useQueryClient();

  const accountsQuery = useQuery({
    queryKey: wechatKeys.accounts(),
    queryFn: () => listWeChatAccounts(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });

  const [loginHandle, setLoginHandle] = useState<WeChatLoginStartResponse | null>(
    null,
  );
  const [loginStatus, setLoginStatus] = useState<WeChatLoginStatusResponse | null>(
    null,
  );
  const loginPollRef = useRef<number | null>(null);
  const mountedRef = useRef(true);

  // When login reaches a terminal state, stop polling and refresh list.
  useEffect(() => {
    if (!loginStatus) return;
    if (isTerminalLoginStatus(loginStatus.status)) {
      if (loginPollRef.current !== null) {
        window.clearInterval(loginPollRef.current);
        loginPollRef.current = null;
      }
      if (loginStatus.status === "confirmed") {
        void queryClient.invalidateQueries({ queryKey: wechatKeys.accounts() });
      }
    }
  }, [loginStatus, queryClient]);

  // Clean up the interval on unmount + set mounted flag.
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (loginPollRef.current !== null) {
        window.clearInterval(loginPollRef.current);
      }
    };
  }, []);

  const startLoginMutation = useMutation({
    mutationFn: () => startWeChatLogin(),
    onSuccess: (data) => {
      setLoginHandle(data);
      setLoginStatus({ status: "waiting" });
      if (loginPollRef.current !== null) {
        window.clearInterval(loginPollRef.current);
      }
      loginPollRef.current = window.setInterval(() => {
        void (async () => {
          try {
            const status = await getWeChatLoginStatus(data.handle);
            if (mountedRef.current) setLoginStatus(status);
          } catch (err) {
            console.error("[wechat] login poll failed", err);
          }
        })();
      }, 1500);
    },
  });

  const cancelLoginMutation = useMutation({
    mutationFn: (handle: string) => cancelWeChatLogin(handle),
    onSuccess: () => {
      setLoginHandle(null);
      setLoginStatus(null);
      if (loginPollRef.current !== null) {
        window.clearInterval(loginPollRef.current);
        loginPollRef.current = null;
      }
    },
  });

  const deleteAccountMutation = useMutation({
    mutationFn: (id: string) => deleteWeChatAccount(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: wechatKeys.accounts() });
    },
  });

  const accounts = accountsQuery.data?.accounts ?? [];
  const hasLoginInFlight =
    loginHandle !== null &&
    loginStatus !== null &&
    !isTerminalLoginStatus(loginStatus.status);

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      {/* Hero */}
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <h1
          className="text-foreground"
          style={{ fontSize: 18, fontWeight: 600, fontFamily: "var(--font-serif, Lora, serif)" }}
        >
          WeChat Bridge
        </h1>
        <p className="mt-1 text-muted-foreground/60" style={{ fontSize: 11 }}>
          个微 iLink 登录 -- 长轮询监听 -- 文本消息自动入{" "}
          <code>~/.clawwiki/raw/</code>
        </p>
      </div>

      {/* Connected accounts */}
      <section className="border-b border-border/50 px-6 py-5">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
            Connected accounts
          </h2>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() =>
                void queryClient.invalidateQueries({
                  queryKey: wechatKeys.accounts(),
                })
              }
              disabled={accountsQuery.isFetching}
              className="flex items-center gap-1 rounded-md border border-border bg-background px-2 py-1 text-caption text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
              title="Refresh list"
            >
              <RefreshCw
                className={
                  "size-3 " + (accountsQuery.isFetching ? "animate-spin" : "")
                }
              />
              Refresh
            </button>
            <button
              type="button"
              onClick={() => startLoginMutation.mutate()}
              disabled={startLoginMutation.isPending || hasLoginInFlight}
              className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-body-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
            >
              {startLoginMutation.isPending ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Plus className="size-3" />
              )}
              Add account
            </button>
          </div>
        </div>

        <AccountList
          accounts={accounts}
          isLoading={accountsQuery.isLoading}
          error={accountsQuery.error}
          onDelete={(id) => {
            if (
              window.confirm(
                `Delete WeChat account ${id}? The persisted bot token and cursor will be removed.`,
              )
            ) {
              deleteAccountMutation.mutate(id);
            }
          }}
          deleteInFlightId={
            deleteAccountMutation.isPending
              ? (deleteAccountMutation.variables ?? null)
              : null
          }
        />

        {startLoginMutation.error && (
          <div
            className="mt-3 rounded-md border px-3 py-2 text-caption"
            style={{
              borderColor:
                "color-mix(in srgb, var(--color-error) 30%, transparent)",
              backgroundColor:
                "color-mix(in srgb, var(--color-error) 5%, transparent)",
              color: "var(--color-error)",
            }}
          >
            Failed to start login: {String(startLoginMutation.error)}
          </div>
        )}
      </section>

      {/* QR login flow card */}
      {loginHandle && loginStatus && (
        <section className="border-b border-border/50 px-6 py-5">
          <h2 className="mb-3 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
            QR login
          </h2>
          <QrLoginCard
            handle={loginHandle}
            status={loginStatus}
            onCancel={() => cancelLoginMutation.mutate(loginHandle.handle)}
            isCancelling={cancelLoginMutation.isPending}
          />
        </section>
      )}

      {/* ── Pipeline: One-scan kefu setup ──────────────────────── */}
      <KefuPipelineSection />

      {/* ── Channel B: Official WeChat Customer Service ─────────── */}
      <KefuSection />

      {/* Pipeline info */}
      <section className="px-6 py-5">
        <h2 className="mb-3 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
          Pipeline (D2 override)
        </h2>
        <div className="rounded-md border border-border/40 p-4">
          <div className="mb-2 text-foreground" style={{ fontSize: 14, fontWeight: 500 }}>
            Personal WeChat → Raw → Inbox → Ask
          </div>
          <ol className="ml-5 list-decimal space-y-1.5 text-muted-foreground/70" style={{ fontSize: 13, lineHeight: 1.6 }}>
            <li>
              Add an account above — scan the QR code in the WeChat ClawBot
              plugin.
            </li>
            <li>
              Text messages to the bot are written to{" "}
              <code>~/.clawwiki/raw/NNNNN_wechat-text_*.md</code> with schema-v1
              frontmatter.
            </li>
            <li>
              Each new raw entry auto-queues a pending task in the{" "}
              <a href="#/inbox" className="text-primary hover:underline">
                Maintenance Inbox
              </a>
              .
            </li>
            <li>
              The original reply path still runs, so users also get an immediate
              chat response from the existing DesktopState agent.
            </li>
          </ol>
          <div className="mt-3 text-muted-foreground/40" style={{ fontSize: 11 }}>
            S6 will layer adapters on top (voice → whisper, image → Vision
            caption, PPT → slides-per-section). S5 only ships the text path.
          </div>
        </div>
      </section>
    </div>
  );
}

/* ─── Account list ─────────────────────────────────────────────── */

function AccountList({
  accounts,
  isLoading,
  error,
  onDelete,
  deleteInFlightId,
}: {
  accounts: WeChatAccountSummary[];
  isLoading: boolean;
  error: Error | null;
  onDelete: (id: string) => void;
  deleteInFlightId: string | null;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center gap-2 text-caption text-muted-foreground">
        <Loader2 className="size-3 animate-spin" />
        Loading accounts…
      </div>
    );
  }
  if (error) {
    return (
      <div
        className="rounded-md border px-3 py-2 text-caption"
        style={{
          borderColor:
            "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor:
            "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        Failed to list WeChat accounts: {error.message}
      </div>
    );
  }
  if (accounts.length === 0) {
    return (
      <div className="rounded-md border border-dashed border-border/40 px-4 py-6 text-center text-muted-foreground/60" style={{ fontSize: 12 }}>
        <Link2 className="mx-auto mb-2 size-5 opacity-30" />
        No WeChat accounts connected yet. Click <b>Add account</b> to begin.
      </div>
    );
  }

  return (
    <ul className="divide-y divide-border/30 overflow-hidden rounded-md border border-border/40">
      {accounts.map((account) => (
        <li key={account.id} className="flex items-center gap-3 px-4 py-3">
          <StatusIndicator status={account.status} />
          <div className="min-w-0 flex-1">
            <div className="flex items-baseline gap-2">
              <span className="truncate text-foreground" style={{ fontSize: 14 }}>
                {account.display_name || account.id}
              </span>
              <span className="shrink-0 font-mono text-muted-foreground/40" style={{ fontSize: 11 }}>
                {account.id}
              </span>
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-muted-foreground/40" style={{ fontSize: 11 }}>
              <span className="font-mono">
                token: {account.bot_token_preview}
              </span>
              {account.last_active_at && (
                <span>last active: {account.last_active_at}</span>
              )}
              <span className="truncate">{account.base_url}</span>
            </div>
          </div>
          <button
            type="button"
            onClick={() => onDelete(account.id)}
            disabled={deleteInFlightId === account.id}
            className="flex shrink-0 items-center gap-1 rounded-md border border-border bg-background px-2 py-1 text-caption text-muted-foreground transition-colors hover:border-destructive hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
          >
            {deleteInFlightId === account.id ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Trash2 className="size-3" />
            )}
            Delete
          </button>
        </li>
      ))}
    </ul>
  );
}

function StatusIndicator({ status }: { status: WeChatAccountStatus }) {
  if (status === "connected") {
    return (
      <Wifi
        className="size-4 shrink-0"
        style={{ color: "var(--color-success)" }}
      />
    );
  }
  if (status === "disconnected") {
    return (
      <WifiOff
        className="size-4 shrink-0"
        style={{ color: "var(--muted-foreground)" }}
      />
    );
  }
  return (
    <AlertTriangle
      className="size-4 shrink-0"
      style={{ color: "var(--color-warning)" }}
    />
  );
}

/* ─── QR login card ─────────────────────────────────────────────── */

function QrLoginCard({
  handle,
  status,
  onCancel,
  isCancelling,
}: {
  handle: WeChatLoginStartResponse;
  status: WeChatLoginStatusResponse;
  onCancel: () => void;
  isCancelling: boolean;
}) {
  const helpText = useMemo(() => {
    switch (status.status) {
      case "waiting":
        return "打开 WeChat ClawBot 插件，扫描右侧二维码";
      case "scanned":
        return "扫码成功，请在手机上点击确认";
      case "confirmed":
        return "✓ 登录成功，账号已加入监听列表";
      case "cancelled":
        return "✗ 登录已取消";
      case "expired":
        return "⏳ 二维码已过期，请重新开始";
      case "failed":
        return `✗ 登录失败${status.error ? ": " + status.error : ""}`;
      default:
        return "";
    }
  }, [status]);

  const isTerminal = isTerminalLoginStatus(status.status);

  // Review nit #16: refuse to render the QR code when the backend
  // returns something that's not a data:image/* URL. This guard
  // matches the contract on the Rust side (always base64-encoded
  // PNG) and fails safely if that contract is ever broken.
  const safeQrSrc = isSafeQrDataUrl(handle.qr_image_base64)
    ? handle.qr_image_base64
    : null;

  return (
    <div className="rounded-md border border-border/40 p-5">
      <div className="flex items-start gap-5">
        {/* QR image */}
        <div className="shrink-0">
          <div className="relative rounded-md border border-border/40 bg-background p-2">
            {safeQrSrc ? (
              <img
                src={safeQrSrc}
                alt="WeChat ClawBot QR code"
                width={192}
                height={192}
                className="size-[192px] rounded"
              />
            ) : (
              <div
                className="flex size-[192px] items-center justify-center rounded text-center text-caption"
                style={{ color: "var(--color-error)" }}
              >
                QR payload is not a data:image/ URL — refusing to render
              </div>
            )}
            {status.status === "confirmed" && (
              <div className="absolute inset-0 flex items-center justify-center rounded-md bg-background/90">
                <CheckCircle2
                  className="size-12"
                  style={{ color: "var(--color-success)" }}
                />
              </div>
            )}
          </div>
          <div className="mt-2 text-center text-muted-foreground/40" style={{ fontSize: 11 }}>
            expires {handle.expires_at}
          </div>
        </div>

        {/* Status */}
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <QrCode
              className="size-3.5"
              style={{ color: "var(--claude-orange)" }}
            />
            <span className="text-foreground" style={{ fontSize: 14, fontWeight: 500 }}>
              {statusLabel(status.status)}
            </span>
          </div>
          <p className="mt-1.5 text-foreground/80" style={{ fontSize: 14, lineHeight: 1.6 }}>{helpText}</p>
          {status.account_id && (
            <p className="mt-1.5 font-mono text-muted-foreground/40" style={{ fontSize: 11 }}>
              account_id: {status.account_id}
            </p>
          )}
          <div className="mt-4 flex items-center gap-2">
            {!isTerminal && (
              <button
                type="button"
                onClick={onCancel}
                disabled={isCancelling}
                className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-caption text-muted-foreground transition-colors hover:border-destructive hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
              >
                {isCancelling ? <Loader2 className="size-3 animate-spin" /> : null}
                Cancel login
              </button>
            )}
            {status.status === "confirmed" && (
              <p className="text-caption text-muted-foreground">
                The monitor is now running — send a message from WeChat to
                test the pipeline.
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function statusLabel(status: string): string {
  switch (status) {
    case "waiting":
      return "Waiting for scan…";
    case "scanned":
      return "Scanned — confirm on phone";
    case "confirmed":
      return "Connected";
    case "cancelled":
      return "Cancelled";
    case "expired":
      return "Expired";
    case "failed":
      return "Failed";
    default:
      return status;
  }
}

/* ─── Channel B: Official WeChat Customer Service ──────────────── */

const kefuKeys = {
  config: () => ["kefu", "config"] as const,
  status: () => ["kefu", "status"] as const,
};

function KefuSection() {
  const queryClient = useQueryClient();
  const [showConfig, setShowConfig] = useState(false);

  const configQuery = useQuery({
    queryKey: kefuKeys.config(),
    queryFn: () => loadKefuConfig(),
    staleTime: 10_000,
  });

  const statusQuery = useQuery({
    queryKey: kefuKeys.status(),
    queryFn: () => getKefuStatus(),
    staleTime: 5_000,
    refetchInterval: 10_000,
  });

  const createMutation = useMutation({
    mutationFn: () => createKefuAccount("ClaudeWiki助手"),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: kefuKeys.config() });
      void queryClient.invalidateQueries({ queryKey: kefuKeys.status() });
    },
  });

  const startMutation = useMutation({
    mutationFn: () => startKefuMonitor(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: kefuKeys.status() });
    },
  });

  const stopMutation = useMutation({
    mutationFn: () => stopKefuMonitor(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: kefuKeys.status() });
    },
  });

  const config = configQuery.data;
  const status = statusQuery.data;
  const configured = config?.corpid && config.corpid.length > 0 && config.configured !== false;
  const accountCreated = !!config?.open_kfid;

  return (
    <section className="border-b border-border/50 px-6 py-5">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-baseline gap-2">
          <h2 className="text-subhead font-semibold text-foreground">
            ClaudeWiki助手 · 微信客服
          </h2>
          <span className="rounded-full bg-primary/10 px-2 py-0.5 text-caption font-medium text-primary">
            Channel B
          </span>
        </div>
        <button
          type="button"
          onClick={() => setShowConfig(!showConfig)}
          className="text-caption text-primary hover:underline"
        >
          {showConfig ? "隐藏配置" : "配置"}
        </button>
      </div>

      {/* Config form */}
      {showConfig && <KefuConfigForm onSaved={() => {
        void queryClient.invalidateQueries({ queryKey: kefuKeys.config() });
        setShowConfig(false);
      }} />}

      {/* Status cards */}
      {configured ? (
        <div className="space-y-3">
          {/* Account status */}
          <div className="rounded-md border border-border bg-muted/5 p-4">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-body-sm font-medium text-foreground">
                  {accountCreated ? (
                    <>
                      <CheckCircle2
                        className="mr-1 inline size-4"
                        style={{ color: "var(--color-success)" }}
                      />
                      {config?.account_name ?? "ClaudeWiki助手"}
                    </>
                  ) : (
                    "未创建客服账号"
                  )}
                </div>
                {config?.open_kfid && (
                  <div className="mt-0.5 font-mono text-caption text-muted-foreground">
                    open_kfid: {config.open_kfid}
                  </div>
                )}
                <div className="mt-0.5 text-caption text-muted-foreground">
                  corpid: {config?.corpid} · secret: {config?.secret_preview}
                </div>
              </div>
              {!accountCreated && (
                <button
                  type="button"
                  onClick={() => createMutation.mutate()}
                  disabled={createMutation.isPending}
                  className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-body-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  {createMutation.isPending ? (
                    <Loader2 className="size-3 animate-spin" />
                  ) : (
                    <Plus className="size-3" />
                  )}
                  创建客服账号
                </button>
              )}
            </div>
            {createMutation.error && (
              <div className="mt-2 text-caption" style={{ color: "var(--color-error)" }}>
                {String(createMutation.error)}
              </div>
            )}
          </div>

          {/* Contact URL QR */}
          {accountCreated && <KefuContactQR />}

          {/* Monitor status */}
          {accountCreated && status && (
            <div className="rounded-md border border-border bg-muted/5 p-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {status.monitor_running ? (
                    <Wifi className="size-4" style={{ color: "var(--color-success)" }} />
                  ) : (
                    <WifiOff className="size-4 text-muted-foreground" />
                  )}
                  <span className="text-body-sm font-medium text-foreground">
                    Monitor {status.monitor_running ? "运行中" : "已停止"}
                  </span>
                </div>
                <div className="flex gap-2">
                  {!status.monitor_running ? (
                    <button
                      type="button"
                      onClick={() => startMutation.mutate()}
                      disabled={startMutation.isPending}
                      className="rounded-md bg-primary px-3 py-1 text-caption font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                    >
                      启动
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => stopMutation.mutate()}
                      disabled={stopMutation.isPending}
                      className="rounded-md border border-border px-3 py-1 text-caption text-muted-foreground hover:border-destructive hover:text-destructive disabled:opacity-50"
                    >
                      停止
                    </button>
                  )}
                </div>
              </div>
              <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-caption text-muted-foreground">
                {status.last_poll_unix_ms && (
                  <span>拉取: {new Date(status.last_poll_unix_ms).toLocaleTimeString()}</span>
                )}
                {status.last_inbound_unix_ms && (
                  <span>消息: {new Date(status.last_inbound_unix_ms).toLocaleTimeString()}</span>
                )}
                {status.consecutive_failures > 0 && (
                  <span style={{ color: "var(--color-warning)" }}>
                    失败: {status.consecutive_failures}
                  </span>
                )}
                {status.last_error && (
                  <span style={{ color: "var(--color-error)" }}>
                    {status.last_error}
                  </span>
                )}
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="rounded-md border border-dashed border-border/50 bg-muted/10 px-4 py-6 text-center text-caption text-muted-foreground">
          <Link2 className="mx-auto mb-1.5 size-5 opacity-40" />
          点击 "配置" 填入企业微信 corpid 和客服 secret，开始接入微信客服。
        </div>
      )}
    </section>
  );
}

function KefuConfigForm({ onSaved }: { onSaved: () => void }) {
  const [corpid, setCorpid] = useState("");
  const [secret, setSecret] = useState("");
  const [token, setToken] = useState("");
  const [aesKey, setAesKey] = useState("");

  const saveMutation = useMutation({
    mutationFn: () =>
      saveKefuConfig({
        corpid,
        secret,
        token,
        encoding_aes_key: aesKey,
      }),
    onSuccess: () => onSaved(),
  });

  return (
    <div className="mb-4 rounded-md border border-border bg-muted/5 p-4">
      <div className="space-y-3">
        <div>
          <label className="mb-1 block text-caption font-medium text-foreground">
            企业 ID (corpid)
          </label>
          <input
            type="text"
            value={corpid}
            onChange={(e) => setCorpid(e.target.value)}
            placeholder="ww1234567890abcd"
            className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-body-sm"
          />
        </div>
        <div>
          <label className="mb-1 block text-caption font-medium text-foreground">
            客服 Secret
          </label>
          <input
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            placeholder="微信客服应用的 secret"
            className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-body-sm"
          />
        </div>
        <div>
          <label className="mb-1 block text-caption font-medium text-foreground">
            Token (回调验证)
          </label>
          <input
            type="text"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="从 kf.weixin.qq.com 获取"
            className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-body-sm"
          />
        </div>
        <div>
          <label className="mb-1 block text-caption font-medium text-foreground">
            EncodingAESKey (43 位)
          </label>
          <input
            type="text"
            value={aesKey}
            onChange={(e) => setAesKey(e.target.value)}
            placeholder="从 kf.weixin.qq.com 获取"
            className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-body-sm"
          />
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => saveMutation.mutate()}
            disabled={saveMutation.isPending || !corpid || !secret || !token || !aesKey}
            className="rounded-md bg-primary px-4 py-1.5 text-body-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {saveMutation.isPending ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              "保存配置"
            )}
          </button>
          {saveMutation.error && (
            <span className="text-caption" style={{ color: "var(--color-error)" }}>
              {String(saveMutation.error)}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

function KefuContactQR() {
  const contactQuery = useQuery({
    queryKey: ["kefu", "contact-url"],
    queryFn: () => getKefuContactUrl(),
    staleTime: 60_000,
    retry: false,
  });

  const url = contactQuery.data?.url;

  return (
    <div className="rounded-md border border-border bg-muted/5 p-4">
      <div className="flex items-start gap-4">
        {url ? (
          <>
            <div className="shrink-0 rounded-md border border-border bg-background p-2">
              <img
                src={`https://api.qrserver.com/v1/create-qr-code/?size=160x160&data=${encodeURIComponent(url)}`}
                alt="客服二维码"
                width={160}
                height={160}
                className="size-[160px] rounded"
              />
            </div>
            <div className="flex-1">
              <div className="flex items-center gap-2">
                <QrCode className="size-4" style={{ color: "var(--claude-orange)" }} />
                <span className="text-body-sm font-semibold text-foreground">
                  扫码接入 ClaudeWiki助手
                </span>
              </div>
              <p className="mt-1 text-body text-foreground/80">
                微信扫描二维码开始对话。扫码后可在微信 "转发给朋友 → 客服消息" 中看到 ClaudeWiki助手。
              </p>
              <div className="mt-2 flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => navigator.clipboard.writeText(url)}
                  className="rounded-md border border-border px-2 py-1 text-caption text-muted-foreground hover:bg-accent hover:text-foreground"
                >
                  复制链接
                </button>
              </div>
              <p className="mt-2 truncate font-mono text-caption text-muted-foreground">
                {url}
              </p>
            </div>
          </>
        ) : contactQuery.isLoading ? (
          <div className="flex items-center gap-2 text-caption text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            生成客服链接...
          </div>
        ) : (
          <div className="text-caption text-muted-foreground">
            {contactQuery.error
              ? `获取客服链接失败: ${String(contactQuery.error)}`
              : "客服链接未生成"}
          </div>
        )}
      </div>
    </div>
  );
}

/* ─── Pipeline: One-scan kefu setup ──────────────────────────── */

const PHASE_LABELS: Record<string, string> = {
  cf_register: "Cloudflare 账号注册",
  worker_deploy: "中继服务器部署",
  wecom_auth: "企业微信授权",
  callback_config: "回调 URL 配置",
  kefu_create: "客服账号创建",
};

function PhaseIcon({ status }: { status: string }) {
  switch (status) {
    case "done":
      return <CheckCircle2 className="size-4" style={{ color: "var(--color-success)" }} />;
    case "running":
      return <Loader2 className="size-4 animate-spin text-primary" />;
    case "waiting_scan":
      return <QrCode className="size-4" style={{ color: "var(--claude-orange)" }} />;
    case "failed":
      return <XCircle className="size-4" style={{ color: "var(--color-error)" }} />;
    case "skipped":
      return <MinusCircle className="size-4 text-muted-foreground" />;
    default:
      return <Circle className="size-4 text-muted-foreground/40" />;
  }
}

function KefuPipelineSection() {
  const queryClient = useQueryClient();
  const [skipCf, setSkipCf] = useState(false);
  const [cfToken, setCfToken] = useState("");
  const [kickoffPolling, setKickoffPolling] = useState(false);

  const pipelineQuery = useQuery({
    queryKey: ["kefu", "pipeline"],
    queryFn: () => getKefuPipelineStatus(),
    staleTime: 1000,
    refetchInterval: 2000,
    refetchIntervalInBackground: true,
    refetchOnMount: "always",
  });

  useEffect(() => {
    if (!kickoffPolling) return;
    const pipeline = pipelineQuery.data;
    if (!pipeline) return;
    const phases = pipeline.phases ?? [];
    const logs = pipeline.logs ?? [];
    const hasMoved =
      logs.length > 0 ||
      Boolean(pipeline.active) ||
      Boolean(pipeline.finished_at) ||
      phases.some((phase) => phase.status !== "pending");
    if (hasMoved) {
      setKickoffPolling(false);
    }
  }, [kickoffPolling, pipelineQuery.data]);

  const startMutation = useMutation({
    mutationFn: () =>
      startKefuPipeline({
        skip_cf_register: skipCf,
        cf_api_token: skipCf ? cfToken : undefined,
      }),
    onMutate: () => {
      setKickoffPolling(true);
      void pipelineQuery.refetch();
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["kefu", "pipeline"] });
      await pipelineQuery.refetch();
    },
    onError: () => {
      setKickoffPolling(false);
    },
  });

  const cancelMutation = useMutation({
    mutationFn: () => cancelKefuPipeline(),
  });

  const pipeline = pipelineQuery.data;
  const isActive = pipeline?.active ?? false;
  const phases = pipeline?.phases ?? [];
  const logs = pipeline?.logs ?? [];
  const hasStarted =
    kickoffPolling ||
    Boolean(pipeline?.started_at) ||
    phases.some((phase) => phase.status !== "pending");
  const visibleLogs =
    logs.length > 0
      ? logs
      : kickoffPolling
        ? ["准备启动新的微信客服接入流程...", "等待桌面后端返回最新阶段状态..."]
        : [];

  return (
    <section className="border-b border-border/50 px-6 py-5">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-baseline gap-2">
          <h2 className="text-subhead font-semibold text-foreground">
            一键接入微信客服
          </h2>
          <span className="rounded-full bg-primary/10 px-2 py-0.5 text-caption font-medium text-primary">
            Pipeline
          </span>
        </div>
      </div>

      {/* Start controls */}
      {!isActive && !pipeline?.contact_url && (
        <div className="space-y-3">
          <p className="text-caption text-muted-foreground">
            自动注册 Cloudflare 中继 → 企业微信扫码授权 → 配置回调 → 创建客服账号。全程仅需扫码一次。
          </p>
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-1.5 text-caption text-muted-foreground">
              <input
                type="checkbox"
                checked={skipCf}
                onChange={(e) => setSkipCf(e.target.checked)}
                className="size-3.5"
              />
              已有 Cloudflare 账号
            </label>
            {skipCf && (
              <input
                type="text"
                value={cfToken}
                onChange={(e) => setCfToken(e.target.value)}
                placeholder="Cloudflare API Token"
                className="rounded-md border border-border bg-background px-2 py-1 text-caption"
              />
            )}
          </div>
          <button
            type="button"
            onClick={() => startMutation.mutate()}
            disabled={startMutation.isPending || (skipCf && !cfToken)}
            className="flex items-center gap-1.5 rounded-md bg-primary px-4 py-2 text-body-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {startMutation.isPending ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Rocket className="size-4" />
            )}
            {hasStarted ? "重新开始" : "一键接入"}
          </button>
          {startMutation.error && (
            <div className="text-caption" style={{ color: "var(--color-error)" }}>
              {String(startMutation.error)}
            </div>
          )}
        </div>
      )}

      {/* Phase progress */}
      {phases.length > 0 && (
        <div className="space-y-2">
          {phases.map((p) => (
            <div key={p.phase} className="flex items-center gap-3">
              <PhaseIcon status={p.status} />
              <span
                className={`text-body-sm ${
                  p.status === "done" || p.status === "running" || p.status === "waiting_scan"
                    ? "text-foreground"
                    : "text-muted-foreground"
                }`}
              >
                {PHASE_LABELS[p.phase] || p.phase}
              </span>
              {p.status === "skipped" && (
                <span className="text-caption italic text-muted-foreground">已跳过</span>
              )}
              {p.message && (
                <span className="truncate text-caption text-muted-foreground">
                  {p.message}
                </span>
              )}
              {p.error && (
                <span className="truncate text-caption" style={{ color: "var(--color-error)" }}>
                  {p.error}
                </span>
              )}
            </div>
          ))}

          {/* Cancel button */}
          {isActive && (
            <button
              type="button"
              onClick={() => cancelMutation.mutate()}
              disabled={cancelMutation.isPending}
              className="mt-2 rounded-md border border-border px-3 py-1 text-caption text-muted-foreground hover:border-destructive hover:text-destructive"
            >
              取消
            </button>
          )}

          {/* Contact URL on completion */}
          {pipeline?.contact_url && (
            <div className="mt-3 rounded-md border border-border bg-muted/5 p-3">
              <div className="flex items-center gap-2 text-body-sm font-medium text-foreground">
                <CheckCircle2 className="size-4" style={{ color: "var(--color-success)" }} />
                接入完成!
              </div>
              <p className="mt-1 font-mono text-caption text-muted-foreground">
                {pipeline.contact_url}
              </p>
            </div>
          )}

          <div className="mt-3 rounded-md border border-border bg-muted/5 p-3">
            <div className="mb-2 text-caption font-medium text-foreground">执行日志</div>
            <div className="max-h-56 overflow-auto rounded-md bg-background px-3 py-2">
              {visibleLogs.length > 0 ? (
                <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-5 text-muted-foreground">
                  {visibleLogs.join("\n")}
                </pre>
              ) : (
                <div className="text-caption text-muted-foreground">
                  暂无日志
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

/**
 * WeChat Bridge · 个微 iLink 漏斗 (wireframes.html §02, D2 override)
 *
 * S5 real implementation. Per the user's D2 override (`docs/clawwiki/
 * product-design.md` §7.1 diagram + commit 6617945), this page does
 * NOT configure an enterprise-WeChat outbound bot + cloud ingest
 * microservice. Instead it surfaces the EXISTING personal-WeChat
 * iLink pipeline that Phase 1-2 of (8) already wired:
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
} from "lucide-react";
import {
  listWeChatAccounts,
  startWeChatLogin,
  getWeChatLoginStatus,
  cancelWeChatLogin,
  deleteWeChatAccount,
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

/**
 * WeChat Bridge - dual WeChat ingress dashboard.
 *
 * Channel A is the personal-WeChat iLink funnel:
 *   phone WeChat -> ClawBot plugin -> ilinkai.weixin.qq.com
 *     -> desktop-core::wechat_ilink::monitor -> DesktopAgentHandler
 *
 * Channel B is the official Kefu funnel:
 *   WeChat customer-service callback/contact link
 *     -> /api/desktop/wechat-kefu/* -> desktop-core::wechat_kefu
 *
 * This top-level #/wechat page is currently the operational surface for
 * both channels. The Settings Modal WeChat section still manages only the
 * iLink account binding flow.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Loader2,
  Link2,
  Plus,
  QrCode,
  RefreshCw,
  Settings,
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
  type KefuCapabilities,
  type WeChatAccountSummary,
  type WeChatAccountStatus,
  type WeChatLoginStartResponse,
  type WeChatLoginStatusResponse,
} from "@/features/settings/api/client";
import { formatIngestError } from "@/lib/ingest/format-error";
import { EnvironmentDoctor } from "@/components/EnvironmentDoctor";
import { RecentIngestCard } from "@/components/RecentIngestCard";
import { EmptyState } from "@/components/ui/empty-state";
import { FailureBanner } from "@/components/ui/failure-banner";
import { BridgeHealthHeader } from "@/features/wechat/components/BridgeHealthHeader";
import { StepRow } from "@/components/ds/StepRow";
import { GroupScopeModal } from "@/features/wechat/components/GroupScopeModal";
import {
  fetchWeChatBridgeHealth,
  type BridgeHealthResponse,
  type WeChatIngestConfig,
} from "@/features/wechat/health-state";

const wechatKeys = {
  accounts: () => ["wechat", "accounts"] as const,
  bridgeHealth: () => ["wechat", "bridge-health"] as const,
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

/**
 * Thin wrapper around the shared `formatIngestError` util. Kept in place
 * (rather than deleting and inlining `formatIngestError` everywhere) so
 * the existing call sites read the same, and so future WeChat-specific
 * pre-processing can be slotted here without changing the ingest util's
 * contract. See `src/lib/ingest/format-error.ts` for the classification
 * catalogue.
 */
function formatWeChatBridgeErrorMessage(
  message: string | null | undefined
): string {
  return formatIngestError(message);
}

export function WeChatBridgePage() {
  const queryClient = useQueryClient();

  const accountsQuery = useQuery({
    queryKey: wechatKeys.accounts(),
    queryFn: () => listWeChatAccounts(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });

  /**
   * M5 — dual-channel health + ingest-scope snapshot. 30s polling is
   * Explorer C UX gate #2 (polling more often than 30s is wasteful and
   * makes the "N 分钟前" labels flicker). Read-only refetching is kept on
   * for the whole page so ops can watch recovery live.
   *
   * TODO(worker-a): swap the stub import for the real tauri.ts wrapper.
   */
  const bridgeHealthQuery = useQuery<BridgeHealthResponse>({
    queryKey: wechatKeys.bridgeHealth(),
    queryFn: () => fetchWeChatBridgeHealth(),
    staleTime: 15_000,
    refetchInterval: 30_000,
  });

  const [groupScopeOpen, setGroupScopeOpen] = useState(false);
  const [dismissedErrorChannels, setDismissedErrorChannels] = useState<
    Set<"ilink" | "kefu">
  >(() => new Set());

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

  const bridgeHealth = bridgeHealthQuery.data;

  return (
    <div className="ds-canvas flex h-full flex-col overflow-hidden">
      {/* DS1.1 Hero — editorial onboarding layout.
          Follows `ClawWiki Design System/ui_kits/desktop-shell-v2/Connect.jsx`:
          a small `section-label` overline, a serif h1 in Chinese, then
          a plain-Chinese explanation of what happens. No iLink / long
          polling / raw path / pipeline wording at the default layer. */}
      <div className="mx-auto w-full max-w-[760px] shrink-0 px-6 pt-10 pb-4">
        <div
          className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground/70"
        >
          连接外脑 · 微信
        </div>
        <h1
          className="mt-1 text-[28px] leading-tight text-foreground"
          style={{ fontFamily: "var(--font-serif, \"Lora\", Georgia, serif)", fontWeight: 500, letterSpacing: "-0.2px" }}
        >
          让微信里的内容自动流进来
        </h1>
        <p className="mt-3 text-[14px] leading-[1.7] text-muted-foreground">
          用一个专属小号当作"外脑入口"。看到值得收藏的文章、语音、图片，转发给这个小号，ClawWiki 会自动接收、整理并归档。
        </p>
      </div>

      {/* Onboarding 3-step — default layer.
          Mirrors v2 kit's Connect.jsx "step-row" treatment without
          forking its imperative state: each step is purely illustrative,
          and the "Start" CTA jumps the user into the concrete action
          (扫码绑定 / 打开 Inbox) that the detailed sections below
          already own. No new backend logic, no new mutations. */}
      <OnboardingSteps
        hasConnectedAccount={accounts.length > 0}
        onStartBind={() => startLoginMutation.mutate()}
        bindPending={startLoginMutation.isPending || hasLoginInFlight}
      />

      {/* 高级信息 — all pre-DS1 technical content lives here.
          `<details>` is uncontrolled (browser-native) so no extra state
          and no HMR surprises. Users who need the bridge health, env
          doctor, account list, QR, kefu pipeline, or pipeline flow
          diagram expand one toggle. */}
      <details className="group min-h-0 flex-1 overflow-hidden border-b border-border/50 [&[open]]:flex [&[open]]:flex-col">
        <summary
          className="flex cursor-pointer items-center gap-2 border-b border-border/30 px-6 py-3 text-[12px] text-muted-foreground transition-colors hover:bg-accent/40"
        >
          <Settings className="size-3.5" />
          <span className="font-medium">高级信息</span>
          <span className="text-muted-foreground/60">
            · 连接状态、环境诊断、账号管理、客服号 Pipeline
          </span>
          <span className="ml-auto text-[11px] text-muted-foreground/60 group-open:hidden">
            展开
          </span>
          <span className="ml-auto hidden text-[11px] text-muted-foreground/60 group-open:inline">
            收起
          </span>
        </summary>
        <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="flex justify-end px-6 py-3">
          <button
            type="button"
            onClick={() => setGroupScopeOpen(true)}
            disabled={!bridgeHealth}
            className="flex shrink-0 items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-caption text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
          >
            <Settings className="size-3" />
            配置群组
          </button>
        </div>

      {/* M5 — dual-channel bridge health summary */}
      {bridgeHealth ? (
        <BridgeHealthHeader
          ilink={bridgeHealth.ilink}
          kefu={bridgeHealth.kefu}
          dismissedErrorChannels={dismissedErrorChannels}
          onErrorDismiss={(channel) =>
            setDismissedErrorChannels((prev) => {
              const next = new Set(prev);
              next.add(channel);
              return next;
            })
          }
        />
      ) : null}

      {/* Environment doctor — Playwright / MarkItDown capability matrix.
          Sits directly below the Hero so a misconfigured machine is
          diagnosed before the user fumbles with the login QR. */}
      <EnvironmentDoctor />

      {/* Recent URL ingest decisions — M3 observability surface. Sits
          directly under the doctor so a power user can answer "why was
          my URL reused / suppressed / rejected?" without opening the
          logs. Collapsed by default; only polls while expanded. */}
      <RecentIngestCard />

      {/* Connected accounts */}
      <section className="border-b border-border/50 px-6 py-5">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
            已连接账号
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
              title="刷新账号列表"
            >
              <RefreshCw
                className={
                  "size-3 " + (accountsQuery.isFetching ? "animate-spin" : "")
                }
              />
              刷新
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
              扫码绑定
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
          <div className="mt-3">
            <FailureBanner
              severity="error"
              title="启动登录失败"
              description={formatWeChatBridgeErrorMessage(
                String(startLoginMutation.error),
              )}
              technicalDetail={String(startLoginMutation.error)}
            />
          </div>
        )}
      </section>

      {/* QR login flow card */}
      {loginHandle && loginStatus && (
        <section className="border-b border-border/50 px-6 py-5">
          <h2 className="mb-3 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
            扫码登录
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
          消息流水线
        </h2>
        <div className="rounded-md border border-border/40 p-4">
          <div className="mb-2 text-foreground" style={{ fontSize: 14, fontWeight: 500 }}>
            微信个人号 → 素材库 → 待整理 → 问问题
          </div>
          <ol className="ml-5 list-decimal space-y-1.5 text-muted-foreground/70" style={{ fontSize: 13, lineHeight: 1.6 }}>
            <li>
              先在上方「扫码绑定」添加账号，打开 WeChat ClawBot 插件里扫描二维码。
            </li>
            <li>
              发给 bot 的文本消息会写入{" "}
              <code>~/.clawwiki/raw/</code>，按统一 schema 存档。
            </li>
            <li>
              每条新素材会自动排进{" "}
              <a href="#/inbox" className="text-primary hover:underline">
                待整理
              </a>
              队列，等你审阅后写入知识库。
            </li>
            <li>
              原有回复链路照常运行，手机端仍能即时收到 AI 回复，桌面这里同步留底。
            </li>
          </ol>
          <div className="mt-3 text-muted-foreground/40" style={{ fontSize: 11 }}>
            当前版本仅支持文本与链接；语音 / 图片 / PPT 等将在后续版本接入。
          </div>
        </div>
      </section>

        </div>
      </details>

      {/* M5 — group scope modal. Lives inside the page root so the dialog
          portal lifts it above every surrounding section. Stays OUTSIDE
          the `<details>` so the modal is reachable even when 高级信息
          is collapsed (the "配置群组" button lives inside 高级信息 but
          the modal itself uses Radix portal, so separation is fine). */}
      {bridgeHealth ? (
        <GroupScopeModal
          open={groupScopeOpen}
          onOpenChange={setGroupScopeOpen}
          config={bridgeHealth.config}
          onSaved={(saved: WeChatIngestConfig) => {
            queryClient.setQueryData<BridgeHealthResponse>(
              wechatKeys.bridgeHealth(),
              (prev) => (prev ? { ...prev, config: saved } : prev),
            );
            void queryClient.invalidateQueries({
              queryKey: wechatKeys.bridgeHealth(),
            });
          }}
        />
      ) : null}
    </div>
  );
}

/* ─── Onboarding steps (DS1-C) ─────────────────────────────────── */

/**
 * Three-step onboarding strip shown at the top of the WeChat page.
 * Displays the concrete user journey in plain Chinese; neutral when
 * nothing is known, "completed" once an account exists.
 *
 * The only interactive element is the primary CTA, which dispatches
 * the existing `startLoginMutation.mutate()` via the `onStartBind`
 * callback so this component stays dumb and adds no new backend path.
 */
function OnboardingSteps({
  hasConnectedAccount,
  onStartBind,
  bindPending,
}: {
  hasConnectedAccount: boolean;
  onStartBind: () => void;
  bindPending: boolean;
}) {
  type StepState = "done" | "active" | "pending";
  interface Step {
    n: number;
    title: string;
    desc: string;
    state: StepState;
  }
  const steps: ReadonlyArray<Step> = [
    {
      n: 1,
      title: "扫码绑定微信小号",
      desc: "用你的主号扫一下，就能和外脑小号建立连接。",
      state: hasConnectedAccount ? "done" : "active",
    },
    {
      n: 2,
      title: "转发一条内容试试",
      desc: "任何一篇公众号文章、一段语音、一张图片都行。",
      state: hasConnectedAccount ? "active" : "pending",
    },
    {
      n: 3,
      title: "在待整理里查看",
      desc: "通常几秒内到达，可以直接审阅、归档进知识库。",
      state: "pending",
    },
  ];

  return (
    <section className="mx-auto w-full max-w-[760px] shrink-0 px-6 pb-6">
      <div
        className="rounded-[14px] border bg-card px-6 py-2 shadow-warm-ring"
        style={{ borderColor: "var(--color-border)" }}
      >
        {steps.map((step) => (
          <StepRow
            key={step.n}
            n={step.n}
            title={step.title}
            desc={step.desc}
            state={step.state}
          >
            {step.state === "active" && step.n === 1 && (
              <button
                type="button"
                onClick={onStartBind}
                disabled={bindPending}
                className="mt-3 inline-flex items-center gap-1.5 rounded-md bg-primary px-4 py-2 text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
                style={{ fontSize: 13, fontWeight: 500 }}
              >
                {bindPending ? (
                  <Loader2 className="size-3 animate-spin" strokeWidth={1.5} />
                ) : (
                  <QrCode className="size-3" strokeWidth={1.5} />
                )}
                开始扫码绑定
              </button>
            )}
            {step.state === "active" && step.n === 2 && (
              <p className="mt-2 text-[11.5px] text-muted-foreground/70">
                已绑定小号，转发一条内容试试。
              </p>
            )}
          </StepRow>
        ))}
      </div>
    </section>
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
      <FailureBanner
        severity="error"
        title="列出微信账号失败"
        description={formatWeChatBridgeErrorMessage(error.message)}
        technicalDetail={error.message}
      />
    );
  }
  if (accounts.length === 0) {
    // R1 sprint — wrap in the shared `EmptyState` primitive for
    // visual consistency with the other surfaces. The card is
    // scoped to a single panel so we use the compact variant.
    return (
      <div className="rounded-md border border-dashed border-border/40">
        <EmptyState
          size="compact"
          icon={Link2}
          title="还没有微信账号"
          description={
            <>
              暂未连接任何微信账号。
              <br />
              点击顶部的「扫码绑定」开始接入。
            </>
          }
        />
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
                <span>最近活跃：{account.last_active_at}</span>
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
        return `✗ 登录失败${
          status.error
            ? ": " + formatWeChatBridgeErrorMessage(status.error)
            : ""
        }`;
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
                无法显示二维码（payload 不是 data:image/ URL）
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
            {formatExpiresAt(handle.expires_at)}
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
              账号：{status.account_id}
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
                取消登录
              </button>
            )}
            {status.status === "confirmed" && (
              <p className="text-caption text-muted-foreground">
                监听已启动 — 在微信里给 bot 发一条消息，就能看到消息进入待整理。
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
      return "等待扫码…";
    case "scanned":
      return "已扫描，请在手机上确认";
    case "confirmed":
      return "已连接";
    case "cancelled":
      return "已取消";
    case "expired":
      return "二维码已过期";
    case "failed":
      return "登录失败";
    default:
      return status;
  }
}

/**
 * Render `handle.expires_at` as a human-readable "有效期至 HH:MM" or
 * remaining-seconds countdown. The backend may emit either a Unix
 * epoch seconds value (sometimes suffixed with "s"), an ISO-8601
 * timestamp, or an already-formatted string; we try the first two
 * parses and fall back to the raw value so the caller never sees
 * a missing row.
 */
function formatExpiresAt(raw: string): string {
  if (!raw) return "";
  const trimmed = raw.replace(/s$/, "").trim();
  const numeric = Number(trimmed);
  if (Number.isFinite(numeric) && numeric > 1_000_000_000) {
    const ms =
      numeric < 10_000_000_000 ? numeric * 1000 : numeric; // sec→ms heuristic
    const d = new Date(ms);
    const hh = d.getHours().toString().padStart(2, "0");
    const mm = d.getMinutes().toString().padStart(2, "0");
    return `有效期至 ${hh}:${mm}`;
  }
  const parsed = Date.parse(raw);
  if (!Number.isNaN(parsed)) {
    const d = new Date(parsed);
    const hh = d.getHours().toString().padStart(2, "0");
    const mm = d.getMinutes().toString().padStart(2, "0");
    return `有效期至 ${hh}:${mm}`;
  }
  return `有效期 ${raw}`;
}

/* ─── Channel B: Official WeChat Customer Service ──────────────── */

const kefuKeys = {
  config: () => ["kefu", "config"] as const,
  status: () => ["kefu", "status"] as const,
};

const DEFAULT_KEFU_CAPABILITIES: KefuCapabilities = {
  text: true,
  url: true,
  query: true,
  commands: ["/recent", "/stats"],
  file: false,
  image: false,
  card: false,
  share: false,
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
  const capabilities = status?.capabilities ?? DEFAULT_KEFU_CAPABILITIES;

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

      <KefuCapabilitiesPanel capabilities={capabilities} />

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
              <div className="mt-2">
                <FailureBanner
                  severity="error"
                  title="创建客服账号失败"
                  description={formatWeChatBridgeErrorMessage(
                    String(createMutation.error),
                  )}
                  technicalDetail={String(createMutation.error)}
                />
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
                    {formatWeChatBridgeErrorMessage(status.last_error)}
                  </span>
                )}
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="rounded-md border border-dashed border-border/50">
          <EmptyState
            size="compact"
            icon={Link2}
            title="尚未配置企业微信客服"
            description='点击 "配置" 填入企业微信 corpid 和客服 secret，开始接入微信客服。'
          />
        </div>
      )}
    </section>
  );
}

function KefuCapabilitiesPanel({
  capabilities,
}: {
  capabilities: KefuCapabilities;
}) {
  const commandLabel =
    capabilities.commands.length > 0
      ? `命令 ${capabilities.commands.join(" ")}`
      : "命令";
  const capabilityItems = [
    { label: "文本", enabled: capabilities.text },
    { label: "链接", enabled: capabilities.url },
    { label: "? 查询", enabled: capabilities.query },
    { label: commandLabel, enabled: capabilities.commands.length > 0 },
    { label: "文件", enabled: capabilities.file },
    { label: "图片", enabled: capabilities.image },
    { label: "卡片", enabled: capabilities.card },
    { label: "分享", enabled: capabilities.share },
  ];
  const supported = capabilityItems
    .filter((item) => item.enabled)
    .map((item) => item.label);
  const unsupported = capabilityItems
    .filter((item) => !item.enabled)
    .map((item) => item.label);

  return (
    <div className="mb-3 rounded-md border border-border bg-muted/5 p-4">
      <div className="mb-2 flex items-center justify-between gap-3">
        <div className="text-body-sm font-medium text-foreground">
          当前消息能力
        </div>
        <span className="rounded-full bg-muted px-2 py-0.5 text-caption text-muted-foreground">
          状态同步
        </span>
      </div>
      <p className="text-caption text-muted-foreground">
        当前支持 {supported.join("、")}；{unsupported.join("、")} 暂不支持。
      </p>
      <div className="mt-3 flex flex-wrap gap-2">
        {capabilityItems.map((item) => (
          <span
            key={item.label}
            className={`inline-flex items-center gap-1 rounded-full border px-2 py-1 text-caption ${
              item.enabled
                ? "border-primary/30 bg-primary/10 text-primary"
                : "border-border bg-background text-muted-foreground"
            }`}
          >
            {item.enabled ? (
              <CheckCircle2 className="size-3" />
            ) : (
              <XCircle className="size-3" />
            )}
            {item.label}
          </span>
        ))}
      </div>
    </div>
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
        </div>
        {saveMutation.error && (
          <div className="mt-3">
            <FailureBanner
              severity="error"
              title="保存配置失败"
              description={formatWeChatBridgeErrorMessage(
                String(saveMutation.error),
              )}
              technicalDetail={String(saveMutation.error)}
            />
          </div>
        )}
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
              ? `获取客服链接失败: ${formatWeChatBridgeErrorMessage(String(contactQuery.error))}`
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
            自动注册 Cloudflare 中继 → 企业微信扫码授权 → 配置回调 → 创建客服账号。接入后当前支持文本、链接与 ? 查询，文件/图片仍按能力面板显示为暂不支持。
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
            <FailureBanner
              severity="error"
              title="启动接入流程失败"
              description={formatWeChatBridgeErrorMessage(
                String(startMutation.error),
              )}
              technicalDetail={String(startMutation.error)}
            />
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
                  {formatWeChatBridgeErrorMessage(p.error)}
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
                  {visibleLogs
                    .map((line) => formatWeChatBridgeErrorMessage(line))
                    .join("\n")}
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

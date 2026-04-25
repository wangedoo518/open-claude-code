import { useEffect, useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  AlertCircle,
  CheckCircle2,
  Loader2,
  MessageCircle,
  Plus,
  QrCode,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import {
  cancelWeChatLogin,
  deleteWeChatAccount,
  getWeChatLoginStatus,
  listWeChatAccounts,
  startWeChatLogin,
  type WeChatAccountStatus,
  type WeChatAccountSummary,
  type WeChatLoginStatus,
  type WeChatLoginStatusResponse,
} from "@/api/desktop/settings";

/**
 * Phase 6C — WeChat account management UI.
 *
 * Pairs with the Phase 6C backend routes at `/api/desktop/wechat/*`.
 * Lets users:
 *  - list persisted WeChat bot accounts with live connection status
 *  - add a new account via in-app QR code scan (no CLI)
 *  - delete an account (stops its monitor + removes credential files)
 *
 * The add flow drives a Radix modal with a 1-second poll loop that
 * tracks the backend's pending-login slot until the user confirms on
 * their phone or cancels.
 */
export function WeChatSettings() {
  const queryClient = useQueryClient();
  const [flashError, setFlashError] = useState<string | null>(null);
  const [loginHandle, setLoginHandle] = useState<string | null>(null);
  const [loginQrImage, setLoginQrImage] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] =
    useState<WeChatAccountSummary | null>(null);

  const accountsQuery = useQuery({
    queryKey: ["wechat", "accounts"],
    queryFn: () => listWeChatAccounts(),
    // Monitors reconnect in the background — poll periodically so the
    // status badge stays fresh even when the user just sits on this page.
    refetchInterval: 5_000,
  });

  const startLoginMutation = useMutation({
    mutationFn: () => startWeChatLogin(),
    onSuccess: (data) => {
      setLoginHandle(data.handle);
      setLoginQrImage(data.qr_image_base64);
    },
    onError: (err) => setFlashError(errorMessage(err)),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteWeChatAccount(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ["wechat", "accounts"],
      });
    },
    onError: (err) => setFlashError(errorMessage(err)),
  });

  const accounts = accountsQuery.data?.accounts ?? [];

  return (
    <div className="space-y-5">
      <header className="space-y-1">
        <h3 className="text-subhead font-semibold text-foreground">
          WeChat 账号
        </h3>
        <p className="text-caption text-muted-foreground">
          绑定微信 ClawBot 后，你可以直接在手机微信里和配置的 LLM 对话。
          消息会经由 iLink 长轮询转发到桌面运行时，回复走 agentic loop
          并分片返回到手机上。凭证保存在本机的{" "}
          <code className="rounded bg-muted px-1 py-0.5 text-label">
            %LOCALAPPDATA%\warwolf\wechat\
          </code>
          ，不会上传到任何远程服务。
        </p>
      </header>

      {flashError && (
        <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-body-sm text-destructive">
          <AlertCircle className="mt-0.5 size-4 shrink-0" />
          <div className="min-w-0 flex-1">{flashError}</div>
          <button
            className="shrink-0 rounded p-1 opacity-70 hover:opacity-100"
            onClick={() => setFlashError(null)}
            aria-label="关闭"
          >
            <X className="size-3.5" />
          </button>
        </div>
      )}

      {accountsQuery.isLoading ? (
        <SectionLoading />
      ) : accounts.length === 0 ? (
        <EmptyState
          onAddClick={() => {
            setFlashError(null);
            startLoginMutation.mutate();
          }}
          starting={startLoginMutation.isPending}
        />
      ) : (
        <div className="space-y-2">
          {accounts.map((account) => (
            <AccountCard
              key={account.id}
              account={account}
              onDelete={() => {
                setFlashError(null);
                setPendingDelete(account);
              }}
              deleting={
                deleteMutation.isPending &&
                deleteMutation.variables === account.id
              }
            />
          ))}
        </div>
      )}

      <Separator />

      <div className="flex items-center justify-between">
        <div className="text-body-sm font-semibold text-foreground">
          添加微信账号
        </div>
        {accounts.length > 0 && (
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              setFlashError(null);
              startLoginMutation.mutate();
            }}
            disabled={startLoginMutation.isPending}
          >
            {startLoginMutation.isPending ? (
              <Loader2 className="mr-1.5 size-3.5 animate-spin" />
            ) : (
              <Plus className="mr-1.5 size-3.5" />
            )}
            扫码绑定
          </Button>
        )}
      </div>

      <div className="text-caption text-muted-foreground">
        <RefreshCw className="mr-1 inline size-3" />
        登录成功后后端会自动启动 iLink 长轮询监听器，一次绑定永久生效。
      </div>

      <ConfirmDialog
        open={!!pendingDelete}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
        title="删除微信账号"
        description={
          pendingDelete
            ? `确定要删除微信账号 "${pendingDelete.display_name}" 吗？后端监听器会立即停止，凭证文件也会从磁盘清除。此操作不可撤销。`
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

      {loginHandle && loginQrImage && (
        <QrLoginDialog
          handle={loginHandle}
          qrImage={loginQrImage}
          onClose={(finalStatus) => {
            setLoginHandle(null);
            setLoginQrImage(null);
            // Always refetch so new accounts show up and cancelled
            // flows clear any stale waiting state.
            void queryClient.invalidateQueries({
              queryKey: ["wechat", "accounts"],
            });
            if (finalStatus === "failed") {
              setFlashError("二维码登录失败，请稍后重试。");
            }
          }}
        />
      )}
    </div>
  );
}

// ── Sub-components ───────────────────────────────────────────────

function SectionLoading() {
  return (
    <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/20 px-4 py-3 text-body-sm text-muted-foreground">
      <Loader2 className="size-4 animate-spin" />
      <span>加载微信账号列表...</span>
    </div>
  );
}

function EmptyState({
  onAddClick,
  starting,
}: {
  onAddClick: () => void;
  starting: boolean;
}) {
  return (
    <div className="rounded-lg border border-dashed border-border bg-muted/10 px-5 py-8 text-center">
      <MessageCircle className="mx-auto mb-3 size-8 text-muted-foreground" />
      <p className="mb-3 text-body-sm text-muted-foreground">
        还没有绑定任何微信账号。点下方按钮扫码绑定第一个 WeChat ClawBot。
      </p>
      <Button size="sm" onClick={onAddClick} disabled={starting}>
        {starting ? (
          <Loader2 className="mr-1.5 size-3.5 animate-spin" />
        ) : (
          <QrCode className="mr-1.5 size-3.5" />
        )}
        扫码绑定
      </Button>
    </div>
  );
}

function AccountCard({
  account,
  onDelete,
  deleting,
}: {
  account: WeChatAccountSummary;
  onDelete: () => void;
  deleting: boolean;
}) {
  return (
    <div className="rounded-lg border border-border bg-background p-3 transition-colors hover:border-border/80">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-body font-semibold text-foreground">
              {account.display_name}
            </span>
            <StatusBadge status={account.status} />
          </div>
          <div className="text-caption text-muted-foreground">
            <code className="rounded bg-muted px-1 py-0.5">{account.id}</code>
          </div>
          <div className="text-caption text-muted-foreground">
            <span>{account.base_url}</span>
          </div>
          <div className="text-caption text-muted-foreground">
            <span className="font-mono">{account.bot_token_preview}</span>
          </div>
          {account.last_active_at && (
            <div className="text-caption text-muted-foreground">
              上次活跃：{account.last_active_at}
            </div>
          )}
        </div>
        <div className="flex flex-col gap-1.5">
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

function StatusBadge({ status }: { status: WeChatAccountStatus }) {
  const config: Record<
    WeChatAccountStatus,
    { label: string; bg: string; fg: string }
  > = {
    connected: {
      label: "已连接",
      bg: "color-mix(in srgb, var(--color-success) 12%, transparent)",
      fg: "var(--color-success)",
    },
    session_expired: {
      label: "会话过期",
      bg: "color-mix(in srgb, var(--color-warning) 12%, transparent)",
      fg: "var(--color-warning)",
    },
    disconnected: {
      label: "未连接",
      bg: "color-mix(in srgb, var(--color-muted-foreground) 12%, transparent)",
      fg: "var(--color-muted-foreground)",
    },
  };
  const c = config[status];
  return (
    <span
      className="rounded px-1.5 py-0.5 text-caption font-semibold uppercase"
      style={{ backgroundColor: c.bg, color: c.fg }}
    >
      {c.label}
    </span>
  );
}

function QrLoginDialog({
  handle,
  qrImage,
  onClose,
}: {
  handle: string;
  qrImage: string;
  onClose: (finalStatus: WeChatLoginStatus | null) => void;
}) {
  const [finalStatus, setFinalStatus] = useState<WeChatLoginStatus | null>(
    null
  );

  // Poll the pending-login slot every second until a terminal state.
  const statusQuery = useQuery({
    queryKey: ["wechat", "login", handle],
    queryFn: () => getWeChatLoginStatus(handle),
    refetchInterval: (query) => {
      const data = query.state.data as WeChatLoginStatusResponse | undefined;
      if (!data) return 1_000;
      if (isTerminalStatus(data.status)) return false;
      return 1_000;
    },
    staleTime: 0,
  });

  const status = statusQuery.data?.status ?? "waiting";
  const statusError = statusQuery.data?.error ?? null;

  // When we observe a terminal state, keep the dialog open briefly so
  // the user can see the outcome, then close.
  useEffect(() => {
    if (!statusQuery.data) return;
    if (!isTerminalStatus(statusQuery.data.status)) return;
    setFinalStatus(statusQuery.data.status);
    const closeDelay =
      statusQuery.data.status === "confirmed" ? 1_500 : 2_500;
    const timer = setTimeout(() => onClose(statusQuery.data!.status), closeDelay);
    return () => clearTimeout(timer);
  }, [statusQuery.data, onClose]);

  const handleCancel = async () => {
    try {
      await cancelWeChatLogin(handle);
    } catch {
      // best-effort
    }
    onClose("cancelled");
  };

  return (
    <DialogPrimitive.Root
      open
      onOpenChange={(open) => {
        if (!open) onClose(finalStatus);
      }}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0" />
        <DialogPrimitive.Content
          className={cn(
            "fixed left-1/2 top-1/2 z-50 w-full max-w-[420px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-background p-6 shadow-lg",
            "data-[state=open]:animate-in data-[state=closed]:animate-out",
            "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
            "data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
          )}
        >
          <div className="flex items-center justify-between">
            <DialogPrimitive.Title className="text-subhead font-semibold text-foreground">
              扫码绑定微信
            </DialogPrimitive.Title>
            <DialogPrimitive.Close
              className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
              aria-label="关闭"
            >
              <X className="size-4" />
            </DialogPrimitive.Close>
          </div>

          <div className="mt-5 flex justify-center">
            <div className="rounded-lg border border-border bg-muted/20 p-3">
              {(() => {
                const imgSrc = qrImageSrc(qrImage);
                return imgSrc ? (
                  <img
                    src={imgSrc}
                    alt="WeChat QR code"
                    className="size-60 rounded bg-white"
                  />
                ) : (
                  <div className="flex size-60 items-center justify-center rounded border border-dashed border-border bg-white p-3 text-caption text-muted-foreground">
                    <a
                      href={qrImage}
                      target="_blank"
                      rel="noreferrer"
                      className="text-primary underline"
                    >
                      在新窗口打开二维码链接
                    </a>
                  </div>
                );
              })()}
            </div>
          </div>

          <div className="mt-5 text-center">
            <StatusText status={status} error={statusError} />
          </div>

          <div className="mt-5 flex flex-col gap-2 text-caption text-muted-foreground">
            <div>
              <span className="font-semibold text-foreground">操作步骤：</span>
            </div>
            <div>1. 打开手机微信 → 我 → 设置 → 插件 → 微信 ClawBot</div>
            <div>2. 扫描上方二维码</div>
            <div>3. 在手机上点击「确认绑定」</div>
          </div>

          <div className="mt-5 flex justify-end gap-2">
            <Button variant="outline" size="sm" onClick={handleCancel}>
              取消
            </Button>
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

function StatusText({
  status,
  error,
}: {
  status: WeChatLoginStatus;
  error: string | null;
}) {
  switch (status) {
    case "waiting":
      return (
        <div className="flex items-center justify-center gap-2 text-body-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          <span>请用微信扫描二维码</span>
        </div>
      );
    case "scanned":
      return (
        <div className="flex items-center justify-center gap-2 text-body-sm text-foreground">
          <CheckCircle2 className="size-4 text-primary" />
          <span>已扫描，请在手机上确认</span>
        </div>
      );
    case "confirmed":
      return (
        <div
          className="flex items-center justify-center gap-2 text-body-sm font-semibold"
          style={{ color: "var(--color-success)" }}
        >
          <CheckCircle2 className="size-4" />
          <span>绑定成功！</span>
        </div>
      );
    case "failed":
      return (
        <div
          className="flex flex-col items-center gap-1 text-body-sm"
          style={{ color: "var(--color-error)" }}
        >
          <div className="flex items-center gap-2">
            <AlertCircle className="size-4" />
            <span>绑定失败</span>
          </div>
          {error && <div className="text-caption">{error}</div>}
        </div>
      );
    case "cancelled":
      return (
        <div className="text-body-sm text-muted-foreground">已取消</div>
      );
    case "expired":
      return (
        <div
          className="flex items-center justify-center gap-2 text-body-sm"
          style={{ color: "var(--color-warning)" }}
        >
          <AlertCircle className="size-4" />
          <span>二维码已过期，请重试</span>
        </div>
      );
  }
}

function isTerminalStatus(status: WeChatLoginStatus): boolean {
  return (
    status === "confirmed" ||
    status === "failed" ||
    status === "cancelled" ||
    status === "expired"
  );
}

/**
 * Determine the best `<img src>` for the QR code value returned
 * by the iLink login API.
 *
 * The backend returns one of:
 *   1. `data:image/png;base64,...` — a real image, use directly
 *   2. `https://liteapp.weixin.qq.com/q/...` — a scan-target URL
 *      that needs to be ENCODED INTO a QR code image
 *   3. Anything else — show as a plain link fallback
 *
 * For case 2, we generate a QR image via the free
 * `api.qrserver.com` service. This is a well-known public API
 * that renders QR codes as PNG images — no npm dependency needed.
 */
function qrImageSrc(value: string): string | null {
  if (value.startsWith("data:image/")) {
    return value; // already an image
  }
  if (value.startsWith("http://") || value.startsWith("https://")) {
    // It's a URL that should be encoded AS a QR code, not loaded
    // directly as an image. Use the free qrserver.com API.
    const encoded = encodeURIComponent(value);
    return `https://api.qrserver.com/v1/create-qr-code/?size=240x240&data=${encoded}`;
  }
  return null; // unknown format — use fallback link
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

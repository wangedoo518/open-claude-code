/**
 * ChannelStatusModal — runtime status panel for the WeChat kefu channel.
 *
 * Shown when the user clicks the status badge while connected.
 * Displays live stats, QR contact link, capabilities matrix, and
 * reconnect / disconnect actions.
 */

import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSettingsStore } from "@/state/settings-store";
import {
  getKefuContactUrl,
  startKefuMonitor,
  stopKefuMonitor,
} from "@/features/settings/api/client";
import type { KefuCapabilities } from "@/features/settings/api/client";
import { useKefuStatus } from "./useKefuStatus";
import { kefuQueryKeys } from "./kefu-query-keys";

// ── Helpers ──────────────────────────────────────────────────────────

function relativeTime(unixMs: number | null | undefined): string {
  if (!unixMs) return "暂无";
  const diff = Date.now() - unixMs;
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  return `${Math.floor(diff / 86_400_000)} 天前`;
}

type StatusLabel = "在线" | "已停止" | "异常";

function statusMeta(channelStatus: string): {
  label: StatusLabel;
  dotClass: string;
} {
  switch (channelStatus) {
    case "connected":
      return { label: "在线", dotClass: "bg-green-500" };
    case "error":
      return { label: "异常", dotClass: "bg-red-500" };
    default:
      return { label: "已停止", dotClass: "bg-orange-400" };
  }
}

// Mapping from KefuCapabilities field → Chinese label
const CAPABILITY_ENTRIES: {
  key: keyof Omit<KefuCapabilities, "commands">;
  label: string;
}[] = [
  { key: "text", label: "文本" },
  { key: "url", label: "链接" },
  { key: "query", label: "查询" },
  { key: "file", label: "文件" },
  { key: "image", label: "图片" },
  { key: "card", label: "卡片" },
  { key: "share", label: "分享" },
];

// ── Component ────────────────────────────────────────────────────────

export function ChannelStatusModal() {
  const open = useSettingsStore((s) => s.channelStatusModalOpen);
  const close = useSettingsStore((s) => s.closeChannelStatusModal);
  const openSettings = useSettingsStore((s) => s.openSettingsModal);

  // Poll every 10 s while modal is open
  const { raw, channelStatus } = useKefuStatus(open ? 10_000 : 30_000);

  const queryClient = useQueryClient();

  // Contact URL query
  const contactQuery = useQuery({
    queryKey: kefuQueryKeys.contactUrl(),
    queryFn: getKefuContactUrl,
    enabled: open,
    staleTime: 60_000,
  });
  const contactUrl = contactQuery.data?.url ?? null;

  // Mutations
  const reconnectMutation = useMutation({
    mutationFn: startKefuMonitor,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: kefuQueryKeys.status() }),
  });

  const disconnectMutation = useMutation({
    mutationFn: stopKefuMonitor,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: kefuQueryKeys.status() }),
  });

  const { label, dotClass } = statusMeta(channelStatus);

  const qrUrl = contactUrl
    ? `https://api.qrserver.com/v1/create-qr-code/?size=160x160&data=${encodeURIComponent(contactUrl)}`
    : null;

  // Derive the "commands" capability separately (it's a string[])
  const commandsSupported =
    raw?.capabilities?.commands && raw.capabilities.commands.length > 0;

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(o) => {
        if (!o) close();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/40 data-[state=open]:animate-fade-in" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 flex max-h-[85vh] w-full max-w-md -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-[var(--color-border)] bg-[var(--color-background)] shadow-lg data-[state=open]:animate-fade-in">
          {/* ── Header ─────────────────────────────────────── */}
          <div className="flex h-12 shrink-0 items-center justify-between border-b border-[var(--color-border)] px-4">
            <Dialog.Title className="text-[14px] font-semibold text-[var(--color-foreground)]">
              微信客服
            </Dialog.Title>
            <Dialog.Close className="rounded-md p-1 text-[var(--color-muted-foreground)] hover:bg-[var(--color-accent)] hover:text-[var(--color-foreground)]">
              <X className="size-4" />
            </Dialog.Close>
          </div>

          {/* ── Body ───────────────────────────────────────── */}
          <div className="flex-1 overflow-y-auto p-4 space-y-5">
            {/* Status card */}
            <div className="rounded-xl border border-[var(--color-border)] p-4 space-y-3">
              <div className="flex items-center gap-2">
                <span
                  className={`inline-block size-2.5 rounded-full ${dotClass}`}
                />
                <span className="text-sm font-medium text-[var(--color-foreground)]">
                  {label}
                </span>
                <span className="text-xs text-[var(--color-muted-foreground)]">
                  ClaudeWiki 助手
                </span>
              </div>

              {/* Stats row */}
              <div className="grid grid-cols-3 gap-2 text-center">
                <div>
                  <div className="text-[11px] text-[var(--color-muted-foreground)]">
                    最近拉取
                  </div>
                  <div className="text-xs font-medium text-[var(--color-foreground)]">
                    {relativeTime(raw?.last_poll_unix_ms)}
                  </div>
                </div>
                <div>
                  <div className="text-[11px] text-[var(--color-muted-foreground)]">
                    最近消息
                  </div>
                  <div className="text-xs font-medium text-[var(--color-foreground)]">
                    {relativeTime(raw?.last_inbound_unix_ms)}
                  </div>
                </div>
                <div>
                  <div className="text-[11px] text-[var(--color-muted-foreground)]">
                    连续异常
                  </div>
                  <div className="text-xs font-medium text-[var(--color-foreground)]">
                    {raw?.consecutive_failures ?? 0}
                  </div>
                </div>
              </div>
            </div>

            {/* QR code section */}
            {contactUrl && (
              <div className="flex flex-col items-center gap-3">
                {qrUrl && (
                  <img
                    src={qrUrl}
                    alt="QR code"
                    width={160}
                    height={160}
                    className="rounded-lg"
                  />
                )}
                <p className="text-xs text-[var(--color-muted-foreground)]">
                  扫码添加 ClaudeWiki 助手
                </p>
                <div className="flex gap-2">
                  <button
                    type="button"
                    className="rounded-lg border border-[var(--color-border)] px-3 py-1.5 text-xs text-[var(--color-foreground)] hover:bg-[var(--color-accent)]"
                    onClick={() =>
                      void navigator.clipboard.writeText(contactUrl)
                    }
                  >
                    复制链接
                  </button>
                  <a
                    href={qrUrl ?? "#"}
                    download="claudewiki-wechat-qr.png"
                    className="inline-flex items-center rounded-lg border border-[var(--color-border)] px-3 py-1.5 text-xs text-[var(--color-foreground)] hover:bg-[var(--color-accent)]"
                  >
                    下载二维码
                  </a>
                </div>
              </div>
            )}

            {/* Capabilities matrix */}
            {raw?.capabilities && (
              <div className="space-y-2">
                <div className="text-xs font-medium text-[var(--color-muted-foreground)]">
                  支持的消息类型
                </div>
                <div className="flex flex-wrap gap-2">
                  {CAPABILITY_ENTRIES.map(({ key, label: capLabel }) => {
                    const supported = raw.capabilities![key] as boolean;
                    return (
                      <span
                        key={key}
                        className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs ${
                          supported
                            ? "bg-green-500/10 text-green-700 dark:text-green-400"
                            : "bg-[var(--color-accent)] text-[var(--color-muted-foreground)]"
                        }`}
                      >
                        {supported ? "\u2705" : "\u2B1C"} {capLabel}
                      </span>
                    );
                  })}

                  {/* commands — derived from array length */}
                  <span
                    className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs ${
                      commandsSupported
                        ? "bg-green-500/10 text-green-700 dark:text-green-400"
                        : "bg-[var(--color-accent)] text-[var(--color-muted-foreground)]"
                    }`}
                  >
                    {commandsSupported ? "\u2705" : "\u2B1C"} 命令
                  </span>
                </div>
              </div>
            )}
          </div>

          {/* ── Action buttons ─────────────────────────────── */}
          <div className="flex items-center justify-between border-t border-[var(--color-border)] px-4 py-3">
            <button
              type="button"
              className="rounded-lg border border-[var(--color-border)] px-3 py-1.5 text-xs font-medium text-[var(--color-foreground)] hover:bg-[var(--color-accent)] disabled:opacity-50"
              disabled={reconnectMutation.isPending}
              onClick={() => reconnectMutation.mutate()}
            >
              重新连接
            </button>

            <button
              type="button"
              className="rounded-lg border border-[var(--color-border)] px-3 py-1.5 text-xs font-medium text-[var(--color-foreground)] hover:bg-[var(--color-accent)]"
              onClick={() => {
                close();
                openSettings();
              }}
            >
              查看设置
            </button>

            <button
              type="button"
              className="rounded-lg border border-red-300 px-3 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950 disabled:opacity-50"
              disabled={disconnectMutation.isPending}
              onClick={() => {
                if (window.confirm("确定断开微信客服连接？")) {
                  disconnectMutation.mutate();
                }
              }}
            >
              断开连接
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

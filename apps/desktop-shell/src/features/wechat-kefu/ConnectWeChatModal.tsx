/**
 * ConnectWeChatModal -- simplified confirmation entry point for the
 * WeChat Kefu (customer service) onboarding flow.
 *
 * The modal is now just a single confirmation step that explains
 * what the pipeline does. Clicking "开始接入" closes the modal and
 * navigates to the full-screen pipeline page at /connect-wechat.
 *
 * All pipeline execution, progress, QR scanning, manual steps, and
 * congrats are handled by ConnectWeChatPipelinePage.
 */

import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";

import { useSettingsStore } from "@/state/settings-store";

// ── Component ──────────────────────────────────────────────────────

export function ConnectWeChatModal() {
  const open = useSettingsStore((s) => s.connectWeChatModalOpen);
  const close = useSettingsStore((s) => s.closeConnectWeChatModal);
  const navigate = useNavigate();

  // No internal step state needed -- the modal is just a confirmation gate.
  // Reset nothing on close since there's no state to reset.
  useEffect(() => {
    // intentional no-op placeholder for future use
  }, [open]);

  function handleStart() {
    close();
    navigate("/connect-wechat");
  }

  return (
    <Dialog.Root open={open} onOpenChange={(o) => { if (!o) close(); }}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/40 data-[state=open]:animate-fade-in" />
        <Dialog.Content
          className="fixed left-1/2 top-1/2 z-50 flex max-h-[85vh] w-full max-w-lg -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-[var(--color-border)] bg-[var(--color-background)] shadow-xl data-[state=open]:animate-fade-in"
        >
          {/* Header */}
          <div className="flex h-12 shrink-0 items-center justify-between border-b border-[var(--color-border)] px-4">
            <Dialog.Title className="text-[14px] font-semibold text-[var(--color-foreground)]">
              接入微信客服
            </Dialog.Title>
            <Dialog.Close className="rounded-md p-1 text-[var(--color-muted-foreground)] hover:bg-[var(--color-accent)] hover:text-[var(--color-foreground)]">
              <X className="size-4" />
            </Dialog.Close>
          </div>

          {/* Body -- confirmation step */}
          <div className="flex-1 overflow-y-auto">
            <div className="flex flex-col gap-5 p-6">
              {/* Hero */}
              <div className="flex flex-col items-center gap-2 text-center">
                <span className="text-4xl">{"\uD83D\uDE80"}</span>
                <h3 className="text-lg font-bold text-[var(--color-foreground)]">
                  一键接入微信客服
                </h3>
                <p className="max-w-sm text-sm leading-relaxed text-[var(--color-muted-foreground)]">
                  自动部署 Cloudflare 中继 + 配置企业微信客服账号，
                  全程引导，3 分钟完成。
                </p>
              </div>

              {/* Info card: 2 manual operations */}
              <div className="rounded-xl border border-amber-200 bg-amber-50 p-4">
                <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-amber-800">
                  <span>{"\u270B"}</span>
                  <span>2 次手动操作</span>
                </div>
                <ul className="space-y-1.5 text-sm text-amber-700">
                  <li className="flex items-start gap-2">
                    <span className="mt-0.5 flex-shrink-0 text-xs">{"\u2460"}</span>
                    <span>Cloudflare 注册时可能需要完成人机验证（CAPTCHA）</span>
                  </li>
                  <li className="flex items-start gap-2">
                    <span className="mt-0.5 flex-shrink-0 text-xs">{"\u2461"}</span>
                    <span>企业微信授权需要扫描二维码</span>
                  </li>
                </ul>
                <p className="mt-2 text-xs text-amber-600">
                  其余步骤（部署中继、配置回调、创建客服账号）均自动完成。
                </p>
              </div>

              {/* Pipeline steps preview */}
              <div className="rounded-xl border border-[var(--color-border)] p-4">
                <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-[var(--color-muted-foreground)]">
                  执行步骤
                </h4>
                <ol className="space-y-1.5 text-sm text-[var(--color-muted-foreground)]">
                  <li className="flex items-center gap-2">
                    <span className="flex h-5 w-5 items-center justify-center rounded-full bg-gray-200 text-xs font-semibold text-gray-600">1</span>
                    <span>{"\u2601\uFE0F"} Cloudflare 账号注册</span>
                    <span className="ml-auto rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700">手动</span>
                  </li>
                  <li className="flex items-center gap-2">
                    <span className="flex h-5 w-5 items-center justify-center rounded-full bg-gray-200 text-xs font-semibold text-gray-600">2</span>
                    <span>{"\uD83D\uDE80"} 部署中继服务器</span>
                    <span className="ml-auto rounded-full bg-indigo-100 px-1.5 py-0.5 text-[10px] text-indigo-700">自动</span>
                  </li>
                  <li className="flex items-center gap-2">
                    <span className="flex h-5 w-5 items-center justify-center rounded-full bg-gray-200 text-xs font-semibold text-gray-600">3</span>
                    <span>{"\uD83D\uDCF1"} 企业微信扫码授权</span>
                    <span className="ml-auto rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700">手动</span>
                  </li>
                  <li className="flex items-center gap-2">
                    <span className="flex h-5 w-5 items-center justify-center rounded-full bg-gray-200 text-xs font-semibold text-gray-600">4</span>
                    <span>{"\uD83D\uDD17"} 回调 URL 配置</span>
                    <span className="ml-auto rounded-full bg-indigo-100 px-1.5 py-0.5 text-[10px] text-indigo-700">自动</span>
                  </li>
                  <li className="flex items-center gap-2">
                    <span className="flex h-5 w-5 items-center justify-center rounded-full bg-gray-200 text-xs font-semibold text-gray-600">5</span>
                    <span>{"\uD83D\uDC64"} 创建客服账号</span>
                    <span className="ml-auto rounded-full bg-indigo-100 px-1.5 py-0.5 text-[10px] text-indigo-700">自动</span>
                  </li>
                </ol>
              </div>

              {/* Info blurb */}
              <div className="rounded-lg bg-[var(--color-accent)] p-4 text-[13px] leading-relaxed text-[var(--color-muted-foreground)]">
                {"\uD83D\uDCD6"} 微信客服接入后，用户扫码即可与 ClaudeWiki 助手对话：发送链接/文本投喂知识，用{" "}
                <code className="rounded bg-gray-200 px-1 text-xs">?</code>{" "}
                前缀提问查询知识库。
              </div>

              {/* CTA */}
              <button
                type="button"
                onClick={handleStart}
                className="w-full rounded-lg bg-indigo-600 px-4 py-2.5 text-sm font-semibold text-white transition hover:bg-indigo-700"
              >
                开始接入 {"\u2192"}
              </button>
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

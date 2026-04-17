/**
 * ConnectWeChatModal — multi-step onboarding dialog for WeChat Kefu
 * (customer service) channel.
 *
 * Steps:
 *   1. choose   – pick between one-click pipeline vs manual config
 *   2. pipeline – automated Cloudflare + WeChat pipeline
 *   3. manual   – manual corpid / secret form
 *   4. success  – confirmation with QR contact link
 */

import { useCallback, useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";

import { useSettingsStore } from "@/state/settings-store";
import {
  saveKefuConfig,
  createKefuAccount,
  startKefuMonitor,
  getKefuContactUrl,
  startKefuPipeline,
  getKefuPipelineStatus,
  cancelKefuPipeline,
} from "@/features/settings/api/client";
import type {
  KefuConfigRequest,
  KefuPipelineState,
  PipelinePhaseState,
} from "@/features/settings/api/client";
import { kefuQueryKeys } from "./kefu-query-keys";

// ── Shared helpers ──────────────────────────────────────────────────

type ModalStep = "choose" | "pipeline" | "manual" | "success";

/** Reject any data-URI that isn't an image type. */
function isSafeQrDataUrl(src: string): boolean {
  return src.startsWith("data:image/");
}

const PHASE_LABELS: Record<string, string> = {
  cf_register: "Cloudflare 账号",
  worker_deploy: "部署中继服务器",
  wecom_auth: "扫码授权企微",
  callback_config: "回调 URL 配置",
  kefu_create: "创建客服账号",
};

const PHASE_NUMBERS = ["①", "②", "③", "④", "⑤"];

// ── Sub-step components ─────────────────────────────────────────────

function StepChoose({ onPipeline, onManual }: { onPipeline: () => void; onManual: () => void }) {
  return (
    <div className="flex flex-col gap-4 p-6">
      {/* Card 1: One-click */}
      <button
        type="button"
        onClick={onPipeline}
        className="group relative flex flex-col items-start gap-2 rounded-xl border border-[var(--color-border)] p-5 text-left transition hover:border-indigo-400 hover:shadow-md"
      >
        <span className="absolute right-3 top-3 rounded-full bg-indigo-600 px-2 py-0.5 text-[11px] font-semibold text-white">
          推荐
        </span>
        <span className="text-lg font-semibold text-[var(--color-foreground)]">
          🚀 一键接入
        </span>
        <span className="text-sm text-[var(--color-muted-foreground)]">
          自动部署 Cloudflare 中继 + 配置微信客服，无需开发经验，3 分钟完成
        </span>
        <span className="mt-1 text-sm font-medium text-indigo-600 group-hover:underline">
          开始一键接入 →
        </span>
      </button>

      {/* Card 2: Manual */}
      <button
        type="button"
        onClick={onManual}
        className="group flex flex-col items-start gap-2 rounded-xl border border-[var(--color-border)] p-5 text-left transition hover:border-indigo-400 hover:shadow-md"
      >
        <span className="text-lg font-semibold text-[var(--color-foreground)]">
          ⚙ 手动配置
        </span>
        <span className="text-sm text-[var(--color-muted-foreground)]">
          已有企业微信客服账号，手动填写 corpid、secret 等
        </span>
        <span className="mt-1 text-sm font-medium text-indigo-600 group-hover:underline">
          手动配置 →
        </span>
      </button>

      {/* Info */}
      <div className="rounded-lg bg-gray-50 p-4 text-[13px] leading-relaxed text-gray-600">
        📖 什么是微信客服？用户在微信扫码即可与 ClaudeWiki 助手对话，发送链接/文本投喂知识，用{" "}
        <code className="rounded bg-gray-200 px-1 text-xs">?</code>{" "}
        前缀提问查询。
      </div>
    </div>
  );
}

// ── Pipeline step ───────────────────────────────────────────────────

function StepPipeline({ onBack, onSuccess }: { onBack: () => void; onSuccess: () => void }) {
  const [skipCfRegister, setSkipCfRegister] = useState(false);
  const [cfApiToken, setCfApiToken] = useState("");
  const [started, setStarted] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);

  const startMut = useMutation({
    mutationFn: () =>
      startKefuPipeline({
        skip_cf_register: skipCfRegister,
        cf_api_token: cfApiToken || undefined,
      }),
    onSuccess: () => setStarted(true),
  });

  const cancelMut = useMutation({ mutationFn: cancelKefuPipeline });

  const pipelineQuery = useQuery({
    queryKey: kefuQueryKeys.pipeline(),
    queryFn: getKefuPipelineStatus,
    enabled: started,
    refetchInterval: started ? 2000 : false,
  });

  const pipeline = pipelineQuery.data as KefuPipelineState | undefined;

  // Auto-transition to success
  useEffect(() => {
    if (!pipeline) return;
    const allDone = pipeline.phases.length > 0 && pipeline.phases.every((p) => p.status === "done" || p.status === "skipped");
    if (allDone && pipeline.contact_url) {
      const timer = setTimeout(() => onSuccess(), 1500);
      return () => clearTimeout(timer);
    }
  }, [pipeline, onSuccess]);

  // Auto-scroll logs
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [pipeline?.logs.length]);

  const hasQr =
    pipeline?.qr_data &&
    pipeline.phases.some((p) => p.status === "waiting_scan") &&
    isSafeQrDataUrl(pipeline.qr_data);

  return (
    <div className="flex flex-col gap-4 p-6">
      {/* Back */}
      <button
        type="button"
        onClick={onBack}
        className="self-start text-sm text-[var(--color-muted-foreground)] hover:text-[var(--color-foreground)]"
      >
        ◀ 返回
      </button>

      <h3 className="text-base font-semibold text-[var(--color-foreground)]">
        一键接入微信客服
      </h3>

      {/* Options before start */}
      {!started && (
        <div className="flex flex-col gap-3">
          <label className="flex items-center gap-2 text-sm text-[var(--color-foreground)]">
            <input
              type="checkbox"
              checked={skipCfRegister}
              onChange={(e) => setSkipCfRegister(e.target.checked)}
              className="size-4 rounded border-gray-300"
            />
            已有 Cloudflare 账号，跳过注册
          </label>

          {skipCfRegister && (
            <input
              type="text"
              placeholder="Cloudflare API Token（可选）"
              value={cfApiToken}
              onChange={(e) => setCfApiToken(e.target.value)}
              className="rounded-lg border border-[var(--color-border)] bg-transparent px-3 py-2 text-sm text-[var(--color-foreground)] placeholder:text-gray-400 focus:border-indigo-500 focus:outline-none"
            />
          )}

          <button
            type="button"
            disabled={startMut.isPending}
            onClick={() => startMut.mutate()}
            className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition hover:bg-indigo-700 disabled:opacity-50"
          >
            {startMut.isPending ? "启动中..." : "开始一键接入"}
          </button>

          {startMut.isError && (
            <p className="text-sm text-red-600">
              启动失败: {String((startMut.error as Error)?.message ?? startMut.error)}
            </p>
          )}
        </div>
      )}

      {/* Phase progress */}
      {started && pipeline && (
        <>
          <ul className="flex flex-col gap-2">
            {pipeline.phases.map((phase, idx) => (
              <li key={phase.phase} className="flex items-center gap-2 text-sm">
                <PhaseStatusIcon status={phase.status} />
                <span
                  className={
                    phase.status === "running" || phase.status === "waiting_scan"
                      ? "animate-pulse font-medium text-indigo-600"
                      : phase.status === "done"
                        ? "text-green-600"
                        : phase.status === "failed"
                          ? "text-red-600"
                          : "text-[var(--color-muted-foreground)]"
                  }
                >
                  {PHASE_NUMBERS[idx] ?? "○"} {PHASE_LABELS[phase.phase] ?? phase.phase}
                </span>
                {phase.message && (
                  <span className="ml-auto truncate text-xs text-gray-400">
                    {phase.message}
                  </span>
                )}
                {phase.error && (
                  <span className="ml-auto truncate text-xs text-red-500">
                    {phase.error}
                  </span>
                )}
              </li>
            ))}
          </ul>

          {/* QR code */}
          {hasQr && (
            <div className="flex flex-col items-center gap-2 rounded-lg border border-amber-200 bg-amber-50 p-4">
              <p className="text-sm font-medium text-amber-700">请使用微信扫描下方二维码</p>
              <img
                src={pipeline.qr_data!}
                alt="扫码授权"
                className="size-48 rounded-lg"
              />
            </div>
          )}

          {/* Logs */}
          {pipeline.logs.length > 0 && (
            <div className="max-h-40 overflow-y-auto rounded-lg bg-gray-900 p-3">
              <pre className="whitespace-pre-wrap text-xs leading-relaxed text-gray-300">
                {pipeline.logs.join("\n")}
              </pre>
              <div ref={logEndRef} />
            </div>
          )}

          {/* Cancel */}
          {pipeline.active && (
            <button
              type="button"
              disabled={cancelMut.isPending}
              onClick={() => cancelMut.mutate()}
              className="self-start rounded-lg border border-red-300 px-4 py-1.5 text-sm text-red-600 transition hover:bg-red-50 disabled:opacity-50"
            >
              {cancelMut.isPending ? "取消中..." : "取消"}
            </button>
          )}
        </>
      )}
    </div>
  );
}

function PhaseStatusIcon({ status }: { status: PipelinePhaseState["status"] }) {
  switch (status) {
    case "done":
    case "skipped":
      return <span className="text-green-600">✓</span>;
    case "running":
    case "waiting_scan":
      return <span className="text-indigo-600">◎</span>;
    case "failed":
      return <span className="text-red-600">✕</span>;
    default:
      return <span className="text-gray-400">○</span>;
  }
}

// ── Manual config step ──────────────────────────────────────────────

function StepManual({ onBack, onSuccess }: { onBack: () => void; onSuccess: () => void }) {
  const [corpid, setCorpid] = useState("");
  const [secret, setSecret] = useState("");
  const [token, setToken] = useState("");
  const [aesKey, setAesKey] = useState("");
  const [relayUrl, setRelayUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const valid = corpid.trim() !== "" && secret.trim() !== "" && (aesKey === "" || aesKey.length === 43);

  async function handleSubmit() {
    if (!valid) return;
    setError(null);
    setSubmitting(true);
    try {
      const config: KefuConfigRequest = {
        corpid: corpid.trim(),
        secret: secret.trim(),
        token: token.trim(),
        encoding_aes_key: aesKey.trim(),
      };
      await saveKefuConfig(config);
      await createKefuAccount();
      await startKefuMonitor();
      onSuccess();
    } catch (err) {
      setError(String((err as Error)?.message ?? err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="flex flex-col gap-4 p-6">
      {/* Back */}
      <button
        type="button"
        onClick={onBack}
        className="self-start text-sm text-[var(--color-muted-foreground)] hover:text-[var(--color-foreground)]"
      >
        ◀ 返回
      </button>

      <h3 className="text-base font-semibold text-[var(--color-foreground)]">
        手动配置微信客服
      </h3>

      <div className="flex flex-col gap-3">
        <FormField label="企业 ID (corpid)" required>
          <input
            type="text"
            value={corpid}
            onChange={(e) => setCorpid(e.target.value)}
            placeholder="ww..."
            className="w-full rounded-lg border border-[var(--color-border)] bg-transparent px-3 py-2 text-sm text-[var(--color-foreground)] placeholder:text-gray-400 focus:border-indigo-500 focus:outline-none"
          />
        </FormField>

        <FormField label="Secret" required>
          <input
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            className="w-full rounded-lg border border-[var(--color-border)] bg-transparent px-3 py-2 text-sm text-[var(--color-foreground)] placeholder:text-gray-400 focus:border-indigo-500 focus:outline-none"
          />
        </FormField>

        <FormField label="Token">
          <input
            type="text"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            className="w-full rounded-lg border border-[var(--color-border)] bg-transparent px-3 py-2 text-sm text-[var(--color-foreground)] placeholder:text-gray-400 focus:border-indigo-500 focus:outline-none"
          />
        </FormField>

        <FormField
          label="EncodingAESKey"
          hint={aesKey !== "" && aesKey.length !== 43 ? "必须为 43 个字符" : undefined}
        >
          <input
            type="text"
            value={aesKey}
            onChange={(e) => setAesKey(e.target.value)}
            className={`w-full rounded-lg border bg-transparent px-3 py-2 text-sm text-[var(--color-foreground)] placeholder:text-gray-400 focus:outline-none ${
              aesKey !== "" && aesKey.length !== 43
                ? "border-red-400 focus:border-red-500"
                : "border-[var(--color-border)] focus:border-indigo-500"
            }`}
          />
        </FormField>

        <FormField label="中继 URL（可选）">
          <input
            type="text"
            value={relayUrl}
            onChange={(e) => setRelayUrl(e.target.value)}
            placeholder="https://..."
            className="w-full rounded-lg border border-[var(--color-border)] bg-transparent px-3 py-2 text-sm text-[var(--color-foreground)] placeholder:text-gray-400 focus:border-indigo-500 focus:outline-none"
          />
        </FormField>
      </div>

      <button
        type="button"
        disabled={!valid || submitting}
        onClick={handleSubmit}
        className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition hover:bg-indigo-700 disabled:opacity-50"
      >
        {submitting ? "保存中..." : "保存并启动"}
      </button>

      {error && (
        <p className="text-sm text-red-600">{error}</p>
      )}

      <a
        href="https://developer.work.weixin.qq.com/document/path/94638"
        target="_blank"
        rel="noopener noreferrer"
        className="text-sm text-indigo-600 hover:underline"
      >
        📖 如何获取这些信息？→ 查看教程
      </a>
    </div>
  );
}

function FormField({
  label,
  required,
  hint,
  children,
}: {
  label: string;
  required?: boolean;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-sm font-medium text-[var(--color-foreground)]">
        {label}
        {required && <span className="ml-0.5 text-red-500">*</span>}
      </span>
      {children}
      {hint && <span className="text-xs text-red-500">{hint}</span>}
    </label>
  );
}

// ── Success step ────────────────────────────────────────────────────

function StepSuccess({ onDone }: { onDone: () => void }) {
  const queryClient = useQueryClient();

  const contactQuery = useQuery({
    queryKey: kefuQueryKeys.contactUrl(),
    queryFn: getKefuContactUrl,
  });

  const contactUrl = contactQuery.data?.url ?? null;

  function handleDone() {
    void queryClient.invalidateQueries({ queryKey: kefuQueryKeys.status() });
    void queryClient.invalidateQueries({ queryKey: kefuQueryKeys.config() });
    onDone();
  }

  return (
    <div className="flex flex-col items-center gap-5 p-8">
      {/* Success icon */}
      <div className="flex size-16 items-center justify-center rounded-full bg-green-100 text-3xl">
        ✅
      </div>

      <h3 className="text-lg font-semibold text-[var(--color-foreground)]">
        微信客服已连接
      </h3>

      {/* QR code */}
      {contactUrl && (
        <img
          src={`https://api.qrserver.com/v1/create-qr-code/?size=200x200&data=${encodeURIComponent(contactUrl)}`}
          alt="客服二维码"
          className="size-48 rounded-lg border border-[var(--color-border)]"
        />
      )}

      {/* Info rows */}
      <div className="w-full rounded-lg border border-[var(--color-border)] p-4 text-sm">
        <div className="flex justify-between py-1">
          <span className="text-[var(--color-muted-foreground)]">客服名称</span>
          <span className="font-medium text-[var(--color-foreground)]">ClaudeWiki 助手</span>
        </div>
        {contactUrl && (
          <div className="flex justify-between py-1">
            <span className="text-[var(--color-muted-foreground)]">中继服务器</span>
            <span className="max-w-[220px] truncate font-mono text-xs text-[var(--color-foreground)]">
              {contactUrl}
            </span>
          </div>
        )}
        <div className="flex justify-between py-1">
          <span className="text-[var(--color-muted-foreground)]">监听状态</span>
          <span className="font-medium text-green-600">● 运行中</span>
        </div>
      </div>

      {/* Usage tips */}
      <div className="w-full rounded-lg bg-gray-50 p-4 text-[13px] leading-relaxed text-gray-600">
        <p className="mb-1 font-medium text-gray-700">使用提示：</p>
        <ul className="flex flex-col gap-0.5">
          <li>发送链接 → 自动入库</li>
          <li>发送文本 → 记录笔记</li>
          <li>
            <code className="rounded bg-gray-200 px-1 text-xs">?</code>提问 → 查询知识库
          </li>
          <li>
            <code className="rounded bg-gray-200 px-1 text-xs">/recent</code> → 最近摄入
          </li>
          <li>
            <code className="rounded bg-gray-200 px-1 text-xs">/stats</code> → 知识统计
          </li>
        </ul>
      </div>

      <button
        type="button"
        onClick={handleDone}
        className="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition hover:bg-indigo-700"
      >
        完成
      </button>
    </div>
  );
}

// ── Main modal ──────────────────────────────────────────────────────

export function ConnectWeChatModal() {
  const open = useSettingsStore((s) => s.connectWeChatModalOpen);
  const close = useSettingsStore((s) => s.closeConnectWeChatModal);
  const [step, setStep] = useState<ModalStep>("choose");

  // Reset step whenever the modal closes
  useEffect(() => {
    if (!open) {
      setStep("choose");
    }
  }, [open]);

  const goSuccess = useCallback(() => setStep("success"), []);

  return (
    <Dialog.Root open={open} onOpenChange={(o) => { if (!o) close(); }}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/40 data-[state=open]:animate-fade-in" />
        <Dialog.Content
          className="fixed left-1/2 top-1/2 z-50 flex max-h-[85vh] w-full max-w-lg -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl bg-white shadow-xl data-[state=open]:animate-fade-in"
        >
          {/* Header */}
          <div className="flex h-12 shrink-0 items-center justify-between border-b border-gray-200 px-4">
            <Dialog.Title className="text-[14px] font-semibold text-gray-900">
              接入微信客服
            </Dialog.Title>
            <Dialog.Close className="rounded-md p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600">
              <X className="size-4" />
            </Dialog.Close>
          </div>

          {/* Body */}
          <div className="flex-1 overflow-y-auto">
            {step === "choose" && (
              <StepChoose
                onPipeline={() => setStep("pipeline")}
                onManual={() => setStep("manual")}
              />
            )}
            {step === "pipeline" && (
              <StepPipeline onBack={() => setStep("choose")} onSuccess={goSuccess} />
            )}
            {step === "manual" && (
              <StepManual onBack={() => setStep("choose")} onSuccess={goSuccess} />
            )}
            {step === "success" && <StepSuccess onDone={close} />}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

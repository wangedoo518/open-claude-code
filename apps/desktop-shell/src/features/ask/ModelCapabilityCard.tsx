import { Check, X } from "lucide-react";

import { cn } from "@/lib/utils";
import {
  formatContextWindow,
  type ModelCapability,
} from "./model-capabilities";

interface Props {
  modelId: string;
  providerLabel?: string;
  capability: ModelCapability;
  className?: string;
}

const VERIFICATION_LABEL: Record<ModelCapability["verificationStatus"], string> = {
  verified: "已验证",
  documented: "文档确认",
  untested: "未验证",
  broken: "不可用",
};

export function ModelCapabilityCard({
  modelId,
  providerLabel,
  capability,
  className,
}: Props) {
  const rows = [
    {
      label: "文本流式",
      active: capability.chat,
      desc: "普通问答与连续输出",
    },
    {
      label: "工具调用",
      active: capability.tools,
      desc: capability.tools ? "可以执行外部工具" : "当前后端路径不可用",
    },
    {
      label: "流式工具",
      active: capability.streamingTools,
      desc: "工具调用过程可实时显示",
    },
    {
      label: "并行工具",
      active: capability.parallelTools,
      desc: "可同时执行多个工具",
    },
    {
      label: "推理模型",
      active: capability.reasoning,
      desc: "支持 reasoning / thinking",
    },
    {
      label: "视觉输入",
      active: capability.vision,
      desc: "可理解图片输入",
    },
    {
      label: "结构化输出",
      active: capability.structuredOutput,
      desc: "支持 JSON / schema 输出",
    },
  ];

  return (
    <div
      className={cn(
        "w-[280px] rounded-lg border border-[rgba(44,44,42,0.14)] bg-white px-3.5 py-3 text-left text-[11px] text-[#5F5E5A] shadow-[0_12px_36px_rgba(44,44,42,0.08)]",
        className,
      )}
    >
      <div className="mb-2 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-[12px] font-medium text-[#2C2C2A]">
            {modelId || "未知模型"}
          </div>
          {providerLabel ? (
            <div className="truncate text-[10px] text-[#888780]">
              {providerLabel}
            </div>
          ) : null}
        </div>
        <span className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-[#888780] ring-1 ring-[rgba(44,44,42,0.08)]">
          {VERIFICATION_LABEL[capability.verificationStatus]}
        </span>
      </div>

      <div className="space-y-1.5">
        {rows.map((row) => (
          <div key={row.label} className="flex items-start gap-2">
            <span
              className={cn(
                "mt-0.5 inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full",
                row.active
                  ? "bg-[#E1F5EE] text-[#0F6E56]"
                  : "bg-[#F1EFE8] text-[#888780]",
              )}
            >
              {row.active ? (
                <Check className="h-2.5 w-2.5" />
              ) : (
                <X className="h-2.5 w-2.5" />
              )}
            </span>
            <div className="min-w-0">
              <div
                className={cn(
                  "text-[11px]",
                  row.active ? "text-[#2C2C2A]" : "text-[#888780]",
                )}
              >
                {row.label}
              </div>
              <div className="text-[10px] leading-snug text-[#888780]">
                {row.desc}
              </div>
            </div>
          </div>
        ))}
      </div>

      <div className="mt-2 border-t border-[rgba(44,44,42,0.08)] pt-2 text-[10px] leading-relaxed text-[#888780]">
        上下文约 {formatContextWindow(capability.contextWindow)}
        {capability.note ? ` · ${capability.note}` : null}
      </div>
    </div>
  );
}

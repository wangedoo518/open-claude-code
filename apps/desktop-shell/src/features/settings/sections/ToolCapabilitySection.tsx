import { CheckCircle2, Info, XCircle } from "lucide-react";
import { SettingGroup } from "../components/SettingGroup";

const ENABLED_TOOLS = [
  // TODO(Step 7.1): bind this read-only display to the backend ToolExposurePolicy.
  {
    label: "联网搜索",
    detail: "WebSearch",
  },
  {
    label: "网页抓取",
    detail: "WebFetch",
  },
  {
    label: "文件读取",
    detail: "read_file, glob_search, grep_search",
  },
];

const DISABLED_TOOLS = [
  {
    label: "文件写入",
    detail: "write_file, edit_file",
    reason: "避免误操作改动本地文件",
  },
  {
    label: "Shell 执行",
    detail: "bash, PowerShell",
    reason: "避免破坏性命令或环境污染",
  },
  {
    label: "Agent 协作",
    detail: "Agent, TeamCreate",
    reason: "仅限 Anthropic agentic 路径",
  },
];

export function ToolCapabilitySection() {
  return (
    <SettingGroup
      title="工具能力"
      description="当前 AI 服务可调用的工具范围。这里只展示策略，开关会在后续版本接入。"
    >
      <div className="space-y-4">
        <div>
          <div className="mb-2 text-[12px] font-medium text-muted-foreground">
            当前允许使用
          </div>
          <div className="grid gap-2">
            {ENABLED_TOOLS.map((tool) => (
              <ToolCapabilityRow
                key={tool.label}
                tone="enabled"
                label={tool.label}
                detail={tool.detail}
              />
            ))}
          </div>
        </div>

        <div>
          <div className="mb-2 text-[12px] font-medium text-muted-foreground">
            当前禁止使用
          </div>
          <div className="grid gap-2">
            {DISABLED_TOOLS.map((tool) => (
              <ToolCapabilityRow
                key={tool.label}
                tone="disabled"
                label={tool.label}
                detail={tool.detail}
                reason={tool.reason}
              />
            ))}
          </div>
        </div>

        <div className="flex items-start gap-2 rounded-lg bg-[rgba(44,44,42,0.025)] px-3 py-2 text-[12px] leading-5 text-muted-foreground">
          <Info className="mt-0.5 size-3.5 shrink-0" strokeWidth={1.6} />
          <div>
            当前策略适合大多数知识管理场景：允许搜索、抓取和读取，默认禁止写入与执行命令。
            未来版本会在这里提供自定义工具能力开关。
          </div>
        </div>
      </div>
    </SettingGroup>
  );
}

function ToolCapabilityRow({
  tone,
  label,
  detail,
  reason,
}: {
  tone: "enabled" | "disabled";
  label: string;
  detail: string;
  reason?: string;
}) {
  const enabled = tone === "enabled";
  const Icon = enabled ? CheckCircle2 : XCircle;

  return (
    <div className="flex items-start gap-3 rounded-lg border border-border/45 bg-card px-3 py-2.5">
      <div
        className={
          enabled
            ? "mt-0.5 flex size-6 items-center justify-center rounded-md bg-[#E1F5EE] text-[#0F6E56]"
            : "mt-0.5 flex size-6 items-center justify-center rounded-md bg-[#F1EFE8] text-[#888780]"
        }
      >
        <Icon className="size-3.5" strokeWidth={1.8} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[13px] font-medium text-foreground">
            {label}
          </span>
          <code className="rounded bg-[rgba(44,44,42,0.05)] px-1.5 py-0.5 text-[11px] text-muted-foreground">
            {detail}
          </code>
        </div>
        {reason ? (
          <div className="mt-1 text-[12px] text-muted-foreground">
            原因：{reason}
          </div>
        ) : null}
      </div>
    </div>
  );
}

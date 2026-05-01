/**
 * ShellInspector — Slice 45 global right-side context panel.
 *
 * Spec §9.3 calls for a single shell-level Inspector that replaces
 * per-page asides over time. This first cut ships the three-mode
 * scaffolding (Inspector / Agent / Activity) and a working Activity
 * mode wired to `/api/wiki/git/audit`. Inspector and Agent are
 * intentional placeholders pointing users at the per-page Inspectors
 * (Slice 40, Slice 42) and the future agent surface; the goal of this
 * slice is to land the shell slot + mode switcher so subsequent slices
 * can flesh out modes incrementally without touching `ClawWikiShell`.
 *
 * Default visible on lg viewports; collapses below 1280px so routes
 * with their own right column (Inbox, Knowledge article) keep the same
 * footprint they had pre-Slice 45.
 */

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  Bot,
  ClipboardList,
  History,
  Info,
} from "lucide-react";
import { useLocation } from "react-router-dom";
import { getVaultGitAudit } from "@/api/wiki/repository";

type InspectorMode = "inspector" | "agent" | "activity";

const MODE_TABS: ReadonlyArray<{
  id: InspectorMode;
  label: string;
  icon: typeof ClipboardList;
}> = [
  { id: "inspector", label: "Inspector", icon: ClipboardList },
  { id: "agent", label: "Agent", icon: Bot },
  { id: "activity", label: "Activity", icon: Activity },
];

export function ShellInspector() {
  const [mode, setMode] = useState<InspectorMode>("activity");

  return (
    <aside className="shell-inspector" aria-label="Shell context inspector">
      <div className="shell-inspector-tabs" role="tablist">
        {MODE_TABS.map((tab) => {
          const Icon = tab.icon;
          const active = mode === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={active}
              data-active={active}
              onClick={() => setMode(tab.id)}
              className="shell-inspector-tab"
              data-mode={tab.id}
            >
              <Icon className="size-3" strokeWidth={1.5} aria-hidden />
              <span>{tab.label}</span>
            </button>
          );
        })}
      </div>
      <div className="shell-inspector-body" role="tabpanel">
        {mode === "inspector" && <InspectorMode />}
        {mode === "agent" && <AgentMode />}
        {mode === "activity" && <ActivityMode />}
      </div>
    </aside>
  );
}

function InspectorMode() {
  const location = useLocation();
  const route = useMemo(() => describeRoute(location.pathname), [
    location.pathname,
  ]);
  return (
    <div className="shell-inspector-section">
      <div className="shell-inspector-empty">
        <Info className="size-3.5" strokeWidth={1.5} aria-hidden />
        <p>当前页面：{route}</p>
        <p className="shell-inspector-hint">
          页面级 Inspector 仍然挂在路由内（Inbox 选中行 / Knowledge 文章页）。
          全局上下文面板会在后续切片接入页面 metadata。
        </p>
      </div>
    </div>
  );
}

function AgentMode() {
  return (
    <div className="shell-inspector-section">
      <div className="shell-inspector-empty">
        <Bot className="size-3.5" strokeWidth={1.5} aria-hidden />
        <p>Agent 维护操作（占位）</p>
        <p className="shell-inspector-hint">
          后续切片会在此提供 “摘要/写作/巡检” 一键操作，对应当前选中的上下文。
        </p>
      </div>
    </div>
  );
}

function ActivityMode() {
  const auditQuery = useQuery({
    queryKey: ["wiki", "git", "audit", { limit: 8, scope: "shell" }] as const,
    queryFn: () => getVaultGitAudit(8),
    staleTime: 30_000,
  });
  const entries = auditQuery.data?.entries ?? [];

  return (
    <div className="shell-inspector-section">
      <div className="shell-inspector-section-head">
        <History className="size-3" strokeWidth={1.5} aria-hidden />
        <span>最近 Vault 活动</span>
      </div>
      {auditQuery.isLoading ? (
        <p className="shell-inspector-hint">加载中…</p>
      ) : entries.length === 0 ? (
        <p className="shell-inspector-hint">暂无 Vault 操作记录</p>
      ) : (
        <ul className="shell-inspector-activity">
          {entries.map((entry, idx) => (
            <li key={idx} className="shell-inspector-activity-row">
              <span className="shell-inspector-activity-op">
                {entry.operation}
              </span>
              {entry.path ? (
                <span className="shell-inspector-activity-path">
                  {entry.path}
                </span>
              ) : entry.commit ? (
                <span className="shell-inspector-activity-path font-mono">
                  {entry.commit}
                </span>
              ) : null}
              <span className="shell-inspector-activity-time">
                {formatRel(entry.timestamp_ms)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function describeRoute(pathname: string): string {
  if (pathname === "/" || pathname === "/dashboard") return "首页 / Pulse";
  if (pathname.startsWith("/ask")) return "Ask";
  if (pathname.startsWith("/inbox")) return "Inbox";
  if (pathname.startsWith("/wiki")) return "Knowledge";
  if (pathname.startsWith("/rules")) return "Rules Studio";
  if (pathname.startsWith("/connections")) return "Connections";
  if (pathname.startsWith("/settings")) return "Settings";
  return pathname;
}

function formatRel(ms: number | null | undefined): string {
  if (!ms) return "";
  const diff = Date.now() - ms;
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return `${days}d`;
}

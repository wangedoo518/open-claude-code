/**
 * InboxInspector — Slice 40 right-side context column.
 *
 * Spec §7.2 calls for a dedicated Inspector pane that surfaces, for the
 * currently-focused inbox entry, "目的、来源、lineage、schema violations、
 * 推荐动作" — distinct from the §1/§2/§3 Maintainer Workbench in the
 * middle. This component is purely additive: it reads from the already-
 * fetched `IntelligentEntry` and reuses `InboxLineageSummary` for the
 * upstream/downstream block. No new server endpoint, no extra fetches
 * beyond the existing lineage query.
 *
 * Empty state: when no entry is focused (or the user just dismissed the
 * deep-link), render a muted hint instead of an empty pane so the column
 * keeps a stable footprint.
 */

import { useMemo } from "react";
import { Link } from "react-router-dom";
import {
  CircleDot,
  FileText,
  GitBranch,
  Tag,
} from "lucide-react";

import type { InboxEntry } from "@/api/wiki/types";
import type { IngestDecision } from "@/lib/tauri";
import type { QueueIntelligence } from "@/features/inbox/queue-intelligence";
import { RecommendedActionBadge } from "@/features/inbox/components/RecommendedActionBadge";
import { InboxLineageSummary } from "@/features/inbox/components/InboxLineageSummary";

export type InboxInspectorEntry = InboxEntry & {
  intelligence: QueueIntelligence;
  decision: IngestDecision | null;
};

export interface InboxInspectorProps {
  entry: InboxInspectorEntry | null;
}

const KIND_LABEL: Record<string, string> = {
  "new-raw": "新素材",
  "update-existing": "更新建议",
  "wechat-article": "微信文章",
  "url-ingest": "URL 摄入",
};

const STATUS_LABEL: Record<string, string> = {
  pending: "待处理",
  approved: "已批准",
  rejected: "已拒绝",
  proposed: "已提案",
  resolved: "已应用",
};

function formatRelative(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return null;
  const diffMs = Date.now() - then;
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;
  const months = Math.floor(days / 30);
  return `${months} 个月前`;
}

export function InboxInspector({ entry }: InboxInspectorProps) {
  if (!entry) {
    return (
      <div
        className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center text-muted-foreground/60"
        style={{ fontSize: 11 }}
      >
        <CircleDot className="size-4 opacity-40" aria-hidden />
        <p>选择一条任务以查看 Inspector</p>
        <p className="text-muted-foreground/40">
          这里会显示推荐动作、来源、目标、lineage 等元信息
        </p>
      </div>
    );
  }

  const intel = entry.intelligence;
  const sourceRawLabel =
    entry.source_raw_id != null
      ? `raw #${String(entry.source_raw_id).padStart(5, "0")}`
      : null;
  const kindLabel = KIND_LABEL[entry.kind] ?? entry.kind;
  const statusLabel = STATUS_LABEL[entry.status] ?? entry.status;
  const proposalStatusLabel =
    entry.proposal_status != null
      ? STATUS_LABEL[entry.proposal_status] ?? entry.proposal_status
      : null;
  const createdRelative = useMemo(
    () => formatRelative(entry.created_at),
    [entry.created_at],
  );
  const resolvedRelative = useMemo(
    () => formatRelative(entry.resolved_at ?? null),
    [entry.resolved_at],
  );

  return (
    <div
      className="flex h-full flex-col gap-3 overflow-y-auto px-4 py-3"
      data-inbox-inspector
    >
      {/* Header */}
      <div className="flex items-center gap-2">
        <span
          className="font-mono uppercase tracking-widest text-muted-foreground/60"
          style={{ fontSize: 10 }}
        >
          Inspector
        </span>
        <span
          className="ml-auto rounded-full border border-border/40 px-1.5 py-0.5 text-muted-foreground/60"
          style={{ fontSize: 10 }}
          title="任务 id"
        >
          #{entry.id}
        </span>
      </div>

      {/* §推荐动作 */}
      <Section icon={Tag} label="推荐动作">
        <div className="flex flex-col gap-1.5">
          <RecommendedActionBadge action={intel.recommended_action} />
          {intel.why_long ? (
            <p
              className="text-muted-foreground"
              style={{ fontSize: 11, lineHeight: 1.5 }}
            >
              {intel.why_long}
            </p>
          ) : null}
          <p
            className="text-muted-foreground/50"
            style={{ fontSize: 10 }}
          >
            评分 {intel.score} · {intel.reason_code}
          </p>
        </div>
      </Section>

      {/* §来源 */}
      <Section icon={FileText} label="来源">
        <ul className="space-y-1" style={{ fontSize: 11 }}>
          <li className="flex items-center gap-1.5 text-foreground/85">
            <span className="text-muted-foreground/60">类型</span>
            <span>{kindLabel}</span>
          </li>
          {sourceRawLabel ? (
            <li className="flex items-center gap-1.5 text-foreground/85">
              <span className="text-muted-foreground/60">raw 引用</span>
              <Link
                to={`/raw?focus=${entry.source_raw_id}`}
                className="text-primary hover:underline"
              >
                {sourceRawLabel}
              </Link>
            </li>
          ) : (
            <li className="text-muted-foreground/50">暂无 raw 引用</li>
          )}
        </ul>
      </Section>

      {/* §目标 */}
      <Section icon={GitBranch} label="目标">
        {entry.target_page_slug ? (
          <p style={{ fontSize: 11 }} className="text-foreground/85">
            <span className="text-muted-foreground/60">已锁定 · </span>
            <Link
              to={`/wiki/${entry.target_page_slug}`}
              className="text-primary hover:underline"
            >
              {entry.target_page_slug}
            </Link>
          </p>
        ) : entry.proposed_wiki_slug ? (
          <p style={{ fontSize: 11 }} className="text-foreground/85">
            <span className="text-muted-foreground/60">建议 slug · </span>
            <span className="font-mono text-foreground">
              {entry.proposed_wiki_slug}
            </span>
          </p>
        ) : (
          <p style={{ fontSize: 11 }} className="text-muted-foreground/50">
            暂未确定目标页 · 在 §2 Maintain 中选择
          </p>
        )}
      </Section>

      {/* §状态 */}
      <Section icon={CircleDot} label="状态">
        <div className="flex flex-wrap items-center gap-1.5" style={{ fontSize: 11 }}>
          <span
            className="rounded-full border border-border/40 px-1.5 py-0.5 text-foreground/85"
            style={{ fontSize: 10 }}
          >
            {statusLabel}
          </span>
          {proposalStatusLabel ? (
            <span
              className="rounded-full border border-border/40 px-1.5 py-0.5 text-foreground/85"
              style={{ fontSize: 10 }}
              title="proposal 状态"
            >
              提案 · {proposalStatusLabel}
            </span>
          ) : null}
        </div>
        <p
          className="mt-1.5 text-muted-foreground/60"
          style={{ fontSize: 10 }}
        >
          {createdRelative ? `入队 ${createdRelative}` : null}
          {createdRelative && resolvedRelative ? " · " : null}
          {resolvedRelative ? `解决 ${resolvedRelative}` : null}
        </p>
      </Section>

      {/* §Lineage */}
      <Section icon={GitBranch} label="Lineage">
        <InboxLineageSummary entryId={entry.id} />
      </Section>

      {/* §Schema */}
      <Section icon={CircleDot} label="Schema 校验">
        <p
          className="text-muted-foreground/60"
          style={{ fontSize: 11, lineHeight: 1.5 }}
        >
          {entry.proposal_status === "pending"
            ? "提案待审 · 详细 schema 错误见 §3 结果"
            : "尚未生成提案 · 完成 §2 Maintain 后会执行 schema validation"}
        </p>
      </Section>
    </div>
  );
}

interface SectionProps {
  icon: React.ComponentType<{ className?: string; "aria-hidden"?: boolean }>;
  label: string;
  children: React.ReactNode;
}

function Section({ icon: Icon, label, children }: SectionProps) {
  return (
    <section className="flex flex-col gap-1.5">
      <div className="flex items-center gap-1 text-muted-foreground/60">
        <Icon className="size-3" aria-hidden />
        <span
          className="font-mono uppercase tracking-widest"
          style={{ fontSize: 10 }}
        >
          {label}
        </span>
      </div>
      <div className="pl-4">{children}</div>
    </section>
  );
}

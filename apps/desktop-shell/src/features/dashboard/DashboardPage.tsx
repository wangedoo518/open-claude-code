/**
 * Dashboard · 你的外脑 Home
 *
 * DS1.1 visual rewrite. The data sources are unchanged:
 *
 *   GET /api/wiki/raw            — total + today's ingest count
 *   GET /api/wiki/stats          — wiki_count, week_new_pages, knowledge_velocity
 *   GET /api/wiki/inbox          — pending_count, total_count
 *   GET /api/wiki/stats/absorb-log  — recent maintainer activity
 *   GET /api/wiki/stats/patrol-report — advanced (collapsed)
 *   GET /api/desktop/bootstrap   — feature capabilities (broker gated on this)
 *   GET /api/broker/status       — optional Codex broker state
 *   GET /api/desktop/wechat-kefu/status — WeChat ingest channel state
 *
 * DS1.1 layout mirrors `ClawWiki Design System/ui_kits/desktop-shell-v2/Home.jsx`:
 *
 *   1. ds-home-hero — center-aligned serif 你的外脑 + tagline
 *   2. ds-skill-row — 4 pastel skill/action cards (问 / 待整理 / 知识库 / 微信)
 *   3. ds-today-block — "今天可以处理 N" header + compact recent activity list
 *   4. <details> 高级信息 — broker / patrol buried below the fold
 *
 * What's DIFFERENT vs DS1:
 *   - No more Codex 令牌池 / 今日入库 stat-card row at the top of the
 *     first-fold; those were developer-console artifacts. Stats still
 *     fetch (real data preserved) but render as a slim secondary row
 *     below the today block — not the hero's visual peer.
 *   - No 查看关系图 default button — users reach Graph via Knowledge Hub.
 *   - Patrol Summary stays behind an `<details>` (same as DS1 did).
 *
 * Nothing new is fetched. Every query here was already live pre-DS1.1.
 */

import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  Loader2,
  MessageCircle,
  FileStack,
  ServerCog,
  Brain,
  BookOpen,
  Link2,
  Inbox as InboxIcon,
  ArrowRight,
  Sparkles,
} from "lucide-react";
import {
  listRawEntries,
  listInboxEntries,
  getWikiStats,
  getAbsorbLog,
  getPatrolReport,
  triggerPatrol,
} from "@/features/ingest/persist";
import { getBootstrap, getKefuStatus } from "@/features/settings/api/client";
import { getBrokerStatus } from "@/features/settings/api/private-cloud";

const dashboardKeys = {
  bootstrap: () => ["desktop", "bootstrap"] as const,
  raw: () => ["wiki", "raw", "list"] as const,
  broker: () => ["broker", "status"] as const,
  inbox: () => ["wiki", "inbox", "list"] as const,
  stats: () => ["wiki", "stats"] as const,
  wechatKefu: () => ["wechat-kefu", "status"] as const,
};

export function DashboardPage() {
  const bootstrapQuery = useQuery({
    queryKey: dashboardKeys.bootstrap(),
    queryFn: getBootstrap,
    staleTime: 60_000,
  });
  const rawQuery = useQuery({
    queryKey: dashboardKeys.raw(),
    queryFn: () => listRawEntries(),
    staleTime: 15_000,
  });
  const privateCloudEnabled = bootstrapQuery.data?.private_cloud_enabled === true;
  const brokerQuery = useQuery({
    queryKey: dashboardKeys.broker(),
    queryFn: () => getBrokerStatus(),
    staleTime: 15_000,
    enabled: privateCloudEnabled,
  });
  const inboxQuery = useQuery({
    queryKey: dashboardKeys.inbox(),
    queryFn: () => listInboxEntries(),
    staleTime: 15_000,
  });
  const statsQuery = useQuery({
    queryKey: dashboardKeys.stats(),
    queryFn: () => getWikiStats(),
    staleTime: 15_000,
  });
  const absorbLogQuery = useQuery({
    queryKey: [...dashboardKeys.stats(), "absorb-log"],
    queryFn: () => getAbsorbLog(10),
    staleTime: 15_000,
  });
  const patrolQuery = useQuery({
    queryKey: [...dashboardKeys.stats(), "patrol-report"],
    queryFn: () => getPatrolReport(),
    staleTime: 60_000,
  });
  const kefuStatusQuery = useQuery({
    queryKey: dashboardKeys.wechatKefu(),
    queryFn: () => getKefuStatus(),
    staleTime: 30_000,
    retry: false,
  });

  const todayDate = formatLocalDate(new Date());
  const rawEntries = rawQuery.data?.entries ?? [];
  const totalIngests = rawEntries.length;
  const todaysIngests =
    statsQuery.data?.today_ingest_count ??
    rawEntries.filter((e) => e.date === todayDate).length;
  const pendingInbox = inboxQuery.data?.pending_count ?? 0;
  const weekNew = statsQuery.data?.week_new_pages ?? 0;
  const wikiCount = statsQuery.data?.wiki_count ?? 0;
  const brokerError =
    privateCloudEnabled && brokerQuery.error instanceof Error
      ? brokerQuery.error.message
      : null;

  // Warm WeChat status sub-copy, derived from real backend state.
  const kefuSub =
    kefuStatusQuery.data?.configured && kefuStatusQuery.data?.account_created
      ? "已接入，可转发内容"
      : kefuStatusQuery.data?.configured
        ? "已配置，等待创建账号"
        : "尚未连接";

  return (
    <div className="ds-canvas flex h-full flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto pb-12">
        {/* 1) Hero — center-aligned serif greet + tagline */}
        <section className="ds-home-hero">
          <div className="ds-greet">
            <span className="ds-greet-underline">你的外脑</span>
          </div>
          <p className="ds-tagline">
            把微信里值得留下的内容，整理成可以追问的知识库。
          </p>
        </section>

        {/* 2) Skill row — 4 pastel action cards. Order matches v2 kit. */}
        <section className="ds-skill-row">
          <SkillCard
            variant="c1"
            title="问一个问题"
            sub="让 AI 基于你喂的内容回答"
            icon={MessageCircle}
            to="/ask"
          />
          <SkillCard
            variant="c2"
            title={pendingInbox > 0 ? `待整理 ${pendingInbox} 条` : "待整理"}
            sub={
              pendingInbox > 0
                ? "Maintainer 给了几个建议，帮我判断一下怎么归"
                : "暂时没有新提议"
            }
            icon={InboxIcon}
            to="/inbox"
          />
          <SkillCard
            variant="c3"
            title="打开知识库"
            sub="浏览已整理的页面、关系图、素材"
            icon={BookOpen}
            to="/wiki"
          />
          <SkillCard
            variant="c4"
            title="连接微信"
            sub={kefuSub}
            icon={Link2}
            to="/wechat"
          />
        </section>

        {/* 3) Today block — "今天可以处理 N" + recent maintainer activity */}
        <section className="ds-today-block">
          <div className="ds-today-head">
            <span className="ds-today-title">今天的动态</span>
            <span className="ds-today-count">
              共 {todaysIngests + pendingInbox} 条
            </span>
            <div className="ml-auto text-[12px] text-muted-foreground">
              <Link
                to="/wiki?view=raw"
                className="inline-flex items-center gap-1 hover:text-foreground"
              >
                查看素材库
                <ArrowRight className="size-3" strokeWidth={1.5} />
              </Link>
            </div>
          </div>

          <ActivityFeed
            isLoading={absorbLogQuery.isLoading}
            error={absorbLogQuery.error}
            entries={absorbLogQuery.data?.entries ?? []}
            emptyHint={
              todaysIngests > 0
                ? `今天已入库 ${todaysIngests} 条素材，还没生成新的整理动作。`
                : "还没有今日动态。转发一条内容到微信，让 Maintainer 开始整理。"
            }
          />
        </section>

        {/* 4) Slim stats row — demoted from first-fold. Real numbers,
             muted presentation. Uses grid so it stays one-line on wide
             screens and wraps gracefully on narrow ones. */}
        <section className="mx-auto mt-10 max-w-[1040px] px-6">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <SlimStat
              icon={FileStack}
              label="今日入库"
              value={rawQuery.isLoading ? "…" : String(todaysIngests)}
              hint={`共 ${totalIngests} 条`}
              to="/wiki?view=raw"
            />
            <SlimStat
              icon={Brain}
              label="本周新增"
              value={statsQuery.isLoading ? "…" : String(weekNew)}
              hint={`共 ${wikiCount} 页`}
              to="/wiki"
            />
            <SlimStat
              icon={InboxIcon}
              label="待审阅"
              value={inboxQuery.isLoading ? "…" : String(pendingInbox)}
              hint={
                inboxQuery.error
                  ? "加载失败"
                  : `共 ${inboxQuery.data?.total_count ?? 0} 条任务`
              }
              to="/inbox"
              warn={!!inboxQuery.error}
            />
          </div>
        </section>

        {/* 5) Advanced · Patrol Summary collapsed by default */}
        <details className="group mx-auto mt-8 max-w-[1040px] px-6">
          <summary className="flex cursor-pointer items-center gap-2 rounded-md border border-border/40 px-4 py-2.5 text-[11px] text-muted-foreground transition-colors hover:bg-accent/40">
            <Sparkles className="size-3.5" strokeWidth={1.5} />
            <span className="font-semibold uppercase tracking-widest">
              高级
            </span>
            <span className="ml-auto text-muted-foreground/60 group-open:hidden">
              展开
            </span>
            <span className="ml-auto hidden text-muted-foreground/60 group-open:inline">
              收起
            </span>
          </summary>
          <div className="mt-2 rounded-md border border-border/40 px-4 py-3">
            <div className="mb-2 flex items-center justify-between">
              <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/60">
                知识质量
              </h3>
              <button
                onClick={() => triggerPatrol().then(() => patrolQuery.refetch())}
                className="rounded px-2 py-0.5 text-[11px] text-primary hover:bg-primary/10 transition-colors"
              >
                立即巡检
              </button>
            </div>
            {patrolQuery.data ? (
              <div className="flex flex-wrap gap-2">
                {patrolQuery.data.summary.schema_violations > 0 && (
                  <span
                    className="rounded-full px-2 py-0.5 text-[10px]"
                    style={{
                      backgroundColor:
                        "color-mix(in srgb, var(--color-destructive) 10%, transparent)",
                      color: "var(--color-destructive)",
                    }}
                  >
                    {patrolQuery.data.summary.schema_violations} schema 违规
                  </span>
                )}
                {patrolQuery.data.summary.orphans > 0 && (
                  <span
                    className="rounded-full px-2 py-0.5 text-[10px]"
                    style={{
                      backgroundColor:
                        "color-mix(in srgb, var(--color-warning) 12%, transparent)",
                      color: "var(--color-warning)",
                    }}
                  >
                    {patrolQuery.data.summary.orphans} 孤儿页
                  </span>
                )}
                {patrolQuery.data.summary.stubs > 0 && (
                  <span
                    className="rounded-full px-2 py-0.5 text-[10px]"
                    style={{
                      backgroundColor:
                        "color-mix(in srgb, var(--color-warning) 12%, transparent)",
                      color: "var(--color-warning)",
                    }}
                  >
                    {patrolQuery.data.summary.stubs} stub
                  </span>
                )}
                {patrolQuery.data.summary.stale > 0 && (
                  <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">
                    {patrolQuery.data.summary.stale} 过期
                  </span>
                )}
                {patrolQuery.data.summary.oversized > 0 && (
                  <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">
                    {patrolQuery.data.summary.oversized} 超长
                  </span>
                )}
                {Object.values(patrolQuery.data.summary).every((v) => v === 0) && (
                  <span
                    className="text-[11px]"
                    style={{ color: "var(--color-success)" }}
                  >
                    全部通过
                  </span>
                )}
                <span className="text-[10px] text-muted-foreground/50">
                  {patrolQuery.data.checked_at.slice(0, 10)}
                </span>
              </div>
            ) : (
              <p className="text-[11px] text-muted-foreground/50">尚未运行巡检</p>
            )}
          </div>
        </details>

        {/* Broker error FYI — only if private cloud is enabled AND it errored */}
        {privateCloudEnabled && brokerError && (
          <section className="mx-auto mt-6 max-w-[1040px] px-6">
            <div
              className="rounded-md border px-4 py-2 text-[11px]"
              style={{
                borderColor:
                  "color-mix(in srgb, var(--color-error) 30%, transparent)",
                backgroundColor:
                  "color-mix(in srgb, var(--color-error) 4%, transparent)",
                color: "var(--color-error)",
              }}
            >
              <ServerCog className="mr-1 inline size-3 align-[-2px]" />
              私有云代理不可达 ·
              <Link to="/settings" className="ml-1 underline">
                打开设置排查 →
              </Link>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

/* ─── Skill card (DS1.1) ────────────────────────────────────────── */

function SkillCard({
  variant,
  title,
  sub,
  icon: Icon,
  to,
}: {
  variant: "c1" | "c2" | "c3" | "c4" | "c5";
  title: string;
  sub: string;
  icon: typeof MessageCircle;
  to: string;
}) {
  return (
    <Link to={to} className={`ds-skill-card ds-skill-${variant} animate-fade-in`}>
      <Icon className="size-5" strokeWidth={1.5} aria-hidden="true" />
      <div className="ds-skill-title">{title}</div>
      <div className="ds-skill-sub">{sub}</div>
      <span className="ds-skill-ill">
        <ArrowRight className="size-5" strokeWidth={1.5} />
      </span>
    </Link>
  );
}

/* ─── Slim stat (DS1.1) ─────────────────────────────────────────── */

function SlimStat({
  icon: Icon,
  label,
  value,
  hint,
  to,
  warn,
}: {
  icon: typeof FileStack;
  label: string;
  value: string;
  hint: string;
  to: string;
  warn?: boolean;
}) {
  const body = (
    <div
      className="flex items-center gap-3 rounded-lg border border-border/50 bg-card px-4 py-3 shadow-warm-ring transition-shadow hover:shadow-warm-ring-hover"
      style={warn ? { borderLeft: "3px solid var(--color-error)" } : undefined}
    >
      <Icon
        className="size-4 shrink-0 text-muted-foreground"
        strokeWidth={1.5}
      />
      <div className="min-w-0 flex-1">
        <div
          className="text-[10.5px] font-semibold uppercase tracking-widest text-muted-foreground/70"
        >
          {label}
        </div>
        <div className="flex items-baseline gap-2">
          <span
            className="tabular-nums text-foreground"
            style={{ fontSize: 18, fontWeight: 600 }}
          >
            {value}
          </span>
          <span className="text-[11px] text-muted-foreground/60">{hint}</span>
        </div>
      </div>
      <ArrowRight
        className="size-3.5 shrink-0 text-muted-foreground/50"
        strokeWidth={1.5}
      />
    </div>
  );
  return <Link to={to}>{body}</Link>;
}

/* ─── Activity feed (compact, replaces old RecentEntries block) ── */

function ActivityFeed({
  isLoading,
  error,
  entries,
  emptyHint,
}: {
  isLoading: boolean;
  error: Error | null;
  entries: ReadonlyArray<{
    timestamp: string;
    entry_id: number;
    action: string;
    page_title?: string | null;
    page_slug?: string | null;
    page_category?: string | null;
  }>;
  emptyHint: string;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center gap-2 rounded-md border border-border/40 bg-card px-4 py-3 text-[12px] text-muted-foreground">
        <Loader2 className="size-3 animate-spin" strokeWidth={1.5} />
        正在加载今日动态…
      </div>
    );
  }
  if (error) {
    return (
      <div
        className="rounded-md border px-4 py-3 text-[12px]"
        style={{
          borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor:
            "color-mix(in srgb, var(--color-error) 4%, transparent)",
          color: "var(--color-error)",
        }}
      >
        加载失败：{error.message}
      </div>
    );
  }
  if (entries.length === 0) {
    return (
      <div className="rounded-md border border-border/40 bg-card px-5 py-6 text-center text-[12px] text-muted-foreground/70">
        {emptyHint}
      </div>
    );
  }
  return (
    <ul className="divide-y divide-border/40 rounded-lg border border-border/40 bg-card">
      {entries.slice(0, 6).map((entry, i) => (
        <li
          key={`${entry.entry_id}-${i}`}
          className="flex items-center gap-3 px-4 py-2.5"
        >
          <span
            className="shrink-0 text-[10.5px] font-mono text-muted-foreground/60"
            style={{ minWidth: 40 }}
          >
            {entry.timestamp.slice(11, 16)}
          </span>
          <span
            className="shrink-0 text-[11px]"
            style={
              entry.action === "create"
                ? { color: "var(--color-success)" }
                : entry.action === "update"
                  ? { color: "var(--color-primary)" }
                  : { color: "var(--color-muted-foreground)" }
            }
          >
            {entry.action === "create"
              ? "新建"
              : entry.action === "update"
                ? "更新"
                : "跳过"}
          </span>
          <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">
            {entry.page_title ?? entry.page_slug ?? `素材 #${entry.entry_id}`}
          </span>
          {entry.page_category && (
            <span className="shrink-0 rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
              {entry.page_category}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}

/* ─── formatLocalDate (unchanged) ───────────────────────────────── */

function formatLocalDate(d: Date): string {
  const y = d.getFullYear().toString().padStart(4, "0");
  const m = (d.getMonth() + 1).toString().padStart(2, "0");
  const day = d.getDate().toString().padStart(2, "0");
  return `${y}-${m}-${day}`;
}

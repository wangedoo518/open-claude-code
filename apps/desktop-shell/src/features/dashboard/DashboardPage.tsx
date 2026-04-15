/**
 * Dashboard · 你的外脑主页
 *
 * S3 real implementation. The canonical §5 wireframe shows six
 * regions: logo strip, today's ingest count, optional runtime-status
 * card, maintainer "pages touched today", pending Inbox, and a QuickAsk
 * composer. S3 wires the first four to live data; QuickAsk is
 * reduced to a "Start a conversation" button that jumps to `/ask`
 * (MVP — the full inline composer lands after S4 when the Ask
 * runtime supports "one-shot sessions" as described in D19).
 *
 * Data sources:
 *   GET /api/wiki/raw             — total ingest count + today's new
 *   GET /api/desktop/bootstrap    — feature capabilities
 *   GET /api/broker/status        — private-cloud pool stats (optional)
 *
 * The page is intentionally LIGHT on logic. Anything that looks like
 * a real statistic (maintenance digest, inbox unread, etc.) lights
 * up in S4 once the maintainer + Inbox sprints add the backend
 * endpoints. Until then those cards render a gentle "—" so users
 * aren't misled by zero values that mean "unknown" rather than "none".
 */

import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  Loader2,
  MessageCircle,
  FileStack,
  ServerCog,
  Brain,
  Inbox as InboxIcon,
  ArrowRight,
} from "lucide-react";
import { listRawEntries, listInboxEntries, getWikiStats, getAbsorbLog, getPatrolReport, triggerPatrol } from "@/features/ingest/persist";
// useSettingsStore / useWikiTabStore available for future Quick Action routing.
import { getBootstrap } from "@/features/settings/api/client";
import { getBrokerStatus } from "@/features/settings/api/private-cloud";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const dashboardKeys = {
  bootstrap: () => ["desktop", "bootstrap"] as const,
  raw: () => ["wiki", "raw", "list"] as const,
  broker: () => ["broker", "status"] as const,
  inbox: () => ["wiki", "inbox", "list"] as const,
  wikiPages: () => ["wiki", "pages", "list"] as const,
  stats: () => ["wiki", "stats"] as const,
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

  const privateCloudEnabled =
    bootstrapQuery.data?.private_cloud_enabled === true;

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

  // wikiQuery removed in v2 — stat cards now use statsQuery.data.wiki_count.

  // v2: WikiStats from the new /api/wiki/stats endpoint.
  const statsQuery = useQuery({
    queryKey: dashboardKeys.stats(),
    queryFn: () => getWikiStats(),
    staleTime: 15_000,
  });

  // v2: ActivityFeed + PatrolSummary data.
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

  // Derive "today's new ingests" on the client so we don't need a
  // dedicated backend endpoint during S3. `entry.date` is the
  // ISO `YYYY-MM-DD` from the filename; comparing against the
  // local-time today is fine because ingests happen on the same
  // machine as the frontend.
  const todayDate = formatLocalDate(new Date());
  const rawEntries = rawQuery.data?.entries ?? [];
  const totalIngests = rawEntries.length;
  const todaysIngests = statsQuery.data?.today_ingest_count
    ?? rawEntries.filter((e) => e.date === todayDate).length;

  const brokerStatus = privateCloudEnabled ? brokerQuery.data : undefined;
  const brokerError =
    privateCloudEnabled && brokerQuery.error instanceof Error
      ? brokerQuery.error.message
      : null;

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      {/* Hero — 07-dashboard.md §6.1 */}
      <section className="border-b border-border/50 px-8 py-6">
        <h1
          className="text-3xl font-medium text-foreground"
          style={{ fontFamily: 'var(--font-family-dt-serif, "Lora", "Songti SC", Georgia, serif)' }}
        >
          你的外脑
        </h1>
        <p className="mt-1 text-muted-foreground/60" style={{ fontSize: 11 }}>
          {statsQuery.data
            ? `${statsQuery.data.wiki_count} 篇知识页面 · 知识速率 ${statsQuery.data.knowledge_velocity.toFixed(1)} 页/天`
            : "加载中..."}
        </p>
      </section>

      {/* Stat cards */}
      <section
        className={cn(
          "grid grid-cols-2 gap-3 px-8 py-4",
          privateCloudEnabled ? "md:grid-cols-4" : "md:grid-cols-3"
        )}
      >
        <StatCard
          icon={FileStack}
          label="今日入库"
          value={rawQuery.isLoading ? "…" : String(todaysIngests)}
          hint={`共 ${totalIngests} 条`}
          tint="var(--color-success)"
          link="/raw"
        />
        {privateCloudEnabled && (
          <StatCard
            icon={ServerCog}
            label="Codex 令牌池"
            value={
              brokerQuery.isLoading
                ? "…"
                : brokerStatus
                  ? String(brokerStatus.pool_size)
                  : "—"
            }
            hint={
              brokerError
                ? "代理不可达"
                : brokerStatus
                  ? `${brokerStatus.fresh_count} 可用`
                  : "连接中…"
            }
            tint={brokerError ? "var(--color-error)" : "var(--claude-blue)"}
            link="/settings"
          />
        )}
        <StatCard
          icon={Brain}
          label="本周新增"
          value={statsQuery.isLoading ? "…" : String(statsQuery.data?.week_new_pages ?? 0)}
          hint={`共 ${statsQuery.data?.wiki_count ?? 0} 个知识页面`}
          tint="var(--deeptutor-purple, var(--agent-purple))"
          link="/wiki"
        />
        <StatCard
          icon={InboxIcon}
          label="待审阅"
          value={inboxQuery.isLoading ? "…" : String(inboxQuery.data?.pending_count ?? 0)}
          hint={inboxQuery.error ? "加载失败" : `共 ${inboxQuery.data?.total_count ?? 0} 条任务`}
          tint={inboxQuery.error ? "var(--color-error)" : "var(--color-warning)"}
          link="/inbox"
        />
      </section>

      {/* QuickAsk CTA */}
      <section className="px-8 py-3">
        <div className="rounded-md border border-border/40 px-5 py-4">
          <div className="flex items-center justify-between gap-4">
            <div>
              <div className="mb-0.5 flex items-center gap-2 text-foreground" style={{ fontSize: 14, fontWeight: 500 }}>
                <MessageCircle
                  className="size-3.5"
                  style={{ color: "var(--claude-orange)" }}
                />
                问问你的外脑
              </div>
              <p className="text-muted-foreground/60" style={{ fontSize: 11 }}>
                与 AI 对话，探索你的素材库。支持 @raw/ 引用和多轮问答。
              </p>
            </div>
            <Link
              to="/ask"
              className="inline-flex shrink-0 items-center gap-1.5 rounded-md bg-primary px-4 py-2 text-primary-foreground transition-colors hover:bg-primary/90"
              style={{ fontSize: 13, fontWeight: 500 }}
            >
              开始对话
              <ArrowRight className="size-3" />
            </Link>
          </div>
        </div>
      </section>

      {/* Activity Feed — 07-dashboard.md §6.3 */}
      <section className="px-8 py-4">
        <div className="mb-3 flex items-baseline justify-between">
          <h2 className="uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
            最近动态
          </h2>
          <Link
            to="/raw"
            className="text-muted-foreground/50 hover:text-foreground"
            style={{ fontSize: 11 }}
          >
            查看全部 →
          </Link>
        </div>
        {absorbLogQuery.data?.entries && absorbLogQuery.data.entries.length > 0 ? (
          <div className="space-y-1.5">
            {absorbLogQuery.data.entries.slice(0, 8).map((entry, i) => (
              <div
                key={`${entry.entry_id}-${i}`}
                className="flex items-center gap-2 rounded-md px-2 py-1.5 text-[12px] hover:bg-accent/50 transition-colors"
              >
                <span className="text-muted-foreground/60 w-12 shrink-0 text-[11px]">
                  {entry.timestamp.slice(11, 16)}
                </span>
                <span className={
                  entry.action === "create"
                    ? "text-[var(--deeptutor-ok,#3F8F5E)]"
                    : entry.action === "update"
                      ? "text-[var(--color-primary)]"
                      : "text-muted-foreground"
                }>
                  {entry.action === "create" ? "新建" : entry.action === "update" ? "更新" : "跳过"}
                </span>
                <span className="flex-1 truncate text-foreground">
                  {entry.page_title ?? entry.page_slug ?? `raw #${entry.entry_id}`}
                </span>
                {entry.page_category && (
                  <span className="shrink-0 rounded bg-primary/10 px-1.5 py-0.5 text-[9px] text-primary">
                    {entry.page_category}
                  </span>
                )}
              </div>
            ))}
          </div>
        ) : (
          <RecentEntries
            isLoading={rawQuery.isLoading}
            error={rawQuery.error}
            entries={rawEntries.slice(-5).reverse()}
          />
        )}
      </section>

      {/* Patrol Summary — 07-dashboard.md §6.5 */}
      <section className="px-8 pb-6">
        <div className="rounded-md border border-border/40 px-4 py-3">
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
                <span className="rounded-full bg-[var(--color-destructive)]/10 px-2 py-0.5 text-[10px] text-[var(--color-destructive)]">
                  {patrolQuery.data.summary.schema_violations} schema 违规
                </span>
              )}
              {patrolQuery.data.summary.orphans > 0 && (
                <span className="rounded-full bg-[var(--deeptutor-warn,#C88B1A)]/10 px-2 py-0.5 text-[10px] text-[var(--deeptutor-warn,#C88B1A)]">
                  {patrolQuery.data.summary.orphans} 孤儿页
                </span>
              )}
              {patrolQuery.data.summary.stubs > 0 && (
                <span className="rounded-full bg-[var(--deeptutor-warn,#C88B1A)]/10 px-2 py-0.5 text-[10px] text-[var(--deeptutor-warn,#C88B1A)]">
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
                <span className="text-[11px] text-[var(--deeptutor-ok,#3F8F5E)]">
                  全部通过
                </span>
              )}
              <span className="text-[10px] text-muted-foreground/50">
                {patrolQuery.data.checked_at.slice(0, 10)}
              </span>
            </div>
          ) : (
            <p className="text-[11px] text-muted-foreground/50">
              尚未运行巡检
            </p>
          )}
        </div>
      </section>

      {/* Quick Actions — 07-dashboard.md §6.6 */}
      <section className="px-8 pb-6">
        <div className="flex gap-3">
          <Button variant="outline" size="default" asChild>
            <Link to="/inbox">
              <InboxIcon className="size-3.5" /> 开始维护
            </Link>
          </Button>
          <Button variant="outline" size="default" asChild>
            <Link to="/graph">
              查看图谱
            </Link>
          </Button>
          <Button variant="outline" size="default" asChild>
            <Link to="/wiki">
              打开 Wiki
            </Link>
          </Button>
        </div>
      </section>
    </div>
  );
}

function StatCard({
  icon: Icon,
  label,
  value,
  hint,
  tint,
  link,
}: {
  icon: typeof FileStack;
  label: string;
  value: string;
  hint?: string;
  tint?: string;
  link?: string;
}) {
  const body = (
    <div
      className="h-full rounded-xl border bg-card p-6 shadow-warm-ring transition-shadow hover:shadow-warm-ring-hover"
      style={{ borderLeft: `3px solid ${tint ?? "var(--color-border)"}` }}
    >
      <div className="mb-1.5 flex items-center gap-1.5 text-muted-foreground/60" style={{ fontSize: 11, textTransform: "uppercase" as const, letterSpacing: "0.05em" }}>
        <Icon className="size-3" style={tint ? { color: tint } : undefined} />
        {label}
      </div>
      <div
        className="tabular-nums leading-none"
        style={{ fontSize: 18, fontWeight: 600, color: tint ?? "var(--color-foreground)" }}
      >
        {value}
      </div>
      {hint && (
        <div className="mt-1.5 truncate text-muted-foreground/50" style={{ fontSize: 11 }}>
          {hint}
        </div>
      )}
    </div>
  );

  if (link) {
    return (
      <Link to={link} className="group block">
        {body}
      </Link>
    );
  }
  return <div className="group">{body}</div>;
}

function RecentEntries({
  isLoading,
  error,
  entries,
}: {
  isLoading: boolean;
  error: Error | null;
  entries: Array<{
    id: number;
    source: string;
    slug: string;
    date: string;
    byte_size: number;
  }>;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center gap-2 text-caption text-muted-foreground">
        <Loader2 className="size-3 animate-spin" />
        加载中…
      </div>
    );
  }
  if (error) {
    return (
      <div
        className="rounded-md border px-3 py-2 text-caption"
        style={{
          borderColor:
            "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor:
            "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        加载失败：{error.message}
      </div>
    );
  }
  if (entries.length === 0) {
    return (
      <div className="px-1 py-6 text-center text-muted-foreground/60" style={{ fontSize: 11 }}>
        还没有素材。{" "}
        <Link to="/raw" className="text-primary hover:underline">
          粘贴第一条 →
        </Link>
      </div>
    );
  }

  return (
    <ul className="divide-y divide-border/30">
      {entries.map((entry) => (
        <li key={entry.id}>
          <Link
            to="/raw"
            className="flex items-center justify-between px-1 py-2.5 transition-colors hover:bg-accent/50"
          >
            <div className="flex min-w-0 items-baseline gap-3">
              <span className="shrink-0 font-mono text-muted-foreground/40" style={{ fontSize: 11 }}>
                #{String(entry.id).padStart(5, "0")}
              </span>
              <span className="truncate text-foreground" style={{ fontSize: 14 }}>
                {entry.slug}
              </span>
              <span className="shrink-0 text-muted-foreground/50" style={{ fontSize: 11 }}>
                {entry.source}
              </span>
            </div>
            <div className="shrink-0 text-muted-foreground/40" style={{ fontSize: 11 }}>
              {entry.date} · {entry.byte_size} B
            </div>
          </Link>
        </li>
      ))}
    </ul>
  );
}

function formatLocalDate(d: Date): string {
  const y = d.getFullYear().toString().padStart(4, "0");
  const m = (d.getMonth() + 1).toString().padStart(2, "0");
  const day = d.getDate().toString().padStart(2, "0");
  return `${y}-${m}-${day}`;
}

import { useEffect, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { registerPageVitality } from "@/features/ask/citation-tracker";
import {
  AlertTriangle,
  Archive,
  ArrowRight,
  Bot,
  CalendarCheck,
  CheckCircle2,
  ClipboardList,
  FileText,
  GitBranch,
  HeartPulse,
  History,
  Inbox,
  Loader2,
  MessageCircle,
  ShieldCheck,
  TrendingUp,
  type LucideIcon,
} from "lucide-react";
import { Link } from "react-router-dom";
import {
  getExternalAiWritePolicy,
  getVaultGitAudit,
  getVaultGitStatus,
  getPatrolReport,
  getWikiStats,
  listWikiPages,
  listInboxEntries,
} from "@/api/wiki/repository";
import type { VaultGitAuditEntry, WikiPageSummary } from "@/api/wiki/types";
import {
  PURPOSE_LENSES,
  type PurposeLensId,
} from "@/features/purpose/purpose-lenses";
import {
  buildPatrolLifecycleSuggestions,
  buildEntropyPulseSummary,
  countPatrolLifecycleSuggestions,
  type EntropyPulseItem,
  type EntropyPulseSummary,
  type PatrolLifecycleSuggestion,
} from "./entropy-pulse";
import { RoiPulsePanel } from "./RoiPulsePanel";
import type { RoiPageSummary } from "./roi-pulse";

const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const EXPRESSIBLE_CATEGORIES = new Set(["concept", "people", "topic", "compare"]);

/**
 * Slice 47 — Pulse headline lock (spec §6.4.3).
 *
 * Five exclusive states map to five fixed copies. Order matters; the
 * first matching branch wins. Pure helper so other surfaces (status
 * bar tooltip, command palette empty state) can mirror the same
 * vocabulary without re-implementing the conditional.
 */
export interface PulseStateInputs {
  totalRisks: number;
  totalPages: number;
  totalRaw: number;
  vaultDirty: boolean;
}

export function derivePulseHeadline(input: PulseStateInputs): string {
  const { totalRisks, totalPages, totalRaw, vaultDirty } = input;
  if (totalPages === 0 && totalRaw === 0 && totalRisks === 0) {
    return "外脑还在等第一条素材，先粘贴或捕获一条试试";
  }
  if (totalRisks === 0 && vaultDirty) {
    return "外脑很干净，但 Vault 有未提交改动，建议 checkpoint";
  }
  if (totalRisks === 0) {
    return "外脑今天很干净，可以继续 capture / express";
  }
  if (totalRisks > 20) {
    return `外脑队列已经较长（${totalRisks} 项），优先看 Top 3 行动`;
  }
  if (totalRisks <= 5) {
    return `有 ${totalRisks} 项小事可以审一下`;
  }
  return `外脑体检发现 ${totalRisks} 项需要处理`;
}

type PurposeDigestItem = {
  id: PurposeLensId;
  label: string;
  output: string;
  weeklyCount: number;
  readyCount: number;
  expressedCount: number;
  totalCount: number;
  recentPages: Array<Pick<WikiPageSummary, "slug" | "title" | "created_at">>;
};

type RecentExpressionItem = Pick<
  WikiPageSummary,
  "slug" | "title" | "created_at" | "expressed_in"
>;

export function DashboardPage() {
  const statsQuery = useQuery({
    queryKey: ["wiki", "stats", "pulse"],
    queryFn: () => getWikiStats(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });
  const inboxQuery = useQuery({
    queryKey: ["wiki", "inbox", "pulse"],
    queryFn: () => listInboxEntries(),
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
  const patrolQuery = useQuery({
    queryKey: ["wiki", "patrol", "pulse"],
    queryFn: () => getPatrolReport(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });
  const gitQuery = useQuery({
    queryKey: ["wiki", "git", "pulse"],
    queryFn: () => getVaultGitStatus(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const gitAuditQuery = useQuery({
    queryKey: ["wiki", "git", "audit", "pulse"],
    queryFn: () => getVaultGitAudit(1),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const externalAiQuery = useQuery({
    queryKey: ["wiki", "external-ai", "write-policy", "pulse"],
    queryFn: () => getExternalAiWritePolicy(),
    staleTime: 10_000,
    refetchInterval: 20_000,
  });
  const pagesQuery = useQuery({
    queryKey: ["wiki", "pages", "pulse-purpose"],
    queryFn: () => listWikiPages(),
    staleTime: 30_000,
    refetchInterval: 60_000,
  });

  // Slice E15.4: feed page vitality into the Ask citation tracker
  // registry so any wiki citation rendered in UsedSourcesBar can
  // resolve its current vitality (cooling/archived) without a
  // separate fetch. Mirrors Home's existing data dependency.
  useEffect(() => {
    const pages = pagesQuery.data?.pages ?? [];
    for (const page of pages) {
      registerPageVitality(page.slug, { vitality: page.vitality });
    }
  }, [pagesQuery.data?.pages]);

  const pendingInbox = inboxQuery.data?.pending_count ?? 0;
  const stats = statsQuery.data;
  const patrol = patrolQuery.data;
  const git = gitQuery.data;
  const latestGitAudit = gitAuditQuery.data?.entries[0] ?? null;
  const purposeDigest = useMemo(
    () => buildPurposeDigest(pagesQuery.data?.pages ?? []),
    [pagesQuery.data?.pages],
  );
  const sourcedPageCount = useMemo(
    () => (pagesQuery.data?.pages ?? []).filter(hasSourceLineage).length,
    [pagesQuery.data?.pages],
  );
  const missingSourceCount = useMemo(
    () => Math.max(0, (pagesQuery.data?.pages ?? []).length - sourcedPageCount),
    [pagesQuery.data?.pages, sourcedPageCount],
  );
  const recentExpressions = useMemo(
    () => buildRecentExpressions(pagesQuery.data?.pages ?? []),
    [pagesQuery.data?.pages],
  );
  const entropyPulse = useMemo(
    () => buildEntropyPulseSummary(pagesQuery.data?.pages ?? []),
    [pagesQuery.data?.pages],
  );
  const lifecyclePatrolSuggestions = useMemo(
    () => buildPatrolLifecycleSuggestions(patrol, pagesQuery.data?.pages ?? []),
    [pagesQuery.data?.pages, patrol],
  );
  const lifecyclePatrolCount = countPatrolLifecycleSuggestions(patrol);
  const entropyReviewCount = entropyPulse.reviewToday.length;
  const activeExternalAiGrants =
    externalAiQuery.data?.grants.filter((grant) => grant.enabled).length ?? 0;
  const schemaViolations = patrol?.summary.schema_violations ?? 0;
  const stalePages = patrol?.summary.stale ?? 0;
  const orphanPages = patrol?.summary.orphans ?? 0;
  // Slice 51 — fresh-vault onboarding override. When the user has
  // basically nothing in the brain yet (≤ 1 page seed, 0 raw items),
  // the maintenance-flavored Top 3 ("提交 5 个 Vault 改动" / "问外脑")
  // is wrong: there's nothing to maintain and nothing to ask about.
  // Replace with three concrete "how do I put my first thing in"
  // affordances that map 1:1 to the three real ingest entry points.
  // Do not apply this override when Vault is dirty; checkpoint safety
  // remains the top action even for an otherwise empty brain.
  const isFreshVault =
    (pagesQuery.data?.pages?.length ?? 0) <= 1 &&
    (statsQuery.data?.raw_count ?? 0) === 0 &&
    pendingInbox === 0 &&
    git?.dirty !== true;
  const topActions = useMemo(
    () => {
      if (isFreshVault) {
        return [
          {
            label: "连接微信，让它自己流进来",
            href: "/wechat",
            tone: "neutral" as const,
          },
          {
            label: "粘贴一条链接 / 文章",
            href: "/raw?add=url",
            tone: "neutral" as const,
          },
          {
            label: "上传 PDF / Word / Markdown",
            href: "/raw?add=file",
            tone: "neutral" as const,
          },
        ] satisfies Array<{
          label: string;
          href: string;
          tone: "warning" | "neutral";
        }>;
      }
      return [
        pendingInbox > 0
          ? {
              label: `审阅 ${pendingInbox} 条待整理建议`,
              href: "/inbox",
              tone: "warning" as const,
            }
          : null,
        git?.dirty
          ? {
              label: `保存 ${git.changed_count} 个 Vault 改动`,
              href: "/connections#git",
              tone: "warning" as const,
            }
          : null,
        entropyReviewCount > 0
          ? {
              label: `今天只复盘 ${entropyReviewCount} 个高信号主题`,
              href: "/#entropy-pulse",
              tone: "warning" as const,
            }
          : null,
        lifecyclePatrolCount > 0
          ? {
              label: `大浪淘沙 ${lifecyclePatrolCount} 个生命周期建议`,
              href: "/#entropy-pulse",
              tone: "warning" as const,
            }
          : null,
        missingSourceCount > 0
          ? {
              label: `补齐 ${missingSourceCount} 页来源线索`,
              href: "/wiki?source=missing",
              tone: "warning" as const,
            }
          : null,
        schemaViolations > 0
          ? {
              label: `修复 ${schemaViolations} 个 schema 问题`,
              href: "/rules#validation",
              tone: "warning" as const,
            }
          : null,
        stalePages + orphanPages > 0
          ? {
              label: `处理 ${stalePages + orphanPages} 个知识质量风险`,
              href: "/wiki",
              tone: "warning" as const,
            }
          : null,
        {
          label: "问外脑一个问题",
          href: "/ask",
          tone: "neutral" as const,
        },
      ].filter(Boolean).slice(0, 3) as Array<{
        label: string;
        href: string;
        tone: "warning" | "neutral";
      }>;
    },
    [
      git?.changed_count,
      git?.dirty,
      entropyReviewCount,
      isFreshVault,
      lifecyclePatrolCount,
      missingSourceCount,
      orphanPages,
      pendingInbox,
      schemaViolations,
      stalePages,
    ],
  );

  const gitRisk = git?.dirty ? git.changed_count : 0;
  const totalRisks =
    pendingInbox +
    schemaViolations +
    stalePages +
    orphanPages +
    missingSourceCount +
    lifecyclePatrolCount +
    gitRisk;
  const isLoading =
    statsQuery.isLoading ||
    inboxQuery.isLoading ||
    patrolQuery.isLoading ||
    gitQuery.isLoading ||
    pagesQuery.isLoading;
  // Slice 47 — Pulse 5-phrase headline lock (spec §6.4.3). Maps Vault
  // state to one of five fixed copies so the home screen never drifts
  // into ad-hoc strings. Order is exclusive top-down: empty vault →
  // dirty-only → calm → small queue → big queue.
  const totalPages = pagesQuery.data?.pages?.length ?? 0;
  const totalRaw = statsQuery.data?.raw_count ?? 0;

  // Slice E18.3 — derive RoiPageSummary inputs once per pages refresh
  // so RoiPulsePanel doesn't have to know the full WikiPageSummary
  // shape. Keeps the panel pure-data and easy to test.
  //
  // Review I6 — `Date.parse("") || 0` silently substituted epoch 0
  // for malformed dates, which then fell outside every 30/90-day
  // window with no signal. Skip pages with un-parseable timestamps
  // entirely so the funnel reflects real data, not zeroed phantoms.
  const roiPages = useMemo<ReadonlyArray<RoiPageSummary>>(() => {
    const pages = pagesQuery.data?.pages ?? [];
    const out: RoiPageSummary[] = [];
    for (const p of pages) {
      const t = Date.parse(p.created_at);
      if (!Number.isFinite(t)) continue;
      out.push({
        slug: p.slug,
        purpose: p.purpose ?? [],
        expressed_in: p.expressed_in ?? [],
        verdict: p.verdict ?? null,
        vitality: p.vitality ?? null,
        created_at_ms: t,
      });
    }
    return out;
  }, [pagesQuery.data?.pages]);
  const headline = derivePulseHeadline({
    totalRisks,
    totalPages,
    totalRaw,
    vaultDirty: git?.dirty === true,
  });
  const primaryAction = topActions[0] ?? {
    label: "粘贴一条素材",
    href: "/raw",
    tone: "neutral" as const,
  };

  return (
    <main className="min-h-full overflow-y-auto bg-background px-6 py-5 text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-5">
        <header className="border-b border-border/50 pb-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <div className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
                Home / Pulse
              </div>
              <h1 className="mt-2 text-[28px] font-semibold tracking-normal">
                {headline}
              </h1>
              <p className="mt-2 max-w-2xl text-[13px] leading-6 text-muted-foreground">
                Buddy 把今天的摄入、维护、知识质量、Git/Vault 和外部 AI 权限压成一张体检单。
              </p>
            </div>
            <Link
              to={primaryAction.href}
              className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-[13px] font-medium text-primary-foreground"
            >
              {primaryAction.label}
              <ArrowRight className="size-4" />
            </Link>
          </div>
        </header>

        {isLoading && (
          <div className="inline-flex items-center gap-2 text-[12px] text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            正在体检…
          </div>
        )}

        <section className="grid gap-3 lg:grid-cols-3">
          <HealthCard
            icon={Inbox}
            title="待审阅"
            value={pendingInbox}
            unit="条"
            status={pendingInbox > 0 ? "需要处理" : "正常"}
            tone={pendingInbox > 0 ? "warning" : "success"}
            href="/inbox"
          />
          <HealthCard
            icon={ShieldCheck}
            title="知识质量"
            value={schemaViolations + stalePages + orphanPages + lifecyclePatrolCount}
            unit="项"
            status={
              schemaViolations + stalePages + orphanPages + lifecyclePatrolCount > 0
                ? "有风险"
                : "正常"
            }
            tone={
              schemaViolations + stalePages + orphanPages + lifecyclePatrolCount > 0
                ? "warning"
                : "success"
            }
            href="/wiki"
          />
          <HealthCard
            icon={GitBranch}
            title="Buddy Vault / Git"
            value={git?.changed_count ?? 0}
            unit="改动"
            status={gitHealthLabel(git, Boolean(gitQuery.error))}
            tone={
              !git || gitQuery.error || !git.git_available || !git.initialized || git.dirty
                ? "warning"
                : "success"
            }
            href="/connections"
          />
        </section>

        <PurposeWeeklyDigest
          items={purposeDigest.items}
          missingPurposeCount={purposeDigest.missingPurposeCount}
          loading={pagesQuery.isLoading}
        />

        <EntropyPulsePanel
          summary={entropyPulse}
          patrolSuggestions={lifecyclePatrolSuggestions}
          loading={pagesQuery.isLoading}
        />

        <RoiPulsePanel pages={roiPages} rawCount={totalRaw} />

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <HeartPulse className="size-4 text-primary" />
              <h2 className="text-[15px] font-medium">
                {isFreshVault ? "其它入口" : "Top 3 建议动作"}
              </h2>
            </div>
            <div className="mt-4 space-y-2">
              {/*
                Slice 51 audit: hero CTA at the top of the page is
                always `topActions[0]`. Rendering the same item again
                as Top-3 #1 looked like two competing primary entries
                on empty Vault. Skip the first row so the "Top 3" list
                always reads "what else can I do" — never duplicates
                the hero. Numbering offsets accordingly.
              */}
              {topActions.slice(1).map((action, idx) => (
                <Link
                  key={`${action.href}-${action.label}`}
                  to={action.href}
                  className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-background px-3 py-3 text-[13px] transition-colors hover:border-primary/40 hover:bg-muted/30"
                >
                  <span className="flex min-w-0 items-center gap-3">
                    <span
                      className="grid size-6 shrink-0 place-items-center rounded bg-muted text-[11px] text-muted-foreground"
                      aria-hidden="true"
                    >
                      {idx + 2}
                    </span>
                    <span className="truncate">{action.label}</span>
                  </span>
                  <ArrowRight className="size-3.5 shrink-0 text-muted-foreground" />
                </Link>
              ))}
            </div>
          </div>

          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <Bot className="size-4 text-primary" />
              <h2 className="text-[15px] font-medium">外部 AI 权限</h2>
            </div>
            <div className="mt-4 space-y-2 text-[12px] text-muted-foreground">
              <StatusRow label="默认模式" value="只读" />
              <StatusRow
                label="会话授权"
                value={`${countExternalAiGrants(externalAiQuery.data?.grants, "session")} 个`}
              />
              <StatusRow
                label="永久规则"
                value={`${countExternalAiGrants(externalAiQuery.data?.grants, "permanent")} 个`}
              />
              <StatusRow label="写入范围" value="wiki / templates / guidance" />
              <StatusRow
                label="当前写权限"
                value={activeExternalAiGrants > 0 ? `${activeExternalAiGrants} 个授权` : "无"}
              />
              <StatusRow
                label="Git checkpoint"
                value={git?.dirty ? `${git.changed_count} 改动待提交` : "已同步本地历史"}
              />
            </div>
            <div className="mt-5 border-t border-border/60 pt-4">
              <div className="flex items-center gap-2">
                <MessageCircle className="size-4 text-primary" />
                <h2 className="text-[15px] font-medium">最近表达</h2>
              </div>
              <RecentExpressionPulse
                items={recentExpressions}
                loading={pagesQuery.isLoading}
              />
            </div>
            <div className="mt-5 border-t border-border/60 pt-4">
              <div className="flex items-center gap-2">
                <History className="size-4 text-primary" />
                <h2 className="text-[15px] font-medium">最近 Git 操作</h2>
              </div>
              <GitAuditPulse
                entry={latestGitAudit}
                error={Boolean(gitAuditQuery.error)}
                loading={gitAuditQuery.isLoading}
              />
            </div>
          </div>
        </section>

        <section className="grid gap-3 md:grid-cols-5">
          <MiniStat icon={FileText} label="知识页" value={stats?.wiki_count ?? 0} />
          <MiniStat icon={ClipboardList} label="原始素材" value={stats?.raw_count ?? 0} />
          <MiniStat icon={MessageCircle} label="今日摄入" value={stats?.today_ingest_count ?? 0} />
          <MiniStat
            icon={History}
            label="来源可追溯"
            value={sourcedPageCount}
            href="/wiki?source=sourced"
          />
          <MiniStat icon={AlertTriangle} label="Schema 风险" value={schemaViolations} />
        </section>
      </div>
    </main>
  );
}

function buildPurposeDigest(pages: WikiPageSummary[]): {
  items: PurposeDigestItem[];
  missingPurposeCount: number;
} {
  const now = Date.now();
  const byId = new Map<PurposeLensId, PurposeDigestItem>(
    PURPOSE_LENSES.map((lens) => [
      lens.id,
      {
        id: lens.id,
        label: lens.zhLabel,
        output: lens.output,
        weeklyCount: 0,
        readyCount: 0,
        expressedCount: 0,
        totalCount: 0,
        recentPages: [],
      },
    ]),
  );
  let missingPurposeCount = 0;

  for (const page of pages) {
    const purposeIds = (page.purpose ?? []).filter((id): id is PurposeLensId =>
      byId.has(id as PurposeLensId),
    );
    if (!purposeIds.length) {
      missingPurposeCount += 1;
      continue;
    }

    const isRecent = isWithinLastWeek(page.created_at, now);
    const isReady = isExpressiblePage(page);
    const hasExpression = Boolean(page.expressed_in?.length);
    for (const purpose of purposeIds) {
      const item = byId.get(purpose);
      if (!item) continue;
      item.totalCount += 1;
      if (hasExpression) item.expressedCount += 1;
      if (isReady && !hasExpression) item.readyCount += 1;
      if (isRecent) {
        item.weeklyCount += 1;
        item.recentPages.push({
          slug: page.slug,
          title: page.title || page.slug,
          created_at: page.created_at,
        });
      }
    }
  }

  const items = Array.from(byId.values()).map((item) => ({
    ...item,
    recentPages: item.recentPages
      .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
      .slice(0, 2),
  }));

  return { items, missingPurposeCount };
}

function isWithinLastWeek(value: string, now: number): boolean {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return false;
  return timestamp <= now && now - timestamp <= WEEK_MS;
}

function isExpressiblePage(page: WikiPageSummary): boolean {
  if (page.category && EXPRESSIBLE_CATEGORIES.has(page.category)) return true;
  return (page.confidence ?? 0) >= 0.6;
}

function hasSourceLineage(page: WikiPageSummary): boolean {
  return (
    (typeof page.source_raw_id === "number" && page.source_raw_id > 0) ||
    Boolean(page.source_refs?.length)
  );
}

function buildRecentExpressions(pages: WikiPageSummary[]): RecentExpressionItem[] {
  return pages
    .filter((page) => page.expressed_in?.length)
    .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
    .slice(0, 3)
    .map((page) => ({
      slug: page.slug,
      title: page.title || page.slug,
      created_at: page.created_at,
      expressed_in: page.expressed_in,
    }));
}

function PurposeWeeklyDigest({
  items,
  missingPurposeCount,
  loading,
}: {
  items: PurposeDigestItem[];
  missingPurposeCount: number;
  loading: boolean;
}) {
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <HeartPulse className="size-4 text-primary" />
            <h2 className="text-[15px] font-medium">本周目的流动</h2>
          </div>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
            每个 purpose 本周吸收了什么、还能表达什么。
          </p>
        </div>
        <Link
          to="/wiki"
          className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-card px-3 text-[12px] text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground"
        >
          打开 Knowledge
          <ArrowRight className="size-3.5" />
        </Link>
      </div>

      {loading ? (
        <div className="rounded-lg border border-border bg-card px-4 py-4 text-[12px] text-muted-foreground">
          正在读取 purpose lens…
        </div>
      ) : (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {items.map((item) => (
            <Link
              key={item.id}
              to={`/wiki?purpose=${item.id}`}
              className="min-h-[158px] rounded-lg border border-border bg-card px-4 py-4 transition-colors hover:border-primary/40 hover:bg-muted/30"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-[14px] font-medium">{item.label}</div>
                  <div className="mt-1 line-clamp-2 text-[12px] leading-5 text-muted-foreground">
                    {item.output}
                  </div>
                </div>
                <ArrowRight className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
              </div>
              <div className="mt-4 grid grid-cols-3 gap-2 text-[12px]">
                <DigestMetric label="本周吸收" value={item.weeklyCount} />
                <DigestMetric label="可表达" value={item.readyCount} />
                <DigestMetric label="已表达" value={item.expressedCount} />
              </div>
              <div className="mt-3 space-y-1.5">
                {item.recentPages.length ? (
                  item.recentPages.map((page) => (
                    <div
                      key={`${item.id}-${page.slug}`}
                      className="min-w-0 truncate rounded bg-muted/50 px-2 py-1.5 text-[12px] text-muted-foreground"
                    >
                      {page.title}
                    </div>
                  ))
                ) : (
                  <div className="rounded bg-muted/50 px-2 py-1.5 text-[12px] leading-5 text-muted-foreground">
                    本周暂无新增，可从已有 {item.totalCount} 页继续提炼。
                  </div>
                )}
              </div>
            </Link>
          ))}
        </div>
      )}

      {missingPurposeCount > 0 ? (
        <Link
          to="/wiki"
          className="flex items-center justify-between gap-3 rounded-lg border border-[var(--color-warning)]/30 bg-[var(--color-warning)]/10 px-4 py-3 text-[12px] text-foreground"
        >
          <span>
            有 {missingPurposeCount} 页缺少 purpose lens，建议进入 Knowledge 补齐。
          </span>
          <ArrowRight className="size-3.5 shrink-0" />
        </Link>
      ) : null}
    </section>
  );
}

function EntropyPulsePanel({
  summary,
  patrolSuggestions,
  loading,
}: {
  summary: EntropyPulseSummary;
  patrolSuggestions: PatrolLifecycleSuggestion[];
  loading: boolean;
}) {
  return (
    <section id="entropy-pulse" className="space-y-3 scroll-mt-5">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <HeartPulse className="size-4 text-primary" />
            <h2 className="text-[15px] font-medium">减熵脉冲</h2>
          </div>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
            只把高信号、增长和冷却候选提出来；归档是注意力状态，不会删除证据。
          </p>
        </div>
        <div className="grid grid-cols-3 gap-2 text-[12px]">
          <DigestMetric label="高优先级" value={summary.highPriorityCount} />
          <DigestMetric label="到期复盘" value={summary.dueReviewCount} />
          <DigestMetric label="冷却候选" value={summary.coolingCount} />
        </div>
      </div>

      {loading ? (
        <div className="rounded-lg border border-border bg-card px-4 py-4 text-[12px] text-muted-foreground">
          正在读取 priority / vitality 信号…
        </div>
      ) : !summary.hasLifecycleSignals ? (
        <div className="rounded-lg border border-border bg-card px-4 py-4 text-[12px] leading-5 text-muted-foreground">
          还没有 priority / vitality 信号。Buddy 会先保留捕获和来源，不会要求你在入口处手动分类。
        </div>
      ) : (
        <div className="grid gap-3 lg:grid-cols-3">
          <EntropyPulseColumn
            icon={CalendarCheck}
            title="今天只看这些"
            description="高优先级、到期复盘，且尚未表达过的页面。"
            items={summary.reviewToday}
            empty="今天没有必须复盘的高信号主题。"
          />
          <EntropyPulseColumn
            icon={TrendingUp}
            title="正在增长"
            description="反复出现、优先级高，或仍在发芽的主题。"
            items={summary.growing}
            empty="暂时没有增长信号。继续捕获，先观察反复出现的方向。"
          />
          <EntropyPulseColumn
            icon={Archive}
            title="冷却 / 可归档"
            description="适合降噪、合并或放入低关注状态，不代表删除。"
            items={summary.cooling}
            empty="暂时没有冷却候选。"
          />
        </div>
      )}

      {!loading && patrolSuggestions.length ? (
        <div className="rounded-lg border border-border bg-card px-4 py-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <div className="text-[14px] font-medium">Patrol 生命周期建议</div>
              <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
                把高优先级未表达、陈旧火花、冷却页和噪音候选提出来，由 Inbox 审阅后再归档或降级。
              </p>
            </div>
            <Link
              to="/inbox"
              className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-background px-3 text-[12px] text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground"
            >
              去 Inbox
              <ArrowRight className="size-3.5" />
            </Link>
          </div>
          <div className="mt-4 grid gap-2 md:grid-cols-3">
            {patrolSuggestions.map((item) => (
              <PatrolLifecycleRow key={`${item.kind}-${item.slug}`} item={item} />
            ))}
          </div>
        </div>
      ) : null}
    </section>
  );
}

function EntropyPulseColumn({
  icon: Icon,
  title,
  description,
  items,
  empty,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  items: EntropyPulseItem[];
  empty: string;
}) {
  return (
    <div className="min-h-[218px] rounded-lg border border-border bg-card px-4 py-4">
      <div className="flex items-start gap-2">
        <span className="mt-0.5 grid size-7 shrink-0 place-items-center rounded-md bg-muted text-muted-foreground">
          <Icon className="size-3.5" />
        </span>
        <div className="min-w-0">
          <h3 className="text-[14px] font-medium">{title}</h3>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
            {description}
          </p>
        </div>
      </div>
      <div className="mt-4 space-y-2">
        {items.length ? (
          items.map((item) => <EntropyPulseRow key={item.slug} item={item} />)
        ) : (
          <div className="rounded-md bg-muted/50 px-3 py-3 text-[12px] leading-5 text-muted-foreground">
            {empty}
          </div>
        )}
      </div>
    </div>
  );
}

function EntropyPulseRow({ item }: { item: EntropyPulseItem }) {
  return (
    <Link
      to={`/wiki/${item.slug}`}
      className="block rounded-md bg-muted/50 px-3 py-2.5 text-[12px] transition-colors hover:bg-muted"
    >
      <div className="flex items-start justify-between gap-2">
        <span className="min-w-0 truncate text-foreground">{item.title}</span>
        <span className="shrink-0 rounded border border-border/60 bg-background px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
          {item.badge}
        </span>
      </div>
      <div className="mt-1 line-clamp-2 leading-5 text-muted-foreground">
        {item.reason}
      </div>
    </Link>
  );
}

function PatrolLifecycleRow({ item }: { item: PatrolLifecycleSuggestion }) {
  return (
    <Link
      to={`/wiki/${item.slug}`}
      className="block min-h-[128px] rounded-md bg-muted/50 px-3 py-3 text-[12px] transition-colors hover:bg-muted"
    >
      <div className="flex items-start justify-between gap-2">
        <span className="min-w-0 truncate text-foreground">{item.title}</span>
        <span className="shrink-0 rounded border border-border/60 bg-background px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
          {item.badge}
        </span>
      </div>
      <div className="mt-2 line-clamp-2 leading-5 text-muted-foreground">
        {item.description}
      </div>
      <div className="mt-2 line-clamp-2 leading-5 text-muted-foreground">
        {item.suggestedAction}
      </div>
    </Link>
  );
}

function DigestMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-md bg-muted/50 px-2.5 py-2">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-1 text-[18px] font-semibold leading-none">{value}</div>
    </div>
  );
}

function RecentExpressionPulse({
  items,
  loading,
}: {
  items: RecentExpressionItem[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="mt-3 rounded-md bg-muted/50 px-3 py-3 text-[12px] text-muted-foreground">
        正在同步表达记录
      </div>
    );
  }

  if (!items.length) {
    return (
      <div className="mt-3 rounded-md bg-muted/50 px-3 py-3 text-[12px] leading-5 text-muted-foreground">
        暂无 expressed_in 记录。表达后可在页面 frontmatter 标记输出位置。
      </div>
    );
  }

  return (
    <div className="mt-3 space-y-2">
      {items.map((item) => (
        <Link
          key={item.slug}
          to={`/wiki/${item.slug}`}
          className="block rounded-md bg-muted/50 px-3 py-2 text-[12px] transition-colors hover:bg-muted"
        >
          <div className="min-w-0 truncate text-foreground">{item.title}</div>
          <div className="mt-1 min-w-0 truncate font-mono text-[11px] text-muted-foreground">
            {(item.expressed_in ?? []).slice(0, 2).join(" / ")}
          </div>
        </Link>
      ))}
    </div>
  );
}

function countExternalAiGrants(
  grants:
    | Array<{
        level: "session" | "permanent";
        enabled: boolean;
      }>
    | undefined,
  level: "session" | "permanent",
) {
  return grants?.filter((grant) => grant.enabled && grant.level === level).length ?? 0;
}

function gitHealthLabel(
  git:
    | {
        git_available: boolean;
        initialized: boolean;
        dirty: boolean;
        changed_count: number;
      }
    | undefined,
  hasError: boolean,
) {
  if (hasError) return "不可用";
  if (!git) return "检查中";
  if (!git.git_available) return "未安装 Git";
  if (!git.initialized) return "未启用";
  if (git.dirty) return "有未提交";
  return "干净";
}

function HealthCard({
  icon: Icon,
  title,
  value,
  unit,
  status,
  tone,
  href,
}: {
  icon: LucideIcon;
  title: string;
  value: number;
  unit: string;
  status: string;
  tone: "success" | "warning";
  href: string;
}) {
  const toneClass =
    tone === "success"
      ? "text-[var(--color-success)] bg-[var(--color-success)]/10"
      : "text-[var(--color-warning)] bg-[var(--color-warning)]/10";
  return (
    <Link
      to={href}
      className="rounded-lg border border-border bg-card px-4 py-4 transition-colors hover:border-primary/40 hover:bg-muted/30"
    >
      <div className="flex items-center justify-between gap-3">
        <span className="grid size-9 place-items-center rounded-md bg-muted text-muted-foreground">
          <Icon className="size-4" />
        </span>
        <span className={`rounded px-2 py-1 text-[11px] leading-none ${toneClass}`}>
          {status}
        </span>
      </div>
      <div className="mt-5 flex items-end gap-1">
        <span className="text-[30px] font-semibold leading-none">{value}</span>
        <span className="pb-1 text-[12px] text-muted-foreground">{unit}</span>
      </div>
      <div className="mt-2 text-[13px] text-muted-foreground">{title}</div>
    </Link>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md bg-muted/50 px-3 py-2">
      <span>{label}</span>
      <span className="text-foreground">{value}</span>
    </div>
  );
}

function GitAuditPulse({
  entry,
  error,
  loading,
}: {
  entry: VaultGitAuditEntry | null;
  error: boolean;
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="mt-3 rounded-md bg-muted/50 px-3 py-3 text-[12px] text-muted-foreground">
        正在同步 Git 操作记录
      </div>
    );
  }

  if (error) {
    return (
      <div className="mt-3 rounded-md bg-muted/50 px-3 py-3 text-[12px] text-muted-foreground">
        Git 操作记录不可用
      </div>
    );
  }

  if (!entry) {
    return (
      <div className="mt-3 rounded-md bg-muted/50 px-3 py-3 text-[12px] text-muted-foreground">
        暂无 Git 操作
      </div>
    );
  }

  return (
    <div className="mt-3 rounded-md bg-muted/50 px-3 py-3 text-[12px]">
      <div className="flex items-center justify-between gap-3">
        <span className="rounded border border-border/60 bg-background px-2 py-0.5 font-mono text-[11px] text-muted-foreground">
          {gitAuditLabel(entry.operation)}
        </span>
        <span className="shrink-0 text-muted-foreground">
          {formatGitAuditTime(entry.timestamp_ms)}
        </span>
      </div>
      <div className="mt-2 min-w-0 truncate text-foreground">
        {entry.summary}
      </div>
      {entry.path || entry.commit || entry.remote ? (
        <div className="mt-1 min-w-0 truncate font-mono text-[11px] text-muted-foreground">
          {entry.path || entry.commit || entry.remote}
        </div>
      ) : null}
    </div>
  );
}

function gitAuditLabel(operation: string): string {
  if (operation === "discard-hunk") return "hunk";
  if (operation === "discard-path") return "discard";
  if (operation === "commit") return "commit";
  if (operation === "remote") return "remote";
  return operation;
}

function formatGitAuditTime(timestampMs: number): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestampMs));
}

function MiniStat({
  icon: Icon,
  label,
  value,
  href,
}: {
  icon: LucideIcon;
  label: string;
  value: number;
  href?: string;
}) {
  const content = (
    <>
      <div className="flex items-center gap-2 text-[12px] text-muted-foreground">
        <Icon className="size-3.5" />
        <span>{label}</span>
      </div>
      <div className="mt-3 flex items-center gap-2">
        <span className="text-[24px] font-semibold">{value}</span>
        {value === 0 ? (
          <span className="text-[var(--color-success)]">
            <CheckCircle2 className="size-4" />
          </span>
        ) : null}
      </div>
    </>
  );
  const className =
    "rounded-lg border border-border bg-card px-4 py-4" +
    (href ? " transition-colors hover:border-primary/40 hover:bg-muted/30" : "");
  if (href) {
    return (
      <Link to={href} className={className}>
        {content}
      </Link>
    );
  }
  return (
    <div className={className}>
      {content}
    </div>
  );
}

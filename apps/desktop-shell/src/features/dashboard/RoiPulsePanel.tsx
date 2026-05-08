import { useMemo } from "react";
import { TrendingUp, ArrowRightToLine, Sparkle } from "lucide-react";
import {
  computeRoiFunnel,
  computeRoiByPurpose,
  type RoiPageSummary,
} from "./roi-pulse";

/**
 * Slice E18 — ROI Pulse Panel.
 *
 * Sits on Home below the entropy pulse. Translates user data we
 * already have (raw_count, expressed_in, verdict, vitality) into a
 * 30/90 day "投入 → 表达" funnel + per-purpose conversion table.
 *
 * Strict no-input rule: this panel only displays. Verdicts are set
 * via the WikiArticle picker (E17.4); raw counts come from existing
 * ingest paths. Empty Vault renders an honest "需要更多数据"
 * placeholder instead of extrapolating from 0.
 */

export interface RoiPulsePanelProps {
  pages: ReadonlyArray<RoiPageSummary>;
  rawCount: number;
  now?: number;
}

export function RoiPulsePanel({
  pages,
  rawCount,
  now = Date.now(),
}: RoiPulsePanelProps) {
  const funnel30 = useMemo(
    () => computeRoiFunnel({ pages, rawCount, now, windowDays: 30 }),
    [pages, rawCount, now],
  );
  const funnel90 = useMemo(
    () => computeRoiFunnel({ pages, rawCount, now, windowDays: 90 }),
    [pages, rawCount, now],
  );
  const byPurpose = useMemo(
    () => computeRoiByPurpose({ pages, now, windowDays: 30 }),
    [pages, now],
  );

  // Honest empty state — don't extrapolate from 0.
  if (funnel90.wikiCount === 0 && funnel90.rawCount === 0) {
    return (
      <section className="rounded-lg border border-border bg-card px-5 py-5">
        <div className="flex items-center gap-2">
          <TrendingUp className="size-4 text-primary" />
          <h2 className="text-[15px] font-medium">投入回报</h2>
        </div>
        <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
          需要先有 30 天的捕获数据才能算「投入 → 表达」的转化率。
        </p>
      </section>
    );
  }

  return (
    <section className="rounded-lg border border-border bg-card px-5 py-5">
      <div className="flex items-center gap-2">
        <TrendingUp className="size-4 text-primary" />
        <h2 className="text-[15px] font-medium">投入回报</h2>
        <span className="text-[11px] text-muted-foreground">
          (近 30 / 90 天)
        </span>
      </div>
      <p className="mt-2 max-w-2xl text-[12px] leading-5 text-muted-foreground">
        判断「值得继续投入 vs 安心放下」需要看你过去几十天里捕的灵感
        实际产出了多少。下面只是把已有数据汇总，不让你重新填。
      </p>

      <div className="mt-4 grid gap-3 md:grid-cols-2">
        <FunnelCard title="近 30 天" funnel={funnel30} />
        <FunnelCard title="近 90 天" funnel={funnel90} />
      </div>

      {byPurpose.length ? (
        <div className="mt-4">
          <div className="text-[12px] font-medium">每个 purpose 的转化</div>
          <p className="mt-1 text-[11px] text-muted-foreground">
            按 30 天内的页面分布；样本 &lt; 3 的 purpose 不显示，避免噪音。
          </p>
          <div className="mt-2 grid gap-2">
            {byPurpose.map((row) => (
              <PurposeRow key={row.purpose} row={row} />
            ))}
          </div>
        </div>
      ) : null}
    </section>
  );
}

function FunnelCard({
  title,
  funnel,
}: {
  title: string;
  funnel: ReturnType<typeof computeRoiFunnel>;
}) {
  const wikiRate =
    funnel.rawCount > 0 ? funnel.wikiCount / funnel.rawCount : 0;
  const expressedRate =
    funnel.wikiCount > 0 ? funnel.expressedCount / funnel.wikiCount : 0;
  return (
    <div className="rounded-md border border-border/70 bg-background px-3 py-3 text-[12px]">
      <div className="text-[13px] font-medium">{title}</div>
      <div className="mt-2 grid grid-cols-3 items-center gap-2 text-center">
        <Stat label="原料" value={funnel.rawCount} />
        <ArrowRightToLine className="size-3 self-center justify-self-center text-muted-foreground" />
        <Stat label="知识页" value={funnel.wikiCount} />
      </div>
      <div className="mt-2 text-[11px] text-muted-foreground">
        raw → wiki: {(wikiRate * 100).toFixed(0)}% · wiki → expressed:{" "}
        {(expressedRate * 100).toFixed(0)}% (
        {funnel.expressedCount}/{funnel.wikiCount})
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div className="text-[16px] font-medium">{value}</div>
      <div className="text-[10px] text-muted-foreground">{label}</div>
    </div>
  );
}

function PurposeRow({
  row,
}: {
  row: ReturnType<typeof computeRoiByPurpose>[number];
}) {
  const tone =
    row.continueRate > row.letGoRate * 1.5
      ? "growing"
      : row.letGoRate > row.continueRate * 1.5
        ? "cooling"
        : "neutral";
  return (
    <div
      className="flex items-center gap-3 rounded-md border border-border/70 bg-background px-3 py-2 text-[12px]"
      data-tone={tone}
    >
      <span className="w-20 truncate font-medium">{row.purpose}</span>
      <span className="flex-1 text-[11px] text-muted-foreground">
        样本 {row.sample} · continue {(row.continueRate * 100).toFixed(0)}% ·
        let_go {(row.letGoRate * 100).toFixed(0)}% · expressed{" "}
        {(row.expressedRate * 100).toFixed(0)}%
      </span>
      {row.continueSoftCount > 0 ? (
        <span
          className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground"
          title="尚未给出 verdict 但 vitality=growing"
        >
          <Sparkle className="size-3" /> +{row.continueSoftCount} 软信号
        </span>
      ) : null}
    </div>
  );
}

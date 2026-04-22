/**
 * Graph · 知识图谱 — Rowboat 风格力导向可视化
 *
 * 点阵画布 + 毛玻璃节点 + 语义化配色 + 流光边线。
 * 图例和统计已内置于 ForceGraph 组件的浮动面板中，
 * 不再需要右侧 sidebar。
 */

import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Loader2, Network } from "lucide-react";
import { getWikiGraph, listRawEntries } from "@/features/ingest/persist";
import { EmptyState } from "@/components/ui/empty-state";
import { ForceGraph } from "./ForceGraph";
import { navigateToWikiPage } from "@/features/wiki/navigate-helpers";

export function GraphPage() {
  const navigate = useNavigate();
  // G1 sprint — focus mode. When the user lands here via a deep link
  // like `/graph?focus=<slug>` (emitted from the Wiki article page's
  // "open in graph" action, or any other surface), seed the
  // ForceGraph's internal search query with that slug so the node is
  // visually highlighted and its neighbors dimmed on first paint.
  // Falling back to the built-in search pipeline avoids a rewrite of
  // the physics/camera code just to add a focus entry point.
  const [searchParams] = useSearchParams();
  const focusSlug = searchParams.get("focus") ?? undefined;

  const rawQuery = useQuery({
    queryKey: ["wiki", "raw", "list"] as const,
    queryFn: () => listRawEntries(),
    staleTime: 30_000,
  });

  const graphQuery = useQuery({
    queryKey: ["wiki", "graph"] as const,
    queryFn: () => getWikiGraph(),
    staleTime: 30_000,
  });

  const entries = (rawQuery.data?.entries ?? []).filter(e => e.byte_size >= 200);
  const graphData = graphQuery.data;
  const isLoading = rawQuery.isLoading || graphQuery.isLoading;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Hero */}
      <div className="shrink-0 border-b border-border/50 px-6 py-4">
        <h1 className="text-lg text-foreground">
          知识图谱
        </h1>
        <p className="mt-1 text-muted-foreground/60" style={{ fontSize: 11 }}>
          知识的形状。
        </p>
      </div>

      {/* Body: Full-bleed force graph */}
      <div className="relative min-h-0 flex-1 overflow-hidden">
        {isLoading ? (
          <div className="flex h-full items-center justify-center gap-2 text-caption text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            加载中…
          </div>
        ) : rawQuery.error ? (
          <GraphError message={(rawQuery.error as Error).message} />
        ) : !graphData || (entries.length === 0 && graphData.nodes.length === 0) ? (
          <GraphEmpty
            nodeCount={graphData?.nodes.length ?? 0}
            edgeCount={graphData?.edges.length ?? 0}
            onOpenInbox={() => navigate("/inbox")}
            onOpenRaw={() => navigate("/raw")}
          />
        ) : (
          <ForceGraph
            graphData={graphData}
            rawEntries={entries}
            initialSearchQuery={focusSlug}
            onClickConcept={(slug) => {
              // G1 sprint — route the Graph → Wiki handoff through
              // the shared `navigateToWikiPage` helper so the jump
              // carries the "wiki-graph" context discriminator for
              // future per-origin telemetry. Semantically identical
              // to the pre-G1 inline setAppMode + openTab + navigate
              // sequence that used to live here.
              navigateToWikiPage(slug, slug, "wiki-graph");
            }}
            onClickRaw={() => navigate("/raw")}
          />
        )}
      </div>
    </div>
  );
}

function GraphEmpty({
  nodeCount: _nodeCount,
  edgeCount: _edgeCount,
  onOpenInbox: _onOpenInbox,
  onOpenRaw,
}: {
  nodeCount: number;
  edgeCount: number;
  onOpenInbox: () => void;
  onOpenRaw: () => void;
}) {
  // DS1.5 — narrative empty state. The R1-era mechanism explainer
  // (markdown [slug](concepts/xxx.md) syntax + source_raw_id sharing)
  // was a capability matrix; users don't need the ruleset. Two lines
  // of suggestion + a single CTA is enough.
  const navigate =
    typeof window !== "undefined"
      ? (path: string) => {
          window.location.hash = `#${path}`;
        }
      : () => {};
  void _nodeCount;
  void _edgeCount;
  void _onOpenInbox;
  return (
    <div className="flex h-full items-center justify-center">
      <EmptyState
        size="full"
        icon={Network}
        title="你的知识图谱还很新"
        description="问几个问题，关系自然会长出来。"
        primaryAction={{
          label: "打开问问题",
          onClick: () => navigate("/ask"),
        }}
        secondaryAction={{ label: "浏览素材", onClick: onOpenRaw }}
      />
    </div>
  );
}

function GraphError({ message }: { message: string }) {
  return (
    <div className="flex h-full items-center justify-center p-6 text-center">
      <div
        className="max-w-md rounded-md border px-4 py-3 text-caption"
        style={{
          borderColor: "color-mix(in srgb, var(--color-error) 30%, transparent)",
          backgroundColor: "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        加载图谱数据失败：{message}
      </div>
    </div>
  );
}

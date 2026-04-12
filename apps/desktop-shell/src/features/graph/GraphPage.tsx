/**
 * Graph · 你的认知网络
 *
 * S6 MVP implementation. The canonical surface eventually shows a
 * force-directed node+edge render of ALL wiki pages colored by
 * fresh/stale/conflict status. That requires:
 *   1. Real wiki/ pages produced by wiki_maintainer (still on hold
 *      until codex_broker::chat_completion is wired), and
 *   2. A layout algorithm (force-directed + collision detection).
 *
 * S6 MVP ships a DIFFERENT shape that's actually useful today:
 *
 *   - Nodes = raw entries (the only layer currently populated)
 *   - Layout = concentric rings grouped by `source` (paste / url /
 *     wechat-text / ...), deterministic polar coordinates so it
 *     doesn't jitter between refreshes
 *   - Colors tint by source (paste = orange, url = blue,
 *     wechat-text = green, others = muted)
 *   - Click a node → jump to /raw (the real navigation target
 *     lands in S4+ once entries are deep-linkable)
 *
 * Everything is plain SVG with CSS-variable fills; no d3 or
 * react-force-graph dep. Works at any canvas size via viewBox and
 * scales text with the zoom.
 */

import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { ArrowRight, Brain, Loader2, Network } from "lucide-react";
import { getWikiGraph, listRawEntries } from "@/features/ingest/persist";
import { ForceGraph } from "./ForceGraph";

/* Old SVG layout constants removed — ForceGraph handles everything. */

export function GraphPage() {
  const navigate = useNavigate();

  const rawQuery = useQuery({
    queryKey: ["wiki", "raw", "list"] as const,
    queryFn: () => listRawEntries(),
    staleTime: 30_000,
  });

  // feat T: pull the wiki graph data so we can render the
  // concept connections sidebar without rewriting the SVG layout.
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
        <h1
          className="text-foreground"
          style={{ fontSize: 18, fontWeight: 600, fontFamily: "var(--font-serif, Lora, serif)" }}
        >
          知识图谱
        </h1>
        <p className="mt-1 text-muted-foreground/60" style={{ fontSize: 11 }}>
          知识图谱 -- 拖拽节点、滚轮缩放、悬停高亮关联
        </p>
      </div>

      {/* Body: Force graph canvas + concept connections sidebar */}
      <div className="relative flex min-h-0 flex-1 overflow-hidden">
        <div className="relative min-h-0 flex-1 overflow-hidden">
          {isLoading ? (
            <div className="flex h-full items-center justify-center gap-2 text-caption text-muted-foreground">
              <Loader2 className="size-3 animate-spin" />
              加载中…
            </div>
          ) : rawQuery.error ? (
            <GraphError message={(rawQuery.error as Error).message} />
          ) : !graphData || (entries.length === 0 && graphData.nodes.length === 0) ? (
            <GraphEmpty />
          ) : (
            <ForceGraph
              graphData={graphData}
              rawEntries={entries}
              onClickConcept={(slug) => navigate(`/wiki?page=${slug}`)}
              onClickRaw={() => navigate("/raw")}
            />
          )}
        </div>
        <ConceptConnectionsSidebar
          isLoading={graphQuery.isLoading}
          error={(graphQuery.error as Error | null)?.message ?? null}
          data={graphQuery.data ?? null}
          onNavigateRaw={() => navigate("/raw")}
          onNavigateWiki={() => navigate("/wiki")}
        />
      </div>
    </div>
  );
}

/* ─── Concept Connections sidebar (feat T) ──────────────────────── */

function ConceptConnectionsSidebar({
  isLoading,
  error,
  data,
  onNavigateRaw,
  onNavigateWiki,
}: {
  isLoading: boolean;
  error: string | null;
  data: import("@/features/ingest/types").WikiGraphResponse | null;
  onNavigateRaw: () => void;
  onNavigateWiki: () => void;
}) {
  return (
    <aside className="flex w-[280px] shrink-0 flex-col overflow-hidden border-l border-border/50">
      <div className="shrink-0 border-b border-border/50 px-4 py-3">
        <div className="mb-1.5 flex items-center gap-2 uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>
          <Network className="size-3" />
          概念关联
        </div>
        {data ? (
          <div className="text-muted-foreground/40" style={{ fontSize: 11 }}>
            {data.concept_count} 概念 ·{" "}
            {data.edge_count} 关联 ·{" "}
            {data.raw_count} 素材
          </div>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        {isLoading ? (
          <div className="flex items-center gap-2 py-4 text-caption text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            加载中…
          </div>
        ) : error ? (
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
            {error}
          </div>
        ) : !data || data.edges.length === 0 ? (
          <div className="px-2 py-3 text-caption text-muted-foreground">
            暂无概念关联。在{" "}
            <a
              href="#/inbox"
              className="text-primary hover:underline"
            >
              Inbox
            </a>{" "}
            中审批维护提案后，图谱会自动增长。
          </div>
        ) : (
          <ul className="space-y-1.5">
            {data.edges.map((edge, idx) => {
              const fromLabel =
                data.nodes.find((n) => n.id === edge.from)?.label ??
                edge.from;
              const rawToLabel =
                data.nodes.find((n) => n.id === edge.to)?.label ?? edge.to;
              // Translate raw labels like "wechat-text: slug" to friendly Chinese
              const toLabel = friendlyRawLabel(rawToLabel);
              return (
                <li
                  key={`${edge.from}->${edge.to}-${idx}`}
                  className="rounded-md border border-border/30 px-2.5 py-2"
                >
                  <button
                    type="button"
                    onClick={onNavigateWiki}
                    className="block w-full truncate text-left text-foreground hover:underline"
                    style={{
                      fontSize: 13,
                      fontWeight: 400,
                      fontFamily: "var(--font-serif, Lora, serif)",
                    }}
                  >
                    <Brain
                      className="mr-1 inline size-3"
                      style={{ color: "var(--claude-orange)" }}
                    />
                    {fromLabel}
                  </button>
                  <button
                    type="button"
                    onClick={onNavigateRaw}
                    className="mt-0.5 flex w-full items-center gap-1 text-left text-muted-foreground/50 hover:text-foreground"
                    style={{ fontSize: 11 }}
                  >
                    <ArrowRight className="size-3 shrink-0" />
                    <span className="truncate">{toLabel}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </aside>
  );
}

/* ─── Label helper ────────────────────────────────────────────── */

const RAW_SOURCE_CN: Record<string, string> = {
  "wechat-text": "微信消息",
  "wechat-article": "微信文章",
  "wechat-url": "微信链接",
  paste: "粘贴",
  url: "网页",
  voice: "语音",
  image: "图片",
  pdf: "PDF",
  pptx: "PPT",
  docx: "文档",
  video: "视频",
  card: "名片",
  chat: "聊天",
};

/** Turn "wechat-text: wechat-f3ykfhpe" → "微信消息" */
function friendlyRawLabel(label: string): string {
  if (!label.includes(": ")) return label;
  const src = label.split(": ")[0];
  return RAW_SOURCE_CN[src] ?? label;
}

function GraphEmpty() {
  return (
    <div className="flex h-full items-center justify-center p-6 text-center">
      <div className="max-w-sm">
        <Network className="mx-auto mb-2 size-8 opacity-30" />
        <p className="text-body text-muted-foreground">
          你的认知网络还是空的。入库一条素材，第一个节点就会出现。
        </p>
      </div>
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
          backgroundColor:
            "color-mix(in srgb, var(--color-error) 5%, transparent)",
          color: "var(--color-error)",
        }}
      >
        加载图谱数据失败：{message}
      </div>
    </div>
  );
}

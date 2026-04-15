/**
 * WikiTab — main container for the Wiki Explorer (v2 multi-tab browser).
 * Per 02-wiki-explorer.md §6.1 layout:
 *   WikiFileTree (left, 240px) | WikiTabBar + WikiContent (center, flex-1)
 */

import { useQuery } from "@tanstack/react-query";

import { useNavigate } from "react-router-dom";
import { WikiFileTree } from "./WikiFileTree";
import { WikiTabBar } from "./WikiTabBar";
import { WikiArticle } from "./WikiArticle";
import { SkillProgressCard } from "./SkillProgressCard";
import { useWikiTabStore } from "@/state/wiki-tab-store";
import { getWikiIndex, getWikiLog, getWikiGraph, listRawEntries } from "@/features/ingest/persist";
import { ForceGraph } from "@/features/graph/ForceGraph";

/* ── Index/Log special page ────────────────────────────────────── */
function SpecialFilePage({ kind }: { kind: "index" | "log" }) {
  const fetchFn = kind === "index" ? getWikiIndex : getWikiLog;
  const { data, isLoading } = useQuery({
    queryKey: ["wiki", kind],
    queryFn: fetchFn,
    staleTime: 10_000,
  });

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center text-muted-foreground">
        Loading...
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[720px] px-8 py-6">
      <h1
        className="mb-4 text-[24px] leading-[1.3] text-foreground"
        style={{ fontFamily: 'var(--font-family-dt-serif, "Lora", serif)' }}
      >
        {kind === "index" ? "Wiki" : "Changelog"}
      </h1>
      <pre className="whitespace-pre-wrap text-[14px] leading-[1.6] text-foreground">
        {data?.content ?? "No content yet."}
      </pre>
    </div>
  );
}

/* ── Embedded Graph (ForceGraph inside WikiTab) ──────────────── */
function EmbeddedGraph() {
  const navigate = useNavigate();
  const openTab = useWikiTabStore((s) => s.openTab);

  const graphQuery = useQuery({
    queryKey: ["wiki", "graph"],
    queryFn: getWikiGraph,
    staleTime: 30_000,
  });
  const rawQuery = useQuery({
    queryKey: ["wiki", "raw", "list"],
    queryFn: listRawEntries,
    staleTime: 30_000,
  });

  if (graphQuery.isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        Loading graph...
      </div>
    );
  }
  if (!graphQuery.data) return null;

  return (
    <div className="graph-view relative h-full w-full">
      <ForceGraph
        graphData={graphQuery.data}
        rawEntries={rawQuery.data?.entries ?? []}
        onClickConcept={(slug) => {
          openTab({ id: slug, kind: "article", slug, title: slug, closable: true });
        }}
        onClickRaw={() => navigate("/raw")}
      />
    </div>
  );
}

/* ── Tab content router ────────────────────────────────────────── */
function WikiContent() {
  const tabs = useWikiTabStore((s) => s.tabs);
  const activeTabId = useWikiTabStore((s) => s.activeTabId);
  const activeTab = tabs.find((t) => t.id === activeTabId);

  if (!activeTab) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        Select a page from the file tree
      </div>
    );
  }

  switch (activeTab.kind) {
    case "index":
      return <SpecialFilePage kind="index" />;
    case "article":
      return activeTab.slug ? <WikiArticle slug={activeTab.slug} /> : null;
    case "graph":
      return <EmbeddedGraph />;
    case "raw":
      return (
        <div className="flex h-full items-center justify-center text-muted-foreground">
          Raw entry viewer (navigate to /raw)
        </div>
      );
    default:
      return null;
  }
}

/* ── Main container ────────────────────────────────────────────── */
export function WikiTab() {
  return (
    <div className="flex h-full">
      {/* Left: File tree — 02-wiki-explorer.md §6.1 */}
      <WikiFileTree />

      {/* Center: Tab bar + content area */}
      <div className="flex flex-1 flex-col overflow-hidden">
        <WikiTabBar />
        <SkillProgressCard />
        <div className="flex-1 overflow-y-auto">
          <WikiContent />
        </div>
      </div>
    </div>
  );
}

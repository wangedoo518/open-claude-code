/**
 * WikiTab — main container for the Wiki Explorer (v2 multi-tab browser).
 * Per 02-wiki-explorer.md §6.1 layout:
 *   WikiFileTree (left, 240px) | WikiTabBar + WikiContent (center, flex-1)
 */

import { useQuery } from "@tanstack/react-query";
import ReactMarkdown from "react-markdown";
import { useNavigate } from "react-router-dom";
import { WikiTabBar } from "./WikiTabBar";
import { WikiArticle } from "./WikiArticle";
import { SkillProgressCard } from "./SkillProgressCard";
import {
  preprocessWikilinks,
  useWikiLinkRenderer,
} from "./wiki-link-utils";
import { useWikiTabStore } from "@/state/wiki-tab-store";
import { getWikiIndex, getWikiLog, getWikiGraph, listRawEntries } from "@/features/ingest/persist";
import { ForceGraph } from "@/features/graph/ForceGraph";

/* ── Index/Log special page ────────────────────────────────────── */
/** Renders `wiki/index.md` or `wiki/log.md` as full markdown with
 *  wiki-link interception. These files are maintained by the
 *  `wiki_maintainer` agent and frequently reference wiki pages via
 *  `[[slug]]`, `[Title](concepts/slug.md)`, or `wiki://slug`. The
 *  shared wiki-link renderer routes those to the tab store instead of
 *  navigating the browser; external http/https links open in new tab. */
function SpecialFilePage({ kind }: { kind: "index" | "log" }) {
  const Anchor = useWikiLinkRenderer();
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

  const content = data?.content ?? "";

  return (
    <div className="mx-auto max-w-[720px] px-8 py-6">
      <h1 className="mb-4 text-[24px] leading-[1.3] text-foreground">
        {kind === "index" ? "Wiki" : "Changelog"}
      </h1>
      {content ? (
        <div className="markdown-content">
          <ReactMarkdown components={{ a: Anchor }}>
            {preprocessWikilinks(content)}
          </ReactMarkdown>
        </div>
      ) : (
        <p className="text-muted-foreground">No content yet.</p>
      )}
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
    case "log":
      return <SpecialFilePage kind="log" />;
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
/**
 * WikiTab — v3: file tree moved to shell Sidebar (Wiki mode).
 * This component now only renders the tab bar + content area.
 */
export function WikiTab() {
  return (
    <div className="flex h-full flex-col overflow-hidden">
      <WikiTabBar />
      <SkillProgressCard />
      <div className="flex-1 overflow-y-auto">
        <WikiContent />
      </div>
    </div>
  );
}

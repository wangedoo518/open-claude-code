/**
 * WikiFileTree — left sidebar file tree for the Wiki Explorer.
 * Per 02-wiki-explorer.md §6.1 and component-spec.md §2.
 */

import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  Search,
  ChevronRight,
  Inbox,
  FileText,
  BookOpen,
  ScrollText,
  FileCode2,
} from "lucide-react";

import { listRawEntries, listWikiPages, listInboxEntries } from "@/features/ingest/persist";
import { useWikiTabStore, type WikiTabItem } from "@/state/wiki-tab-store";
import { AbsorbTriggerButton } from "./AbsorbTriggerButton";
import type { RawEntry, WikiPageSummary } from "@/features/ingest/types";

/* ── Query keys ────────────────────────────────────────────────── */
const treeKeys = {
  raw: () => ["wiki-tree", "raw"] as const,
  pages: () => ["wiki-tree", "pages"] as const,
  inbox: () => ["wiki-tree", "inbox"] as const,
};

/* ── Types ─────────────────────────────────────────────────────── */
interface TreeSection {
  id: string;
  label: string;
  icon: React.ReactNode;
  badge?: number;
  children: TreeNode[];
  /** If set, clicking the section header navigates here instead of toggling. */
  linkTo?: string;
}

/** Every tree node carries an explicit action so there are no
 *  "renders but click does nothing" dead nodes. If a node appears
 *  in the tree, it MUST have one of these actions. */
type TreeNodeAction =
  | { type: "openTab"; tab: WikiTabItem }
  | { type: "navigate"; to: string };

interface TreeNode {
  id: string;
  label: string;
  action: TreeNodeAction;
}

/* ── Component ─────────────────────────────────────────────────── */
export function WikiFileTree({ embedded = false }: { embedded?: boolean }) {
  const navigate = useNavigate();
  const openTab = useWikiTabStore((s) => s.openTab);
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set(["wiki"]));

  /* ── Data fetching ─────────────────────────────────────────── */
  const { data: rawData } = useQuery({
    queryKey: treeKeys.raw(),
    queryFn: listRawEntries,
    staleTime: 10_000,
  });
  const { data: pagesData } = useQuery({
    queryKey: treeKeys.pages(),
    queryFn: listWikiPages,
    staleTime: 10_000,
  });
  const { data: inboxData } = useQuery({
    queryKey: treeKeys.inbox(),
    queryFn: listInboxEntries,
    staleTime: 30_000,
  });

  /* ── Build tree sections ───────────────────────────────────── */
  const sections = useMemo(() => {
    const raws: RawEntry[] = rawData?.entries ?? [];
    const pages: WikiPageSummary[] = pagesData?.pages ?? [];
    const pendingCount = inboxData?.pending_count ?? 0;

    const lowerFilter = filter.toLowerCase();
    const matchesFilter = (text: string) =>
      !lowerFilter || text.toLowerCase().includes(lowerFilter);

    // Inbox section
    const inboxSection: TreeSection = {
      id: "inbox",
      label: "Inbox",
      icon: <Inbox className="size-4" />,
      badge: pendingCount > 0 ? pendingCount : undefined,
      children: [],
      linkTo: "/inbox",
    };

    // Raw section (latest 20) — click navigates to /raw?entry=N,
    // deep-linking directly to the specific raw entry.
    const rawNodes: TreeNode[] = raws
      .slice(0, 20)
      .filter((r) => matchesFilter(r.slug) || matchesFilter(r.source))
      .map((r) => ({
        id: `raw-${r.id}`,
        label: `${r.slug} (${r.source})`,
        action: { type: "navigate" as const, to: `/raw?entry=${r.id}` },
      }));

    const rawSection: TreeSection = {
      id: "raw",
      label: "Raw",
      icon: <FileText className="size-4" />,
      children: rawNodes,
    };

    // Wiki section — grouped by category
    const categories = ["concepts", "people", "topics", "compare"] as const;

    const wikiChildren: TreeNode[] = [];
    for (const cat of categories) {
      // category field from backend is "concept" not "concepts", etc.
      const catKey = cat === "concepts" ? "concept" : cat === "topics" ? "topic" : cat;
      const catPages = pages.filter(
        (p) => (p as WikiPageSummary & { category?: string }).category === catKey ||
          // Fallback: if no category field, put under concepts
          (!((p as WikiPageSummary & { category?: string }).category) && cat === "concepts"),
      );

      for (const p of catPages) {
        if (matchesFilter(p.title) || matchesFilter(p.slug)) {
          wikiChildren.push({
            id: `wiki-${p.slug}`,
            label: p.title || p.slug,
            action: {
              type: "openTab",
              tab: {
                id: p.slug,
                kind: "article",
                slug: p.slug,
                title: p.title || p.slug,
                closable: true,
              },
            },
          });
        }
      }
    }

    const wikiSection: TreeSection = {
      id: "wiki",
      label: "Wiki",
      icon: <BookOpen className="size-4" />,
      children: wikiChildren,
    };

    // Schema section — CLAUDE.md opens the dedicated /schema editor page,
    // NOT a wiki tab (SchemaEditorPage is its own route, not a WikiTabItem).
    const schemaSection: TreeSection = {
      id: "schema",
      label: "Schema",
      icon: <ScrollText className="size-4" />,
      children: [
        {
          id: "schema-claude",
          label: "CLAUDE.md",
          action: { type: "navigate", to: "/schema" },
        },
      ],
    };

    // Log section — changelog / audit trail (opens as closable wiki tab)
    const logSection: TreeSection = {
      id: "log-section",
      label: "Log",
      icon: <FileText className="size-4" />,
      children: [
        {
          id: "_log",
          label: "Changelog",
          action: {
            type: "openTab",
            tab: { id: "_log", kind: "log", title: "Log", closable: true },
          },
        },
      ],
    };

    return [inboxSection, rawSection, wikiSection, schemaSection, logSection];
  }, [rawData, pagesData, inboxData, filter]);

  /* ── Handlers ──────────────────────────────────────────────── */
  const toggleSection = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  /** Unified action dispatch — every node carries its own action,
   *  so there's no "renders but click does nothing" dead-node bug. */
  const handleNodeClick = (node: TreeNode) => {
    if (node.action.type === "openTab") {
      openTab(node.action.tab);
    } else {
      navigate(node.action.to);
    }
  };

  const handleSectionClick = (section: TreeSection) => {
    if (section.linkTo) {
      navigate(section.linkTo);
    } else {
      toggleSection(section.id);
    }
  };

  /* ── Render ────────────────────────────────────────────────── */
  return (
    <div className={
      embedded
        ? "flex h-full w-full flex-col"
        : "flex h-full w-[240px] min-w-[180px] max-w-[360px] flex-col border-r border-[var(--color-sidebar-border)] bg-[var(--color-sidebar-background)]"
    }>
      {/* Search bar — per component-spec.md §2.3 */}
      <div className="sticky top-0 z-10 p-2">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 size-4 -translate-y-1/2 text-[var(--color-muted-foreground)]" />
          <input
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="搜索..."
            className="h-8 w-full rounded-lg bg-[var(--color-background)]/90 pl-8 pr-2 text-[13px] text-[var(--color-foreground)] placeholder:text-[var(--color-muted-foreground)] outline-none backdrop-blur-sm"
          />
        </div>
      </div>

      {/* Tree sections */}
      <div className="flex-1 overflow-y-auto px-1 pb-2">
        {sections.map((section) => (
          <div key={section.id} className="mb-1">
            {/* Section header */}
            <button
              onClick={() => handleSectionClick(section)}
              className="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[12px] font-semibold text-[var(--color-sidebar-foreground)] hover:bg-[var(--color-sidebar-accent)] transition-colors"
            >
              {!section.linkTo && (
                <ChevronRight
                  className={`size-4 transition-transform duration-200 ${
                    expanded.has(section.id) ? "rotate-90" : ""
                  }`}
                />
              )}
              {section.icon}
              <span className="flex-1 text-left">{section.label}</span>
              {section.badge != null && (
                <span className="flex min-w-[18px] items-center justify-center rounded-full bg-[var(--color-destructive)] px-1.5 py-0.5 text-[10px] font-semibold text-white">
                  {section.badge}
                </span>
              )}
            </button>
            {/* Absorb button for Wiki section */}
            {section.id === "wiki" && (
              <div
                className="ml-auto mr-1 -mt-7 flex justify-end"
                onClick={(e) => e.stopPropagation()}
              >
                <AbsorbTriggerButton compact />
              </div>
            )}

            {/* Children (collapsed by default except wiki) */}
            {expanded.has(section.id) && section.children.length > 0 && (
              <div className="ml-4">
                {section.children.map((node) => (
                  <button
                    key={node.id}
                    onClick={() => handleNodeClick(node)}
                    className="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[12px] text-[var(--color-sidebar-foreground)] hover:bg-[var(--color-foreground)]/5 transition-colors truncate"
                  >
                    <FileCode2 className="size-3.5 shrink-0 text-[var(--color-muted-foreground)]" />
                    <span className="truncate">{node.label}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

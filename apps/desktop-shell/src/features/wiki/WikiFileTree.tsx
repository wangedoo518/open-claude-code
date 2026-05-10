/**
 * WikiFileTree — left sidebar file tree for the Wiki Explorer.
 * Per 02-wiki-explorer.md §6.1 and component-spec.md §2.
 */

import { useEffect, useMemo, useRef, useState } from "react";
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

import { listRawEntries, listWikiPages, listInboxEntries } from "@/api/wiki/repository";
import { useWikiTabStore, type WikiTabItem } from "@/state/wiki-tab-store";
import { AbsorbTriggerButton } from "./AbsorbTriggerButton";
import type { RawEntry, WikiPageSummary } from "@/api/wiki/types";

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

type VisibleTreeItem =
  | { id: string; kind: "section"; section: TreeSection }
  | { id: string; kind: "node"; node: TreeNode };

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
    queryFn: () => listWikiPages(),
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

    // Inbox section (待整理) — click the header to go to /inbox.
    const inboxSection: TreeSection = {
      id: "inbox",
      label: "待整理",
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
      label: "素材库",
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
        (p) => p.category === catKey ||
          // Fallback: if no category field, put under concepts
          (!p.category && cat === "concepts"),
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
      label: "知识库",
      icon: <BookOpen className="size-4" />,
      children: wikiChildren,
    };

    // I4 sprint: 整理规则 (formerly "Schema") and 最近变更 (formerly
    // "Log") are grouped into a single collapsed "高级" section so the
    // default sidebar shows 待整理 / 素材库 / 知识库 only. Both routes
    // remain reachable from the command palette and from direct URLs.
    const advancedSection: TreeSection = {
      id: "advanced",
      label: "高级",
      icon: <ScrollText className="size-4" />,
      children: [
        {
          id: "schema-claude",
          label: "整理规则",
          action: { type: "navigate", to: "/schema" },
        },
        {
          id: "_log",
          label: "最近变更",
          action: {
            type: "openTab",
            tab: { id: "_log", kind: "log", title: "最近变更", closable: true },
          },
        },
      ],
    };

    return [inboxSection, rawSection, wikiSection, advancedSection];
  }, [rawData, pagesData, inboxData, filter]);

  const visibleItems = useMemo<VisibleTreeItem[]>(() => {
    const items: VisibleTreeItem[] = [];
    for (const section of sections) {
      items.push({ id: `section-${section.id}`, kind: "section", section });
      if (expanded.has(section.id)) {
        for (const node of section.children) {
          items.push({ id: `node-${node.id}`, kind: "node", node });
        }
      }
    }
    return items;
  }, [sections, expanded]);

  const itemRefs = useRef(new Map<string, HTMLButtonElement>());
  const [activeItemId, setActiveItemId] = useState<string | null>(null);

  useEffect(() => {
    if (!visibleItems.length) {
      setActiveItemId(null);
      return;
    }
    if (!activeItemId || !visibleItems.some((item) => item.id === activeItemId)) {
      setActiveItemId(visibleItems[0].id);
    }
  }, [activeItemId, visibleItems]);

  const setItemRef = (id: string) => (element: HTMLButtonElement | null) => {
    if (element) itemRefs.current.set(id, element);
    else itemRefs.current.delete(id);
  };

  const focusItem = (id: string) => {
    setActiveItemId(id);
    window.requestAnimationFrame(() => {
      itemRefs.current.get(id)?.focus();
    });
  };

  const isTabStop = (id: string) =>
    activeItemId ? activeItemId === id : visibleItems[0]?.id === id;

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
   *  so there's no "renders but click does nothing" dead-node bug.
   *
   *  U1 fix: when a wiki tab is opened from a non-/wiki route, the
   *  tab appeared in the store but the user was still looking at
   *  the previous page. Always route to /wiki alongside the openTab
   *  so the newly-opened article actually surfaces. */
  const handleNodeClick = (node: TreeNode) => {
    if (node.action.type === "openTab") {
      openTab(node.action.tab);
      if (!window.location.hash.startsWith("#/wiki")) {
        navigate("/wiki");
      }
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

  const handleTreeKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey || !visibleItems.length) {
      return;
    }

    const currentIndex = Math.max(
      0,
      visibleItems.findIndex((item) => item.id === activeItemId),
    );
    const currentItem = visibleItems[currentIndex];

    switch (event.key) {
      case "ArrowDown": {
        event.preventDefault();
        const next = visibleItems[Math.min(currentIndex + 1, visibleItems.length - 1)];
        focusItem(next.id);
        break;
      }
      case "ArrowUp": {
        event.preventDefault();
        const previous = visibleItems[Math.max(currentIndex - 1, 0)];
        focusItem(previous.id);
        break;
      }
      case "Home":
        event.preventDefault();
        focusItem(visibleItems[0].id);
        break;
      case "End":
        event.preventDefault();
        focusItem(visibleItems[visibleItems.length - 1].id);
        break;
      case "ArrowRight":
        if (
          currentItem?.kind === "section" &&
          !currentItem.section.linkTo &&
          !expanded.has(currentItem.section.id)
        ) {
          event.preventDefault();
          toggleSection(currentItem.section.id);
        }
        break;
      case "ArrowLeft":
        if (
          currentItem?.kind === "section" &&
          !currentItem.section.linkTo &&
          expanded.has(currentItem.section.id)
        ) {
          event.preventDefault();
          toggleSection(currentItem.section.id);
        }
        break;
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
      <div className="flex-1 overflow-y-auto px-1 pb-2" onKeyDown={handleTreeKeyDown}>
        {sections.map((section) => (
          <div key={section.id} className="mb-1">
            {/* Section header */}
            <button
              ref={setItemRef(`section-${section.id}`)}
              tabIndex={isTabStop(`section-${section.id}`) ? 0 : -1}
              onClick={() => handleSectionClick(section)}
              onFocus={() => setActiveItemId(`section-${section.id}`)}
              className={`flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[12px] font-semibold text-[var(--color-sidebar-foreground)] transition-colors hover:bg-[var(--color-sidebar-accent)] ${
                activeItemId === `section-${section.id}` ? "bg-[var(--color-sidebar-accent)]" : ""
              }`}
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
                    ref={setItemRef(`node-${node.id}`)}
                    tabIndex={isTabStop(`node-${node.id}`) ? 0 : -1}
                    onClick={() => handleNodeClick(node)}
                    onFocus={() => setActiveItemId(`node-${node.id}`)}
                    className={`flex w-full items-center gap-1.5 truncate rounded-md px-2 py-1 text-[12px] text-[var(--color-sidebar-foreground)] transition-colors hover:bg-[var(--color-foreground)]/5 ${
                      activeItemId === `node-${node.id}` ? "bg-[var(--color-foreground)]/5" : ""
                    }`}
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

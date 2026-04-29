/**
 * Palette data source — aggregates CLAWWIKI_ROUTES + wiki pages + raw
 * entries + pending inbox entries into `PaletteGroup[]`, filtered by
 * the current query.
 *
 * React Query keys are REUSED (not shadowed) so this hook's fetches
 * share the same cache as Sidebar / RawLibrary / Wiki / Inbox surfaces.
 *
 *     ["wiki", "raw", "list"]     — RawEntry[]
 *     ["wiki", "pages", "list"]   — WikiPageSummary[]
 *     ["wiki", "inbox", "list"]   — InboxEntry[] (filter to pending)
 *
 * The Recent group only renders when the query is empty (so it doesn't
 * compete visually with live filter results).
 *
 * S1 Unified Search upgrades (Worker A's scope):
 *   1. `usePaletteContext` — derive a route-aware `PaletteContext` from
 *      HashRouter location + wiki tab store, so the palette knows
 *      "where is the user right now?" (wiki page, raw detail, etc.).
 *   2. Context-driven graph fetch — when the context is a wiki slug,
 *      pull `/api/wiki/pages/{slug}/graph` (G1 endpoint) and splice
 *      backlinks / related / see_also as palette items with boosted
 *      scores and `why` explanations.
 *   3. Scoring — every item carries a `score` (larger = better rank)
 *      and a `why` string; default (empty query) and search (non-empty)
 *      both sort by score desc.
 *   4. Per-kind `secondaryActions` — items advertise their default
 *      chip actions (Ask-with / Focus-in-graph / Open-wiki) so Worker B's
 *      UI can render chips without a second lookup.
 *
 * All four additions are strictly additive — the existing `PaletteItem`
 * fields (`value`, `label`, `hint`, `icon`, `kind`, and kind-specific
 * refs) remain intact. The extra fields (`score`, `why`, `secondaryActions`)
 * are declared in a local `PaletteContextFields` interface and will be
 * promoted into `types.ts` by Worker C during the S1 handshake.
 */

import { useEffect, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useLocation } from "react-router-dom";
import {
  BookOpen,
  Clock,
  Compass,
  FileText,
  Inbox,
  LayoutDashboard,
  Link2,
  MessageCircle,
  Network,
  ScrollText,
  Settings,
} from "lucide-react";

import {
  CLAWWIKI_ROUTES,
  type ClawWikiRoute,
} from "@/shell/clawwiki-routes";
import { listInboxEntries, listRawEntries, listWikiPages } from "@/api/wiki/repository";
import type {
  InboxEntry,
  RawEntry,
  WikiPageSummary,
} from "@/api/wiki/types";
import { useCommandPaletteStore } from "@/state/command-palette-store";
import { useWikiTabStore } from "@/state/wiki-tab-store";
import { getRouteCommand } from "./command-manifest";
import {
  getWikiPageGraph,
  type PageGraph,
  type PageGraphNode,
  type RelatedPageHit,
} from "@/lib/tauri";

import { normalizeForSearch } from "./filter";
import {
  paletteValueFor,
  type InboxPaletteItem,
  type PaletteGroup,
  type PaletteIcon,
  type PaletteItem,
  type PaletteItemSecondaryAction,
  type PaletteRecentItem,
  type RawPaletteItem,
  type RoutePaletteItem,
  type WikiPaletteItem,
} from "./types";

// ── S1 local narrowing helper ─────────────────────────────────────
//
// `PaletteItemSecondaryAction` is the canonical type from `./types`
// (Worker C). The `PaletteContextFields` mix-in below narrows the
// three optional fields that types.ts declares on `PaletteItemBase`
// (score / why / secondaryActions) into *required* fields inside this
// module — every builder in here always populates them, so downstream
// scoring code can rely on them being present without null-guards.

/** Extra fields each palette item carries under S1 — score + reason + chips. */
interface PaletteContextFields {
  /** Sort score (higher = ranked earlier). */
  score: number;
  /** Short Chinese explanation of why the item surfaced. */
  why: string;
  /** Default chip actions for this kind; empty array when none apply. */
  secondaryActions: PaletteItemSecondaryAction[];
}

/** A `PaletteItem` enriched with S1 scoring + secondary-action metadata. */
type ScoredPaletteItem = PaletteItem & PaletteContextFields;

// ── S1 PaletteContext — URL-driven awareness for weighting ────────────

/**
 * Where the user currently is, used to weight palette results. We
 * intentionally keep this a shallow discriminated union — each variant
 * carries only the id the scorer needs.
 */
export type PaletteContext =
  | { kind: "wiki"; slug: string }
  | { kind: "raw"; id: number }
  | { kind: "inbox"; id: number }
  | { kind: "ask"; sessionId?: string }
  | { kind: "graph" }
  | { kind: "none" };

/**
 * Derive the current `PaletteContext` from the HashRouter pathname +
 * the wiki-tab-store's active tab.
 *
 * Routing shape (see `ClawWikiShell.tsx`):
 *   - `#/wiki`         → wiki list; slug (if any) comes from the wiki-tab-store's
 *                        active article tab (`navigateToWikiPage` only jumps to
 *                        `/wiki` and hands the slug off through the tab store).
 *   - `#/wiki/<slug>`  → direct slug in pathname (future-proof for when wiki
 *                        routes carry slugs in the URL).
 *   - `#/raw?entry=N`  → raw library focused on id N (query param).
 *   - `#/inbox?task=N` → inbox focused on task id N (query param).
 *   - `#/ask/<id?>`    → Ask surface; optional session id in the path.
 *   - `#/graph`        → graph page.
 *
 * Falls back to `{ kind: "none" }` for every unrecognised path so the
 * scorer can skip any context-driven boost cleanly.
 */
export function usePaletteContext(): PaletteContext {
  const location = useLocation();
  // `location.search` under HashRouter already reflects the `?foo=bar`
  // portion after the hash path, so URLSearchParams works directly.
  const activeTab = useWikiTabStore((s) =>
    s.tabs.find((t) => t.id === s.activeTabId),
  );

  return useMemo<PaletteContext>(() => {
    const pathname = location.pathname ?? "";

    // --- Wiki ---------------------------------------------------------
    if (pathname.startsWith("/wiki")) {
      // Future-proof: `/wiki/<slug>` gives us the slug directly.
      const direct = pathname.slice("/wiki".length).replace(/^\/+/, "");
      if (direct.length > 0) {
        // Strip any trailing path segments beyond the first (e.g.
        // `/wiki/foo/bar` → slug "foo"). Unlikely today but cheap.
        const slug = direct.split("/")[0];
        return { kind: "wiki", slug };
      }
      // Current in-app pattern: slug lives on the active wiki tab.
      if (activeTab && activeTab.kind === "article" && activeTab.slug) {
        return { kind: "wiki", slug: activeTab.slug };
      }
      // `/wiki` with no article selected — leave context "none" so the
      // palette doesn't try to fetch a graph for nothing.
      return { kind: "none" };
    }

    // --- Raw: `/raw?entry=N` -----------------------------------------
    if (pathname.startsWith("/raw")) {
      const params = new URLSearchParams(location.search);
      const entry = params.get("entry");
      if (entry) {
        const n = Number(entry);
        if (Number.isFinite(n)) return { kind: "raw", id: n };
      }
      return { kind: "none" };
    }

    // --- Inbox: `/inbox?task=N` --------------------------------------
    if (pathname.startsWith("/inbox")) {
      const params = new URLSearchParams(location.search);
      const task = params.get("task");
      if (task) {
        const n = Number(task);
        if (Number.isFinite(n)) return { kind: "inbox", id: n };
      }
      return { kind: "none" };
    }

    // --- Ask: `/ask[/<sessionId>]` -----------------------------------
    if (pathname.startsWith("/ask")) {
      const rest = pathname.slice("/ask".length).replace(/^\/+/, "");
      const sessionId = rest.split("/")[0] || undefined;
      return { kind: "ask", sessionId };
    }

    // --- Graph --------------------------------------------------------
    if (pathname.startsWith("/graph")) {
      return { kind: "graph" };
    }

    return { kind: "none" };
  }, [location.pathname, location.search, activeTab]);
}

// ── Local translators (copied from RawLibraryPage / InboxPage) ─────
// Intentionally duplicated here — per Worker B's scope we may not
// cross-import product code. These mappings are small and stable.

/** Translate raw source tag to a Chinese display label. */
function translateSource(source: string): string {
  const map: Record<string, string> = {
    "wechat-url": "微信链接",
    "wechat-text": "微信消息",
    "wechat-article": "微信文章",
    "paste-text": "粘贴文本",
    "paste-url": "粘贴链接",
    paste: "粘贴",
    url: "网页",
    pdf: "PDF 文件",
    docx: "Word 文件",
    pptx: "PPT 文件",
    image: "图片",
  };
  return map[source] ?? source;
}

/** Translate inbox kind to a Chinese display label. */
function translateKind(kind: string): string {
  const map: Record<string, string> = {
    "new-raw": "新素材",
    stale: "待更新",
    conflict: "冲突",
    deprecate: "弃用",
  };
  return map[kind] ?? kind;
}

/** Translate inbox status to a Chinese display label. */
function translateStatus(status: string): string {
  const map: Record<string, string> = {
    pending: "待处理",
    approved: "已批准",
    rejected: "已拒绝",
  };
  return map[status] ?? status;
}

// ── Route key → Lucide icon mapping ───────────────────────────────

/** Map a CLAWWIKI_ROUTES key to a Lucide icon component. */
function iconForRouteKey(key: string): PaletteIcon {
  const map: Record<string, PaletteIcon> = {
    dashboard: LayoutDashboard,
    ask: MessageCircle,
    inbox: Inbox,
    raw: FileText,
    wiki: BookOpen,
    graph: Network,
    schema: ScrollText,
    wechat: Link2,
    settings: Settings,
  };
  return map[key] ?? Compass;
}

// ── Secondary action defaults per kind ────────────────────────────

/**
 * Default chip actions for each item kind. Returned as fresh arrays
 * (mutable so the hook can append inbox-specific chips when an inbox
 * entry has a resolved `target_page_slug`).
 */
function defaultSecondaryActions(
  kind: PaletteItem["kind"],
): PaletteItemSecondaryAction[] {
  switch (kind) {
    case "wiki":
      return [
        { id: "ask_with", label: "以此页为依据 Ask" },
        { id: "focus_graph", label: "在图谱中聚焦" },
      ];
    case "raw":
      return [{ id: "ask_with", label: "以此素材 Ask" }];
    case "inbox":
      // Inbox items don't get a default "open wiki" chip; the builder
      // adds it only when `target_page_slug` is available.
      return [];
    case "route":
      return [];
  }
}

// ── Item constructors ─────────────────────────────────────────────

function buildRouteItems(): (RoutePaletteItem & PaletteContextFields)[] {
  return CLAWWIKI_ROUTES.map((route: ClawWikiRoute) => {
    const command = getRouteCommand(route.key);
    return {
      kind: "route" as const,
      value: paletteValueFor("route", route.key),
      label: route.label,
      hint: route.key,
      icon: iconForRouteKey(route.key),
      routeKey: route.key,
      path: route.path,
      commandId: command?.id,
      // Score / why are assigned by the ranker below; start at 0 so
      // untouched routes fall to the tail of search results.
      score: 0,
      why: "",
      secondaryActions: defaultSecondaryActions("route"),
    };
  });
}

function buildWikiItems(
  pages: WikiPageSummary[] | undefined,
): (WikiPaletteItem & PaletteContextFields)[] {
  if (!pages) return [];
  // Cap at 100 so the list stays bounded in sparse edge cases.
  const sliced = pages.slice(0, 100);
  return sliced.map((page) => {
    const label = page.title || page.slug;
    const summary = page.summary ?? "";
    const hint = summary.length > 40 ? `${summary.slice(0, 40)}…` : summary;
    return {
      kind: "wiki" as const,
      value: paletteValueFor("wiki", page.slug),
      label,
      hint: hint || undefined,
      icon: BookOpen,
      slug: page.slug,
      title: label,
      score: 0,
      why: "",
      secondaryActions: defaultSecondaryActions("wiki"),
    };
  });
}

function buildRawItems(
  entries: RawEntry[] | undefined,
): (RawPaletteItem & PaletteContextFields)[] {
  if (!entries) return [];
  // `listRawEntries` returns id asc; palette UX wants newest first.
  const sorted = [...entries].sort((a, b) => b.id - a.id).slice(0, 50);
  return sorted.map((entry, idx) => ({
    kind: "raw" as const,
    value: paletteValueFor("raw", entry.id),
    label: entry.slug,
    hint: `${translateSource(entry.source)} · ${entry.date}`,
    icon: FileText,
    id: entry.id,
    // Preserve newest-first in default (no-query) ordering by giving
    // earlier entries a small time-decay bonus. The ranker overrides
    // this once a query is typed.
    score: Math.max(0, 20 - idx),
    why: "",
    secondaryActions: defaultSecondaryActions("raw"),
  }));
}

function buildInboxItems(
  entries: InboxEntry[] | undefined,
): (InboxPaletteItem & PaletteContextFields)[] {
  if (!entries) return [];
  const pending = entries
    .filter((e) => e.status === "pending")
    .slice(0, 50);
  return pending.map((entry) => {
    // Inbox items only advertise "open wiki" when they've been
    // approved to a target slug; otherwise there's nothing to open.
    const actions = [...defaultSecondaryActions("inbox")];
    if (entry.target_page_slug) {
      actions.push({ id: "open_wiki", label: "打开关联 Wiki" });
    }
    return {
      kind: "inbox" as const,
      value: paletteValueFor("inbox", entry.id),
      label: entry.title,
      hint: `${translateKind(entry.kind)} · ${translateStatus(entry.status)}`,
      icon: Inbox,
      id: entry.id,
      score: 0,
      why: "",
      secondaryActions: actions,
    };
  });
}

// ── Context-graph neighbor → palette item adapters ────────────────
//
// Backlinks + related + see_also all come from the same G1 endpoint
// but have slightly different shapes. All three become wiki palette
// items with boosted scores + a `why` string that reflects the origin.

function buildContextNeighborItems(
  graph: PageGraph | undefined,
): (WikiPaletteItem & PaletteContextFields)[] {
  if (!graph) return [];
  const out: (WikiPaletteItem & PaletteContextFields)[] = [];
  const seen = new Set<string>(); // dedupe across backlinks/related/see_also

  const pushNode = (node: PageGraphNode, why: string, boost: number) => {
    if (seen.has(node.slug)) return;
    seen.add(node.slug);
    out.push({
      kind: "wiki",
      value: paletteValueFor("wiki", node.slug),
      label: node.title || node.slug,
      hint: node.category || undefined,
      icon: BookOpen,
      slug: node.slug,
      title: node.title || node.slug,
      score: boost,
      why,
      secondaryActions: defaultSecondaryActions("wiki"),
    });
  };

  const pushRelated = (hit: RelatedPageHit, why: string, boost: number) => {
    if (seen.has(hit.slug)) return;
    seen.add(hit.slug);
    const hint =
      hit.reasons && hit.reasons.length > 0
        ? hit.reasons.join(" · ")
        : hit.category || undefined;
    out.push({
      kind: "wiki",
      value: paletteValueFor("wiki", hit.slug),
      label: hit.title || hit.slug,
      hint,
      icon: BookOpen,
      slug: hit.slug,
      title: hit.title || hit.slug,
      score: boost,
      why,
      secondaryActions: defaultSecondaryActions("wiki"),
    });
  };

  for (const n of graph.backlinks ?? []) {
    pushNode(n, "本页反向链接", 200);
  }
  for (const r of graph.related ?? []) {
    pushRelated(r, "本页相关", 150);
  }
  // The G1 endpoint returns `outgoing` (neighbors this page links to);
  // the task brief calls the third bucket "see_also" — we treat
  // outgoing as the see-also surface since that's the available field
  // on `PageGraph`.
  for (const n of graph.outgoing ?? []) {
    pushNode(n, "本页 See also", 120);
  }
  return out;
}

// ── Recent reconstruction ─────────────────────────────────────────

/**
 * Rebuild a concrete PaletteItem from a stored PaletteRecentItem.
 *
 * Returns `null` when the recent entry can't be hydrated (e.g. a
 * route key no longer exists in CLAWWIKI_ROUTES — graceful skip).
 */
function reconstructPaletteItemFromRecent(
  r: PaletteRecentItem,
): ScoredPaletteItem | null {
  switch (r.kind) {
    case "route": {
      const route = CLAWWIKI_ROUTES.find((rt) => rt.key === r.id);
      if (!route) return null;
      return {
        kind: "route",
        value: paletteValueFor("route", r.id),
        label: r.label,
        hint: r.hint,
        icon: Clock,
        routeKey: r.id,
        path: route.path,
        score: 80,
        why: "最近使用",
        secondaryActions: defaultSecondaryActions("route"),
      };
    }
    case "wiki":
      return {
        kind: "wiki",
        value: paletteValueFor("wiki", r.id),
        label: r.label,
        hint: r.hint,
        icon: Clock,
        slug: r.id,
        title: r.label,
        score: 80,
        why: "最近使用",
        secondaryActions: defaultSecondaryActions("wiki"),
      };
    case "raw": {
      const id = Number(r.id);
      if (!Number.isFinite(id)) return null;
      return {
        kind: "raw",
        value: paletteValueFor("raw", id),
        label: r.label,
        hint: r.hint,
        icon: Clock,
        id,
        score: 80,
        why: "最近使用",
        secondaryActions: defaultSecondaryActions("raw"),
      };
    }
    case "inbox": {
      const id = Number(r.id);
      if (!Number.isFinite(id)) return null;
      return {
        kind: "inbox",
        value: paletteValueFor("inbox", id),
        label: r.label,
        hint: r.hint,
        icon: Clock,
        id,
        score: 80,
        why: "最近使用",
        secondaryActions: defaultSecondaryActions("inbox"),
      };
    }
    default:
      // Unknown kind on disk — skip it rather than throw.
      return null;
  }
}

// ── Scoring + context boosting ────────────────────────────────────

/**
 * Score a single item against a (normalised) query. Mutates `item.score`
 * and `item.why` in place for the match; leaves them untouched when
 * the query doesn't match. Caller should filter out items whose score
 * remains at their baseline (0) when a query is active.
 *
 * Scoring table:
 *   - exact title match (label === query)    → 1000 "完全匹配标题"
 *   - title starts with query                → 500  "标题以此开头"
 *   - title contains query                   → 100  "标题包含 ..."
 *   - hint / extras contain query            → 50   "内容包含 ..."
 */
function scoreItemAgainstQuery(
  item: ScoredPaletteItem,
  nq: string,
  extras: string[] = [],
): boolean {
  if (nq === "") return true;
  const label = normalizeForSearch(item.label);
  if (label === nq) {
    item.score = Math.max(item.score, 0) + 1000;
    item.why = "完全匹配标题";
    return true;
  }
  if (label.startsWith(nq)) {
    item.score = Math.max(item.score, 0) + 500;
    item.why = "标题以此开头";
    return true;
  }
  if (label.includes(nq)) {
    item.score = Math.max(item.score, 0) + 100;
    item.why = `标题包含 ${nq}`;
    return true;
  }
  const hint = item.hint ? normalizeForSearch(item.hint) : "";
  if (hint && hint.includes(nq)) {
    item.score = Math.max(item.score, 0) + 50;
    item.why = `内容包含 ${nq}`;
    return true;
  }
  for (const field of extras) {
    if (normalizeForSearch(field).includes(nq)) {
      item.score = Math.max(item.score, 0) + 50;
      item.why = `内容包含 ${nq}`;
      return true;
    }
  }
  return false;
}

/**
 * Merge context-driven neighbor items into `base`, bumping the score
 * of any existing duplicate (same slug / id) rather than emitting two
 * rows. Returns the merged list in insertion order — callers sort
 * afterwards by score.
 */
function mergeContextItems<T extends ScoredPaletteItem>(
  base: T[],
  context: T[],
): T[] {
  if (context.length === 0) return base;
  const byValue = new Map<string, T>();
  for (const item of base) byValue.set(item.value, item);
  const merged: T[] = [...base];
  for (const ctxItem of context) {
    const existing = byValue.get(ctxItem.value);
    if (existing) {
      // Preserve the higher score + the context reason (context is
      // the more informative "why" than a generic search match).
      existing.score += ctxItem.score;
      if (!existing.why || existing.why.startsWith("标题")) {
        existing.why = ctxItem.why;
      }
    } else {
      byValue.set(ctxItem.value, ctxItem);
      merged.push(ctxItem);
    }
  }
  return merged;
}

// ── Main hook ─────────────────────────────────────────────────────

/**
 * Compose the grouped palette items for the current query.
 *
 * The returned array is stable across renders when inputs are
 * unchanged, courtesy of `useMemo` over the React Query data
 * references, the zustand `recent` array, and the active
 * `PaletteContext`.
 */
export function useGroupedPaletteItems(query: string): PaletteGroup[] {
  // 3 parallel list queries — reuse existing keys so cache is shared.
  const rawQuery = useQuery({
    queryKey: ["wiki", "raw", "list"],
    queryFn: listRawEntries,
    staleTime: 10_000,
  });

  const pagesQuery = useQuery({
    queryKey: ["wiki", "pages", "list"],
    queryFn: listWikiPages,
    staleTime: 10_000,
  });

  const inboxQuery = useQuery({
    queryKey: ["wiki", "inbox", "list"],
    queryFn: listInboxEntries,
    staleTime: 10_000,
  });

  const recent = useCommandPaletteStore((s) => s.recent);
  const open = useCommandPaletteStore((s) => s.open);

  // S1: URL-driven context + graph fetch (only when palette is open
  // AND the user is on a wiki article — otherwise no network spin).
  const context = usePaletteContext();
  const contextGraphQuery = useQuery<PageGraph>({
    queryKey: [
      "palette-context-graph",
      context.kind === "wiki" ? context.slug : null,
    ],
    queryFn: () => getWikiPageGraph((context as { kind: "wiki"; slug: string }).slug),
    enabled: open && context.kind === "wiki",
    staleTime: 30_000,
  });

  // When the palette opens, kick a background refetch so results feel
  // fresh without forcing refetch on every keystroke.
  useEffect(() => {
    if (open) {
      void rawQuery.refetch();
      void pagesQuery.refetch();
      void inboxQuery.refetch();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const rawData = rawQuery.data;
  const pagesData = pagesQuery.data;
  const inboxData = inboxQuery.data;
  const contextGraph = contextGraphQuery.data;

  const groups = useMemo<PaletteGroup[]>(() => {
    const trimmed = query.trim();
    const nq = normalizeForSearch(trimmed);

    // Static + live item pools (all already carry baseline score/why/actions).
    const allRouteItems = buildRouteItems();
    const allWikiItems = buildWikiItems(pagesData?.pages);
    const allRawItems = buildRawItems(rawData?.entries);
    const allInboxItems = buildInboxItems(inboxData?.entries);
    const contextWikiItems = buildContextNeighborItems(contextGraph);

    // Merge context neighbors into the wiki pool, boosting duplicates.
    const wikiPool = mergeContextItems(allWikiItems, contextWikiItems);

    // --- Scoring pass -------------------------------------------------
    //
    // We score each pool independently (so group boundaries stay
    // stable) but rank *within* a group by the same score function.
    //
    // When `nq` is empty, every item keeps its baseline score (context
    // boosts, recency, time decay) — so context neighbors surface to
    // the top of Wiki, and newest Raws win the Raw group.

    const matchedRoutes = allRouteItems.filter((i) =>
      scoreItemAgainstQuery(i, nq),
    );
    const matchedWikis = wikiPool.filter((i) => {
      // Wiki items match against slug too (same spirit as the old
      // `filterPaletteItems` call that only matched label + hint).
      return scoreItemAgainstQuery(i, nq, [i.slug]);
    });
    const matchedRaws = allRawItems.filter((i) =>
      scoreItemAgainstQuery(i, nq, [String(i.id)]),
    );
    const matchedInbox = allInboxItems.filter((i) =>
      scoreItemAgainstQuery(i, nq, [String(i.id)]),
    );

    // Sort desc by score; stable fallback by label to avoid churn.
    const byScoreDesc = (a: ScoredPaletteItem, b: ScoredPaletteItem) =>
      b.score - a.score || a.label.localeCompare(b.label);
    matchedRoutes.sort(byScoreDesc);
    matchedWikis.sort(byScoreDesc);
    matchedRaws.sort(byScoreDesc);
    matchedInbox.sort(byScoreDesc);

    const result: PaletteGroup[] = [];

    // Recent — only when the query is empty so it doesn't compete
    // with live filter results. Recent items come pre-scored at 80.
    if (trimmed === "") {
      const recentItems = recent
        .slice(0, 5)
        .map((r) => reconstructPaletteItemFromRecent(r))
        .filter((x): x is ScoredPaletteItem => x !== null);
      if (recentItems.length > 0) {
        result.push({
          id: "recent",
          heading: "最近",
          items: recentItems,
        });
      }
    }

    // Pages — static source, never loading/error.
    result.push({
      id: "pages",
      heading: "页面",
      items: matchedRoutes,
    });

    // Wiki — live fetch from /api/wiki/pages + context graph neighbors.
    result.push({
      id: "wiki",
      heading: "知识库",
      items: matchedWikis,
      isLoading:
        pagesQuery.isLoading ||
        (context.kind === "wiki" && contextGraphQuery.isLoading),
      // Treat a graph-fetch error as non-fatal — the main wiki list
      // still succeeded, so we only flag error when that list failed.
      isError: pagesQuery.isError,
    });

    // Raw — include id as an extra searchable field so users can
    // paste a known id like "17" to land on it directly.
    result.push({
      id: "raw",
      heading: "素材库",
      items: matchedRaws,
      isLoading: rawQuery.isLoading,
      isError: rawQuery.isError,
    });

    // Inbox — same deal, numeric id searchable.
    result.push({
      id: "inbox",
      heading: "待整理",
      items: matchedInbox,
      isLoading: inboxQuery.isLoading,
      isError: inboxQuery.isError,
    });

    return result;
  }, [
    query,
    recent,
    rawData,
    pagesData,
    inboxData,
    contextGraph,
    context,
    rawQuery.isLoading,
    rawQuery.isError,
    pagesQuery.isLoading,
    pagesQuery.isError,
    inboxQuery.isLoading,
    inboxQuery.isError,
    contextGraphQuery.isLoading,
  ]);

  return groups;
}

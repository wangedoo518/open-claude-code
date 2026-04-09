import { useMemo, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  CLAWWIKI_ROUTES,
  type ClawWikiRoute,
  type ClawWikiSection,
} from "./clawwiki-routes";
import { listInboxEntries } from "@/features/ingest/persist";

/**
 * ClawWiki canonical Sidebar (220px expanded / 56px collapsed).
 *
 * Per `docs/clawwiki/product-design.md` §5 and `wireframes.html` §01-10,
 * the sidebar is the single navigation surface for the shell:
 *
 *   ┌─ [Logo] ClawWiki · 你的外脑
 *   │
 *   ├─ PRIMARY
 *   │  📊 Dashboard
 *   │  💬 Ask
 *   │  📨 Inbox  [—]
 *   │  📥 Raw Library
 *   │  📖 Wiki Pages
 *   │  🕸  Graph
 *   │  📐 Schema
 *   │
 *   ├─ FUNNEL
 *   │  🔗 WeChat Bridge
 *   │
 *   ├─ (spacer)
 *   └─ ⚙  Settings + collapse toggle foot
 *
 * Design decisions captured here (reviewers read this comment block):
 * - The 220px expanded width is hard-pinned from wireframes.html. Do
 *   not bump it without re-running the screenshots; downstream layouts
 *   assume `1400 - 220 = 1180` for the main pane.
 * - The 56px collapsed width shows only icons and a centered active
 *   indicator bar, matching DeepTutor reference implementations.
 * - Active state comes from `useLocation` (not zustand) so back/forward
 *   browser history and deep links work without extra plumbing.
 * - Collapse state is LOCAL to this component for S0.2. It will move
 *   into settings-store as `sidebarCollapsed` in a follow-up when we
 *   want cross-session persistence.
 */
const EXPANDED_WIDTH = 220;
const COLLAPSED_WIDTH = 56;

/** Section label text shown as a caps-lock divider. */
const SECTION_LABELS: Record<ClawWikiSection, string | null> = {
  primary: "PRIMARY",
  funnel: "FUNNEL",
  // Settings gets a visual separator (spacer) but no caps-lock label.
  settings: null,
};

function groupBySection(
  routes: readonly ClawWikiRoute[],
): Record<ClawWikiSection, ClawWikiRoute[]> {
  const grouped: Record<ClawWikiSection, ClawWikiRoute[]> = {
    primary: [],
    funnel: [],
    settings: [],
  };
  for (const r of routes) {
    grouped[r.section].push(r);
  }
  return grouped;
}

export function Sidebar() {
  const [collapsed, setCollapsed] = useState(false);
  const location = useLocation();

  const grouped = useMemo(() => groupBySection(CLAWWIKI_ROUTES), []);

  // S4: Inbox badge shows live pending count. The useQuery is cheap
  // (15-second stale time, no polling when inbox is empty), and the
  // Sidebar is always mounted so it's the natural owner of this
  // cross-page data. Any page that mutates the inbox invalidates the
  // same query key and this badge updates automatically.
  const inboxQuery = useQuery({
    queryKey: ["wiki", "inbox", "list"] as const,
    queryFn: () => listInboxEntries(),
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
  const inboxBadge = (() => {
    const pending = inboxQuery.data?.pending_count;
    if (pending === undefined || pending === 0) return undefined;
    return pending > 99 ? "99+" : String(pending);
  })();

  const width = collapsed ? COLLAPSED_WIDTH : EXPANDED_WIDTH;

  return (
    <aside
      className="flex h-full flex-shrink-0 flex-col border-r border-sidebar-border bg-sidebar-background text-sidebar-foreground transition-[width] duration-150 ease-out"
      style={{ width }}
      aria-label="ClawWiki primary navigation"
    >
      {/* Logo row */}
      <div className="flex h-14 flex-shrink-0 items-center gap-2.5 border-b border-sidebar-border px-3">
        <div className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg bg-primary font-bold text-primary-foreground">
          C
        </div>
        {!collapsed ? (
          <div className="flex flex-col leading-tight">
            <span className="text-sm font-semibold text-foreground">
              ClawWiki
            </span>
            <span className="text-[10px] text-muted-foreground">你的外脑</span>
          </div>
        ) : null}
      </div>

      {/* Scrollable nav groups */}
      <nav className="flex flex-1 flex-col gap-0.5 overflow-y-auto px-2 py-3 scrollbar-none">
        <SidebarGroup
          label={SECTION_LABELS.primary}
          items={grouped.primary}
          currentPath={location.pathname}
          collapsed={collapsed}
          badgeOverrides={{ inbox: inboxBadge }}
        />

        <div className="my-2 h-px bg-sidebar-border" aria-hidden="true" />

        <SidebarGroup
          label={SECTION_LABELS.funnel}
          items={grouped.funnel}
          currentPath={location.pathname}
          collapsed={collapsed}
        />

        {/* Spacer pushes Settings to the bottom */}
        <div className="flex-1" />

        <div className="my-2 h-px bg-sidebar-border" aria-hidden="true" />

        <SidebarGroup
          label={SECTION_LABELS.settings}
          items={grouped.settings}
          currentPath={location.pathname}
          collapsed={collapsed}
        />
      </nav>

      {/* Collapse toggle foot */}
      <button
        type="button"
        onClick={() => setCollapsed((c) => !c)}
        className="flex h-9 flex-shrink-0 items-center justify-center border-t border-sidebar-border text-[11px] text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {collapsed ? "▶" : "◀ Collapse"}
      </button>
    </aside>
  );
}

interface SidebarGroupProps {
  label: string | null;
  items: ClawWikiRoute[];
  currentPath: string;
  collapsed: boolean;
  /**
   * Per-route badge overrides keyed by `ClawWikiRoute.key`. When a
   * key is present with an `undefined` value, NO badge is shown (the
   * static placeholder from `clawwiki-routes.ts` is suppressed). Any
   * string value is used verbatim.
   */
  badgeOverrides?: Record<string, string | undefined>;
}

function SidebarGroup({
  label,
  items,
  currentPath,
  collapsed,
  badgeOverrides,
}: SidebarGroupProps) {
  if (items.length === 0) return null;
  return (
    <div className="flex flex-col gap-0.5">
      {label && !collapsed ? (
        <div className="mb-1 mt-1 px-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
          {label}
        </div>
      ) : null}
      {items.map((item) => {
        // Resolve the final badge string here, so SidebarItem can be
        // a dumb renderer. Semantics:
        //   - override key present with string value   → show string
        //   - override key present with undefined      → show NOTHING
        //     (live data exists but count is zero; we hide the "—")
        //   - override key absent                      → static fallback
        const hasOverride =
          badgeOverrides !== undefined &&
          Object.prototype.hasOwnProperty.call(badgeOverrides, item.key);
        const resolvedBadge: string | undefined = hasOverride
          ? badgeOverrides![item.key]
          : item.badge;
        return (
          <SidebarItem
            key={item.key}
            route={item}
            active={isActive(currentPath, item.path)}
            collapsed={collapsed}
            badge={resolvedBadge}
          />
        );
      })}
    </div>
  );
}

/** Match the exact path or any subpath (e.g. /ask/:sessionId → /ask). */
function isActive(currentPath: string, itemPath: string): boolean {
  if (currentPath === itemPath) return true;
  return currentPath.startsWith(`${itemPath}/`);
}

interface SidebarItemProps {
  route: ClawWikiRoute;
  active: boolean;
  collapsed: boolean;
  /**
   * Resolved badge string or `undefined` to hide the badge entirely.
   * SidebarGroup is responsible for resolving overrides vs static
   * fallback; this component is a dumb renderer.
   */
  badge: string | undefined;
}

function SidebarItem({ route, active, collapsed, badge }: SidebarItemProps) {
  const baseCls =
    "group relative flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13px] transition-colors";
  const activeCls = active
    ? "bg-sidebar-accent text-sidebar-accent-foreground font-semibold"
    : "text-sidebar-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground";

  return (
    <Link
      to={route.path}
      className={`${baseCls} ${activeCls}`}
      title={collapsed ? route.label : undefined}
      aria-current={active ? "page" : undefined}
    >
      {/* Active-state indicator bar on the left edge */}
      {active ? (
        <span
          className="absolute inset-y-1 left-0 w-[3px] rounded-r bg-primary"
          aria-hidden="true"
        />
      ) : null}

      <span className="flex-shrink-0 text-base leading-none" aria-hidden="true">
        {route.icon}
      </span>

      {!collapsed ? (
        <>
          <span className="flex-1 truncate">{route.label}</span>
          {badge ? (
            <span className="flex-shrink-0 rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-mono text-primary">
              {badge}
            </span>
          ) : null}
        </>
      ) : null}
    </Link>
  );
}

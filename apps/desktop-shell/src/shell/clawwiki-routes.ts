/**
 * Single source of truth for ClawWiki canonical shell routes.
 *
 * The shell exposes seven PRIMARY pages, one FUNNEL page (WeChat
 * Bridge), and one persistent Settings entry pinned to the Sidebar
 * foot. The Sidebar component,
 * the `ClawWikiShell` route table, and any future "go to next section"
 * keyboard shortcut all read from this list so they stay in sync.
 *
 * Each entry:
 * - `path` is the HashRouter pathname (must start with `/`)
 * - `key` is stable across renames and is used in tests / analytics
 * - `icon` uses a single-char glyph so we don't need to import a full
 *   icon library at the Sidebar level (Lucide is available but the
 *   wireframes already use these emoji, keeping parity is cheaper)
 * - `label` is the Chinese user-facing label (the product is CN-only
 *   until MVP ships)
 * - `section` groups items for Sidebar rendering ("primary" / "funnel"
 *   / "settings"); Sidebar renders section dividers based on this
 * - `sprint` tags when each surface is planned to light up — stub
 *   pages all include it so reviewers know what's coming
 * - `badge` is an optional Sidebar counter (e.g. Inbox unread) — for
 *   S0.2 all badges are static "—" placeholders
 */
export type ClawWikiSection = "primary" | "funnel" | "settings";

export interface ClawWikiRoute {
  key: string;
  path: string;
  icon: string;
  label: string;
  section: ClawWikiSection;
  sprint: string;
  badge?: string;
}

export const CLAWWIKI_ROUTES: readonly ClawWikiRoute[] = [
  {
    key: "dashboard",
    path: "/dashboard",
    icon: "📊",
    label: "Dashboard",
    section: "primary",
    sprint: "S3",
  },
  {
    key: "ask",
    path: "/ask",
    icon: "💬",
    label: "Ask",
    section: "primary",
    sprint: "S3",
  },
  {
    key: "inbox",
    path: "/inbox",
    icon: "📨",
    label: "Inbox",
    section: "primary",
    sprint: "S4",
    badge: "—",
  },
  {
    key: "raw",
    path: "/raw",
    icon: "📥",
    label: "Raw Library",
    section: "primary",
    sprint: "S1",
  },
  {
    key: "wiki",
    path: "/wiki",
    icon: "📖",
    label: "Wiki Pages",
    section: "primary",
    sprint: "S4",
  },
  {
    key: "graph",
    path: "/graph",
    icon: "🕸",
    label: "Graph",
    section: "primary",
    sprint: "S6",
  },
  {
    key: "schema",
    path: "/schema",
    icon: "📐",
    label: "Schema",
    section: "primary",
    sprint: "S6",
  },
  {
    key: "wechat",
    path: "/wechat",
    icon: "🔗",
    label: "WeChat Bridge",
    section: "funnel",
    sprint: "S5 (iLink)",
  },
  {
    key: "settings",
    path: "/settings",
    icon: "⚙️",
    label: "Settings",
    section: "settings",
    sprint: "reused",
  },
] as const;

/**
 * The route the ClawWiki shell falls back to when a legacy or unknown
 * path is visited. Dashboard is chosen per canonical §5: "用户最高频的
 * 动作是在微信发一条 → 然后回到桌面看 Inbox → 点 Ask 接着挖", and
 * Dashboard is the surface that answers "my external brain grew by
 * how much today?".
 */
export const CLAWWIKI_DEFAULT_ROUTE = "/dashboard";

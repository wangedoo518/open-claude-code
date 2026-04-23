/**
 * Single source of truth for ClawWiki canonical shell routes.
 *
 * The shell exposes user-task-oriented PRIMARY entries, plus a few
 * SECONDARY / ADVANCED routes that stay reachable by URL + palette
 * + in-page links but don't occupy a top-level slot in the sidebar.
 * The Sidebar component, the `ClawWikiShell` route table, the command
 * palette, and any future "go to next section" keyboard shortcut all
 * read from this list so they stay in sync.
 *
 * Each entry:
 * - `path` is the HashRouter pathname (must start with `/`)
 * - `key` is stable across renames and is used in tests / analytics
 * - `icon` is a Lucide component (canonical iconography per the
 *   design-system handoff; replaces the S0.2-era emoji route glyphs).
 * - `label` is the Chinese user-facing label — I4 sprint moved
 *   everything to task-oriented language (问问题 / 待整理 / 知识库
 *   etc.) so the default sidebar stops reading like a system module
 *   directory.
 * - `section` groups items for Sidebar rendering
 *     primary  — always-visible top-level task entries
 *     funnel   — always-visible entry that points at a specific flow
 *                (WeChat onboarding — separate group so users can find
 *                it even if they're not in the middle of an Ask/Wiki
 *                workflow)
 *     advanced — reachable by URL + palette + in-page links, NOT
 *                rendered in the sidebar's default top-level list
 *     settings — pinned to the sidebar foot
 * - `sprint` tags when each surface is planned to light up
 * - `badge` is an optional Sidebar counter (e.g. Inbox unread) — for
 *   S0.2 all badges are static "—" placeholders
 */
import {
  BookOpen,
  FileStack,
  Home,
  Inbox,
  Link2,
  MessageCircle,
  Network,
  Play,
  Settings,
  Sigma,
  type LucideIcon,
} from "lucide-react";

export type ClawWikiSection = "primary" | "funnel" | "advanced" | "settings";

export interface ClawWikiRoute {
  key: string;
  path: string;
  icon: LucideIcon;
  label: string;
  section: ClawWikiSection;
  sprint: string;
  badge?: string;
}

export const CLAWWIKI_ROUTES: readonly ClawWikiRoute[] = [
  {
    key: "dashboard",
    path: "/dashboard",
    icon: Home,
    label: "首页",
    section: "primary",
    sprint: "S3",
  },
  // Ask is promoted back to a top-level primary entry so the sidebar
  // reads "问问题" instead of forcing users through the old Chat/Wiki
  // mode toggle.
  {
    key: "ask",
    path: "/ask",
    icon: MessageCircle,
    label: "问问题",
    section: "primary",
    sprint: "S3",
  },
  {
    key: "inbox",
    path: "/inbox",
    icon: Inbox,
    label: "待整理",
    section: "primary",
    sprint: "S4",
    badge: "—",
  },
  {
    key: "wiki",
    path: "/wiki",
    icon: BookOpen,
    label: "知识库",
    section: "primary",
    sprint: "S4",
  },
  {
    key: "wechat",
    path: "/wechat",
    icon: Link2,
    label: "微信接入",
    section: "funnel",
    sprint: "S5 (iLink)",
  },
  // ── Advanced / secondary routes — not rendered in the default
  //    sidebar but still accessible by direct URL, command palette,
  //    and in-page links (Dashboard quick actions, Wiki article
  //    "查看关系图" button, Settings → 高级).
  {
    key: "raw",
    path: "/raw",
    icon: FileStack,
    label: "素材库",
    section: "advanced",
    sprint: "S1",
  },
  {
    key: "graph",
    path: "/graph",
    icon: Network,
    label: "关系图",
    section: "advanced",
    sprint: "S6",
  },
  {
    key: "schema",
    path: "/schema",
    icon: Sigma,
    label: "整理规则",
    section: "advanced",
    sprint: "S6",
  },
  {
    key: "settings",
    path: "/settings",
    icon: Settings,
    label: "设置",
    section: "settings",
    sprint: "reused",
  },
  // Batch E §1 — synthetic route for the command palette's "查看演示
  // 对话" entry. Its `path` is the canonical Ask page; the palette
  // action dispatcher (features/palette/actions.ts) special-cases the
  // `ask.demo` key and additionally flips `useAskUiStore.showDemo`
  // before navigating. Kept in `advanced` so the sidebar's
  // primary/funnel filter doesn't render it — command-palette-only.
  {
    key: "ask.demo",
    path: "/ask",
    icon: Play,
    label: "查看演示对话",
    section: "advanced",
    sprint: "Batch-E",
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

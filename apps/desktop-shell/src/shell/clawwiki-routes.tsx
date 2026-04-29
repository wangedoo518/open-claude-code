/**
 * Canonical ClawWiki shell route config.
 *
 * Sidebar, command palette, and the React Router table all read from this
 * file. That keeps route metadata and route implementation from drifting
 * when a new surface is added.
 */
import type { ReactNode } from "react";
import {
  BookOpen,
  Cable,
  Eye,
  FileStack,
  Home,
  Inbox,
  Link2,
  MessageCircle,
  Network,
  Play,
  Scissors,
  Settings,
  ShieldCheck,
  Sigma,
  type LucideIcon,
} from "lucide-react";
import { AskPage } from "@/features/ask/AskPage";
import { ConnectionsPage } from "@/features/connections/ConnectionsPage";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { GraphPage } from "@/features/graph/GraphPage";
import { InboxPage } from "@/features/inbox/InboxPage";
import { BreakdownPage } from "@/features/power/BreakdownPage";
import { CleanupPage } from "@/features/power/CleanupPage";
import { RawLibraryPage } from "@/features/raw/RawLibraryPage";
import { SchemaEditorPage } from "@/features/schema/SchemaEditorPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { WebViewerPage } from "@/features/viewer/WebViewerPage";
import { WeChatBridgePage } from "@/features/wechat/WeChatBridgePage";
import { ConnectWeChatPipelinePage } from "@/features/wechat-kefu/ConnectWeChatPipelinePage";
import { KnowledgeHubPage } from "@/features/wiki/KnowledgeHubPage";

export type ClawWikiSection = "daily" | "tune" | "system" | "hidden";

export interface ClawWikiRoute {
  /** Stable key for palette actions, tests, and analytics. */
  key: string;
  /** Navigation target. This is what sidebar and palette links use. */
  path: string;
  /** React Router match pattern when it differs from the navigation target. */
  routePath?: string;
  icon: LucideIcon;
  label: string;
  section: ClawWikiSection;
  sprint: string;
  badge?: string;
  /** Present when this route should be mounted into `<Routes>`. */
  render?: () => ReactNode;
}

export type ClawWikiRoutableRoute = ClawWikiRoute & {
  render: () => ReactNode;
};

export const CLAWWIKI_ROUTES: readonly ClawWikiRoute[] = [
  {
    key: "dashboard",
    path: "/",
    routePath: "/",
    icon: Home,
    label: "首页",
    section: "daily",
    sprint: "S3",
    render: () => <DashboardPage />,
  },
  {
    key: "dashboard.legacy",
    path: "/dashboard",
    icon: Home,
    label: "旧首页入口",
    section: "hidden",
    sprint: "compat",
    render: () => <DashboardPage />,
  },
  {
    key: "ask",
    path: "/ask",
    routePath: "/ask/*",
    icon: MessageCircle,
    label: "问问题",
    section: "daily",
    sprint: "S3",
    render: () => <AskPage />,
  },
  {
    key: "inbox",
    path: "/inbox",
    icon: Inbox,
    label: "待整理",
    section: "daily",
    sprint: "S4",
    badge: "•",
    render: () => <InboxPage />,
  },
  {
    key: "wiki",
    path: "/wiki",
    routePath: "/wiki/*",
    icon: BookOpen,
    label: "知识库",
    section: "daily",
    sprint: "S4",
    render: () => <KnowledgeHubPage />,
  },
  {
    key: "wechat",
    path: "/wechat",
    icon: Link2,
    label: "微信接入",
    section: "hidden",
    sprint: "S5 (iLink)",
    render: () => <WeChatBridgePage />,
  },
  {
    key: "raw",
    path: "/raw",
    routePath: "/raw/*",
    icon: FileStack,
    label: "素材库",
    section: "hidden",
    sprint: "S1",
    render: () => <RawLibraryPage />,
  },
  {
    key: "graph",
    path: "/graph",
    icon: Network,
    label: "关系图",
    section: "hidden",
    sprint: "S6",
    render: () => <GraphPage />,
  },
  {
    key: "cleanup",
    path: "/cleanup",
    icon: ShieldCheck,
    label: "清理建议",
    section: "hidden",
    sprint: "Phase 4",
    render: () => <CleanupPage />,
  },
  {
    key: "breakdown",
    path: "/breakdown",
    icon: Scissors,
    label: "页面拆解",
    section: "hidden",
    sprint: "Phase 4",
    render: () => <BreakdownPage />,
  },
  {
    key: "viewer",
    path: "/viewer",
    routePath: "/viewer/*",
    icon: Eye,
    label: "只读查看",
    section: "hidden",
    sprint: "Phase 4",
    render: () => <WebViewerPage />,
  },
  {
    key: "schema",
    path: "/schema",
    routePath: "/schema/*",
    icon: Sigma,
    label: "整理规则",
    section: "hidden",
    sprint: "S6",
    render: () => <SchemaEditorPage />,
  },
  {
    key: "rules",
    path: "/rules",
    routePath: "/rules/*",
    icon: Sigma,
    label: "规则",
    section: "tune",
    sprint: "Tolaria deep design",
    render: () => <SchemaEditorPage />,
  },
  {
    key: "connections",
    path: "/connections",
    routePath: "/connections/*",
    icon: Cable,
    label: "连接",
    section: "tune",
    sprint: "Tolaria deep design",
    render: () => <ConnectionsPage />,
  },
  {
    key: "settings",
    path: "/settings",
    icon: Settings,
    label: "设置",
    section: "system",
    sprint: "reused",
    render: () => <SettingsPage />,
  },
  {
    key: "connect-wechat",
    path: "/connect-wechat",
    icon: Link2,
    label: "连接微信",
    section: "hidden",
    sprint: "Kefu onboarding",
    render: () => <ConnectWeChatPipelinePage />,
  },
  {
    key: "ask.demo",
    path: "/ask",
    icon: Play,
    label: "查看演示对话",
    section: "hidden",
    sprint: "Batch-E",
  },
] as const;

export const CLAWWIKI_ROUTER_ROUTES: readonly ClawWikiRoutableRoute[] =
  CLAWWIKI_ROUTES.filter(
    (route): route is ClawWikiRoutableRoute =>
      typeof route.render === "function",
  );

export const CLAWWIKI_DEFAULT_ROUTE = "/";

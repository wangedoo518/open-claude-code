/**
 * Slice E28.2 — categorize a Top 3 action by its href so the row
 * can render a category icon + tone-tinted left bar. Pure fn so
 * tests can pin the contract without DOM.
 *
 * Adding a new category: append to CATEGORY_RULES (order matters —
 * first match wins). Each rule has a substring matcher and the
 * resulting category. Tone is the CSS color token used for the
 * row's 4px left bar.
 */

import type { LucideIcon } from "lucide-react";
import {
  BookOpen,
  FileSearch,
  FileStack,
  GitBranch,
  Inbox,
  MessageCircle,
  Send,
  ShieldCheck,
  Sparkles,
} from "lucide-react";

export type ActionCategoryId =
  | "inbox"
  | "vault"
  | "wiki"
  | "express"
  | "ingest"
  | "wechat"
  | "patrol"
  | "default";

export interface ActionCategory {
  id: ActionCategoryId;
  icon: LucideIcon;
  /** Tone token name (CSS variable). Used as `var(--{tone})` for
   *  the row's 3px left bar and the icon color. Kept warm to match
   *  the cream + terracotta palette — no jarring blues / pinks. */
  tone:
    | "color-primary"
    | "color-warning"
    | "color-success"
    | "color-muted-foreground";
}

const CATEGORY_RULES: ReadonlyArray<{
  test: (href: string) => boolean;
  category: ActionCategory;
}> = [
  {
    test: (h) => h.startsWith("/inbox"),
    category: { id: "inbox", icon: Inbox, tone: "color-warning" },
  },
  {
    test: (h) => h.startsWith("/connections") && h.includes("git"),
    category: { id: "vault", icon: GitBranch, tone: "color-primary" },
  },
  {
    test: (h) => h.startsWith("/raw"),
    category: { id: "ingest", icon: FileStack, tone: "color-muted-foreground" },
  },
  {
    test: (h) => h.startsWith("/wechat"),
    category: { id: "wechat", icon: MessageCircle, tone: "color-success" },
  },
  {
    test: (h) => h.startsWith("/wiki"),
    category: { id: "wiki", icon: BookOpen, tone: "color-primary" },
  },
  {
    test: (h) => h.startsWith("/ask") || h.startsWith("/drafts"),
    category: { id: "express", icon: Send, tone: "color-primary" },
  },
  {
    test: (h) => h.startsWith("/rules") || h.includes("patrol"),
    category: { id: "patrol", icon: ShieldCheck, tone: "color-warning" },
  },
];

const DEFAULT_CATEGORY: ActionCategory = {
  id: "default",
  icon: Sparkles,
  tone: "color-muted-foreground",
};

export function categorizeAction(href: string): ActionCategory {
  for (const rule of CATEGORY_RULES) {
    if (rule.test(href)) return rule.category;
  }
  // Use FileSearch fallback only if href contains "source" (lifecycle
  // patrol surface common pattern); otherwise generic.
  if (href.includes("source")) {
    return { id: "patrol", icon: FileSearch, tone: "color-warning" };
  }
  return DEFAULT_CATEGORY;
}

/**
 * row-primitives — shared leaf helpers used by multiple row/card
 * components after the DS2.x-A migration.
 *
 * DS 2.x-B product: hoists what DS 2.x-A deliberately duplicated
 * (see the `components/ds/InboxRow.tsx` + `RawEntryCard.tsx` JSDoc
 * headers and memory/observations.jsonl's two "其他代码不动" notes).
 * Now that both feature-specific row components have landed on
 * origin/main, the duplication is a known hazard the audit flagged
 * as the natural next-sprint extraction target. This file is that
 * extraction.
 *
 * Surfaces:
 *   - StatusIcon          · Inbox row leading slot (pending / approved / rejected)
 *   - translateSource     · Raw source key → 中文 label
 *   - sourceBadgeStyle    · Raw source key → pill bg/text colour pair
 *   - SourceIcon          · Raw source key → Lucide icon component
 *   - formatSize          · byte_size → human string (B / KB / MB)
 *
 * Consumers (post-DS 2.x-B hoist):
 *   - features/inbox/InboxPage.tsx        (detail pane §1 Evidence)
 *   - components/ds/InboxRow.tsx          (leading slot, non-batch mode)
 *   - features/raw/RawLibraryPage.tsx     (filter / focused-entry header
 *                                          / ExpandedDetail / FullScreenReader)
 *   - components/ds/RawEntryCard.tsx      (row source tile + badge)
 *
 * DS 2.x-A invariant: byte-identical behaviour to the pre-hoist
 * copies; every class string and label mapping carried over unchanged.
 *
 * Token sweep round 2 (Batch F task A): the source-badge palette was
 * migrated from hard-coded `rgba(34,197,94,0.12)` / `rgb(22,163,74)`
 * tuples to `color-mix(in srgb, var(--color-*) NN%, transparent)` +
 * `var(--color-*)` token references. Dark-mode overrides + any future
 * theme customisation now apply consistently (Batch E §2 showed the
 * light→dark success/warning/permission ramps are already wired; this
 * sweep puts the source badges under the same regime). Hue family is
 * preserved though the DS tokens are slightly more muted than the
 * pre-DS1.6-B tailwind defaults — intentional shift.
 */

import type { CSSProperties, FC } from "react";
import {
  AlertCircle,
  CheckCircle2,
  XCircle,
  MessageSquare,
  Globe,
  File,
  FileText,
} from "lucide-react";
import type { InboxEntry } from "@/features/ingest/types";

/* ─── Inbox status ──────────────────────────────────────────────── */

/**
 * Status badge keyed off `InboxEntry["status"]`. Historically inlined
 * in both `InboxPage.tsx` (detail pane) and `InboxRow.tsx` (list row)
 * with byte-identical bodies. Hoisted here DS 2.x-B.
 */
export const StatusIcon: FC<{ status: InboxEntry["status"] }> = ({ status }) => {
  if (status === "pending") {
    return (
      <AlertCircle
        className="size-4 shrink-0"
        style={{ color: "var(--color-warning)" }}
      />
    );
  }
  if (status === "approved") {
    return (
      <CheckCircle2
        className="size-4 shrink-0"
        style={{ color: "var(--color-success)" }}
      />
    );
  }
  return (
    <XCircle
      className="size-4 shrink-0"
      style={{ color: "var(--color-error)" }}
    />
  );
};

/* ─── Raw source helpers ────────────────────────────────────────── */

/** Translate known source labels to Chinese. */
export function translateSource(source: string): string {
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

/**
 * Color chip style per source category.
 *
 * Token mapping (Batch F · token sweep round 2):
 *   wechat family → --color-success   (DS success, olive-green #3f8f5e)
 *   url family    → --claude-blue     (DS Focus Blue = alias of --ring)
 *   file family   → --color-permission (DS permission purple #8855cc)
 *   default       → --color-muted-foreground (neutral grey)
 *
 * All backgrounds use `color-mix(in srgb, <token> 12%, transparent)`
 * which is the same compositing recipe DS1.6-B established for its
 * success / warning / error soft-fills.
 */
export function sourceBadgeStyle(source: string): {
  bg: string;
  text: string;
} {
  if (source.startsWith("wechat"))
    return {
      bg: "color-mix(in srgb, var(--color-success) 12%, transparent)",
      text: "var(--color-success)",
    };
  if (source === "url" || source === "paste-url" || source === "wechat-url")
    return {
      bg: "color-mix(in srgb, var(--claude-blue) 12%, transparent)",
      text: "var(--claude-blue)",
    };
  if (["pdf", "docx", "pptx", "image"].includes(source))
    return {
      bg: "color-mix(in srgb, var(--color-permission) 12%, transparent)",
      text: "var(--color-permission)",
    };
  return {
    bg: "color-mix(in srgb, var(--color-muted-foreground) 12%, transparent)",
    text: "var(--color-muted-foreground)",
  };
}

/** Icon per source type. */
export function SourceIcon({
  source,
  className,
  style,
}: {
  source: string;
  className?: string;
  style?: CSSProperties;
}) {
  if (source.startsWith("wechat"))
    return <MessageSquare className={className} style={style} />;
  if (source === "url" || source === "paste-url")
    return <Globe className={className} style={style} />;
  if (["pdf", "docx", "pptx", "image"].includes(source))
    return <File className={className} style={style} />;
  return <FileText className={className} style={style} />;
}

/** Format byte size to human-readable. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

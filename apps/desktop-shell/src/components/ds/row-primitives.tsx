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
 * Invariant — byte-identical behaviour to the pre-hoist copies. The
 * DS 2.x-A migration took care to preserve visual parity, so this
 * hoist is a pure move: every class string, every `rgba(...)` tuple,
 * every label mapping is carried over unchanged. If a future sprint
 * wants to switch the hard-coded source palette to `var(--color-*)`
 * tokens, that's a separate change with its own visual review.
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

/** Color chip style per source category. */
export function sourceBadgeStyle(source: string): {
  bg: string;
  text: string;
} {
  if (source.startsWith("wechat"))
    return { bg: "rgba(34,197,94,0.12)", text: "rgb(22,163,74)" };
  if (source === "url" || source === "paste-url" || source === "wechat-url")
    return { bg: "rgba(59,130,246,0.12)", text: "rgb(37,99,235)" };
  if (["pdf", "docx", "pptx", "image"].includes(source))
    return { bg: "rgba(168,85,247,0.12)", text: "rgb(147,51,234)" };
  return { bg: "rgba(156,163,175,0.12)", text: "rgb(107,114,128)" };
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

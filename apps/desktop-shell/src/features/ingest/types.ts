// S1 ingest pipeline — wire types.
//
// Mirrors the JSON shapes returned by the desktop-server `/api/wiki/raw`
// routes (handlers in `rust/crates/desktop-server/src/lib.rs`). Keep
// this file in sync when the Rust struct changes.

export type RawSource =
  | "paste"
  | "wechat-text"
  | "wechat-article"
  | "url"
  | "voice"
  | "image"
  | "pdf"
  | "pptx"
  | "docx"
  | "video"
  | "card"
  | "chat";

export interface RawEntry {
  id: number;
  filename: string;
  source: string;
  slug: string;
  /** ISO date `YYYY-MM-DD` from the filename. */
  date: string;
  source_url?: string | null;
  /** ISO-8601 datetime from the frontmatter. */
  ingested_at: string;
  byte_size: number;
}

export interface IngestRawRequest {
  source: RawSource;
  title: string;
  body: string;
  source_url?: string;
}

export interface RawListResponse {
  entries: RawEntry[];
}

export interface RawDetailResponse {
  entry: RawEntry;
  body: string;
}

// ── S4 Inbox layer ────────────────────────────────────────────────
//
// Wire types mirror the Rust enums in `wiki_store::InboxKind` /
// `InboxStatus`. Kept as string unions so we get exhaustive switches
// on the frontend. Adding a variant here and forgetting to handle it
// in the InboxPage switch triggers a TS error immediately.

export type InboxKind = "new-raw" | "conflict" | "stale" | "deprecate";
export type InboxStatus = "pending" | "approved" | "rejected";

export interface InboxEntry {
  id: number;
  kind: InboxKind;
  status: InboxStatus;
  title: string;
  description: string;
  source_raw_id?: number | null;
  created_at: string;
  resolved_at?: string | null;
}

export interface InboxListResponse {
  entries: InboxEntry[];
  pending_count: number;
  total_count: number;
}

export type InboxResolveAction = "approve" | "reject";

// ── S6 Schema layer ──────────────────────────────────────────────

export interface SchemaResponse {
  path: string;
  content: string;
  /**
   * Always `"disk"` now that `init_wiki` seeds the file on every
   * handler call. The historical `"canonical-template"` variant
   * was removed in the nit-polish pass (review finding #4).
   */
  source: "disk";
  byte_size: number;
}

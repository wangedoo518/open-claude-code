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

// ── S4 Wiki Maintainer MVP (engram-style) ────────────────────────
//
// Wire types for the maintainer flow: `propose` produces a
// `WikiPageProposal` via one `chat_completion` call, then
// `approve-with-write` persists it to `wiki/concepts/{slug}.md`
// and resolves the corresponding inbox entry atomically.
//
// Mirrors the Rust types in `wiki_maintainer::WikiPageProposal`
// and the `/api/wiki/inbox/:id/propose` response envelope.

export interface WikiPageProposal {
  /** kebab-case ASCII slug, primary key */
  slug: string;
  /** human-readable display title (may contain CJK) */
  title: string;
  /** one-line summary, ≤ 200 chars */
  summary: string;
  /** full markdown body, ≤ 200 words */
  body: string;
  /** raw/ entry id that seeded this proposal (echoed from server) */
  source_raw_id: number;
}

export interface WikiProposalResponse {
  proposal: WikiPageProposal;
  inbox_id: number;
  source_raw_id: number;
}

export interface WikiApproveWithWriteResponse {
  /** Absolute path where the concept page was written. */
  written_path: string;
  slug: string;
  /**
   * Updated inbox entry after the approve. `null` if the inbox
   * resolve failed after the page was written — the page is on
   * disk and the user can retry approval from the Inbox UI.
   */
  inbox_entry: InboxEntry | null;
}

export interface WikiPageSummary {
  slug: string;
  title: string;
  summary: string;
  source_raw_id?: number | null;
  created_at: string;
  byte_size: number;
}

export interface WikiPagesListResponse {
  pages: WikiPageSummary[];
  total_count: number;
}

export interface WikiPageDetailResponse {
  summary: WikiPageSummary;
  body: string;
}

/**
 * Shape returned by `GET /api/wiki/index` and `GET /api/wiki/log`.
 * Both special files (`wiki/index.md`, `wiki/log.md`) are plain
 * markdown with no frontmatter — the backend hands them back
 * verbatim along with a simple byte size and existence flag.
 *
 * `exists: false` means the file has never been written yet (a
 * fresh wiki). The frontend can use this to show an "empty state"
 * hint instead of an error.
 */
export interface WikiSpecialFileResponse {
  path: string;
  content: string;
  byte_size: number;
  exists: boolean;
}

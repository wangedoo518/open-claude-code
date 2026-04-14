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
 * Shape returned by `GET /api/wiki/graph` (feat T). Nodes are raw
 * entries + concept pages; edges are derived-from links from
 * concept pages to their source raws.
 */
export interface WikiGraphNode {
  id: string;
  label: string;
  kind: "raw" | "concept";
  /** Fine-grained category for semantic coloring on the graph. */
  category: "raw" | "concept" | "people" | "topic" | "compare";
}

export interface WikiGraphEdge {
  from: string;
  to: string;
  kind: "derived-from" | "references";
}

export interface WikiGraphResponse {
  nodes: WikiGraphNode[];
  edges: WikiGraphEdge[];
  raw_count: number;
  concept_count: number;
  edge_count: number;
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

/**
 * One hit in a wiki search result. Mirrors Rust's `WikiSearchHit`.
 * `score` is the computed relevance score (higher = more relevant);
 * `snippet` is a short excerpt around the first body match, or
 * empty string when the match was only in slug/title/summary.
 */
export interface WikiSearchHit {
  page: WikiPageSummary;
  score: number;
  snippet: string;
}

/**
 * Response shape for `GET /api/wiki/search?q=&limit=`.
 * `total_matches` is the count BEFORE limit truncation,
 * `hits.length` is at most `limit`.
 */
export interface WikiSearchResponse {
  query: string;
  hits: WikiSearchHit[];
  total_matches: number;
  limit: number;
}

// ── v2 types (technical-design.md §3.5–§3.9) ─────────────────────

/**
 * Record of a single absorb operation result.
 * Persisted to `{meta}/_absorb_log.json`.
 */
export interface AbsorbLogEntry {
  entry_id: number;
  timestamp: string;
  action: "create" | "update" | "skip";
  page_slug: string | null;
  page_title: string | null;
  page_category: string | null;
}

export interface AbsorbLogResponse {
  entries: AbsorbLogEntry[];
  total: number;
}

/** Reverse-link index: target slug → list of referring slugs. */
export type BacklinksIndex = Record<string, string[]>;

export interface BacklinksDetailResponse {
  slug: string;
  backlinks: Array<{ slug: string; title: string; category: string }>;
  count: number;
}

export interface BacklinksFullResponse {
  index: BacklinksIndex;
  total_pages: number;
  total_backlinks: number;
}

/** Aggregated wiki statistics (§3.9). */
export interface WikiStats {
  raw_count: number;
  wiki_count: number;
  concept_count: number;
  people_count: number;
  topic_count: number;
  compare_count: number;
  edge_count: number;
  orphan_count: number;
  inbox_pending: number;
  inbox_resolved: number;
  today_ingest_count: number;
  week_new_pages: number;
  avg_page_words: number;
  absorb_success_rate: number;
  knowledge_velocity: number;
  last_absorb_at: string | null;
}

/** Schema template metadata (§3.7). */
export interface SchemaTemplate {
  name: string;
  file_path: string;
  content: string;
}

/** POST /api/wiki/absorb response (§2.1). */
export interface AbsorbTaskResponse {
  task_id: string;
  status: "started";
  total_entries: number;
}

/** Patrol issue (§3.8). */
export interface PatrolIssue {
  kind: "orphan" | "stale" | "schema-violation" | "oversized" | "stub" | "confidence-decay" | "uncrystallized";
  page_slug: string;
  description: string;
  suggested_action: string;
}

export interface PatrolSummary {
  orphans: number;
  stale: number;
  schema_violations: number;
  oversized: number;
  stubs: number;
  confidence_decay: number;
  uncrystallized: number;
}

export interface PatrolReport {
  issues: PatrolIssue[];
  summary: PatrolSummary;
  checked_at: string;
}

/** A single source page referenced in a /query answer (§2.2). */
export interface QuerySource {
  slug: string;
  title: string;
  relevance_score: number;
  snippet: string;
}

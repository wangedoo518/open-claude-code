// S1 ingest pipeline — wire types.
//
// Mirrors the JSON shapes returned by the desktop-server `/api/wiki/raw`
// routes (handlers in `rust/crates/desktop-server/src/lib.rs`). Keep
// this file in sync when the Rust struct changes.

// Public DTOs for the neutral Wiki API layer.
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

  // ── W1 Maintainer Workbench additions (all optional) ─────────────
  //
  // Populated by M3/M4 URL ingest dedupe/refresh layers. Older
  // backends may not include these; the Workbench Evidence section
  // degrades gracefully when they're absent.

  /** Canonical URL after normalisation (utm strip / trailing slash). */
  canonical_url?: string | null;
  /** URL as originally submitted by the user (pre-canonicalisation). */
  original_url?: string | null;
  /** Hex-encoded SHA-256 of the fetched body; stable across re-fetches. */
  content_hash?: string | null;
  /**
   * The most recent `IngestDecision` that produced (or reused) this raw.
   * Shape is a serde-tagged union — see `IngestDecision` in `lib/tauri.ts`.
   */
  last_ingest_decision?: unknown;
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

/**
 * W1 Maintainer Workbench action vocabulary. Mirrors the `action`
 * field on `POST /api/wiki/inbox/{id}/maintain` (Worker B).
 *
 * - `create_new`: generate a fresh wiki page from the raw (legacy
 *   "propose + approve-with-write" flow; still the default).
 * - `update_existing`: append/merge into an existing page — requires
 *   `target_page_slug`.
 * - `reject`: discard the inbox task with a reason — requires
 *   `rejection_reason`.
 */
export type MaintainAction = "create_new" | "update_existing" | "reject";

/**
 * Outcome returned by the `maintain` endpoint. `created` / `updated`
 * carry a `target_page_slug` so the UI can deep-link the result;
 * `rejected` echoes the `rejection_reason`; `failed` surfaces
 * `error` for user-visible troubleshooting.
 */
export type MaintainOutcome = "created" | "updated" | "rejected" | "failed";

export interface InboxEntry {
  id: number;
  kind: InboxKind;
  status: InboxStatus;
  title: string;
  description: string;
  source_raw_id?: number | null;
  created_at: string;
  resolved_at?: string | null;

  // ── W1 Maintainer Workbench additions (all optional) ───────────
  //
  // These fields are populated by the maintain endpoint (Worker B)
  // and surfaced in the Workbench Result section. Older backends
  // that haven't shipped `/maintain` leave them undefined.

  /** Kebab slug the server "proposed" from the raw (pre-commit). */
  proposed_wiki_slug?: string | null;
  /** LLM-generated title for the proposal (pre-commit). */
  proposed_title?: string | null;
  /** One-line LLM summary for the proposal (pre-commit). */
  proposed_summary?: string | null;
  /** Full markdown body for the proposal (pre-commit). */
  proposed_content_markdown?: string | null;

  /** Which action the user chose in the workbench (see `MaintainAction`). */
  maintain_action?: MaintainAction | null;
  /** Outcome returned by the `maintain` endpoint. */
  maintain_outcome?: MaintainOutcome | null;
  /** For `created`/`updated` outcomes — slug of the wiki page that was written. */
  target_page_slug?: string | null;
  /** For `rejected` outcome — user-provided reason. */
  rejection_reason?: string | null;
  /** For `failed` outcome — user-visible error message. */
  maintain_error?: string | null;

  // ── W2 update_existing preview/apply additions (all optional) ──
  //
  // Populated by Worker A's /proposal endpoints. When
  // `proposal_status === "pending"`, the frontend must render the
  // diff preview (Phase 2) instead of Phase 1's "generate proposal"
  // button — including after a reload (persisted on the inbox entry,
  // so users can close/reopen the Workbench without losing the draft).
  //
  // `before_markdown_snapshot` captures the target wiki page content
  // at proposal creation time; `proposed_after_markdown` is the full
  // LLM-merged result. `proposal_summary` is a one-line change digest
  // rendered above the diff columns.

  /** Current lifecycle of the W2 update proposal. */
  proposal_status?: "pending" | "applied" | "cancelled" | null;
  /** Full markdown the LLM merged; shown as the "after" column. */
  proposed_after_markdown?: string | null;
  /** Snapshot of the target page at proposal-generation time. */
  before_markdown_snapshot?: string | null;
  /** One-line summary describing what changed. */
  proposal_summary?: string | null;
}

/**
 * W2 update_existing proposal envelope. Returned verbatim by
 * `POST /api/wiki/inbox/{id}/proposal`. The frontend stores it
 * in local state after creation and falls back to reconstructing
 * it from the `proposal_*` fields on `InboxEntry` across reloads.
 */
export interface UpdateProposal {
  target_slug: string;
  before_markdown: string;
  after_markdown: string;
  summary: string;
  /** Epoch seconds when the proposal was generated on the server. */
  generated_at: number;
}

/**
 * Request body for `POST /api/wiki/inbox/{id}/maintain`. Aligned with
 * Worker B's contract.
 */
export interface MaintainRequest {
  action: MaintainAction;
  /** Required when `action === "update_existing"`. */
  target_page_slug?: string;
  /** Required when `action === "reject"`. Minimum 4 chars enforced client-side. */
  rejection_reason?: string;
}

/**
 * Response envelope for `POST /api/wiki/inbox/{id}/maintain`. Only
 * `outcome` is always present; the other fields are populated
 * conditionally per the rules in `MaintainOutcome`.
 */
export interface MaintainResponse {
  outcome: MaintainOutcome;
  target_page_slug?: string | null;
  rejection_reason?: string | null;
  error?: string | null;
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
  /** optional conflict signal; non-empty means the raw should be reviewed in Inbox */
  conflict_with?: string[];
  /** short reason paired with conflict_with */
  conflict_reason?: string | null;
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
  purpose?: string[];
  source_raw_id?: number | null;
  created_at: string;
  byte_size: number;
  category?: "concept" | "people" | "topic" | "compare" | string;
  confidence?: number;
  last_verified?: string | null;
}

export interface WikiPagesListResponse {
  pages: WikiPageSummary[];
  total_count: number;
}

export interface WikiPageDetailResponse {
  summary: WikiPageSummary;
  body: string;
  /** Complete markdown file including YAML frontmatter. */
  content?: string;
}

export interface WikiPageWriteResponse extends WikiPageDetailResponse {
  ok: boolean;
  path: string;
  byte_size: number;
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

export interface VaultGitChange {
  path: string;
  xy: string;
  staged: string;
  unstaged: string;
}

export interface VaultGitStatus {
  vault_path: string;
  git_available: boolean;
  initialized: boolean;
  branch?: string | null;
  upstream?: string | null;
  ahead: number;
  behind: number;
  dirty: boolean;
  changed_count: number;
  staged_count: number;
  unstaged_count: number;
  untracked_count: number;
  remote_connected: boolean;
  remote_name?: string | null;
  remote_url_redacted?: string | null;
  last_commit?: string | null;
  changes: VaultGitChange[];
}

export interface VaultGitDiff {
  vault_path: string;
  staged: boolean;
  diff: string;
  byte_size: number;
  sections: VaultGitDiffSection[];
  truncated: boolean;
}

export interface VaultGitDiffSection {
  path: string;
  kind: "tracked" | "staged" | "untracked" | string;
  diff: string;
  byte_size: number;
  hunks: VaultGitDiffHunk[];
  truncated: boolean;
}

export interface VaultGitDiffHunk {
  header: string;
  old_start?: number | null;
  old_lines?: number | null;
  new_start?: number | null;
  new_lines?: number | null;
  lines: VaultGitDiffLine[];
}

export interface VaultGitDiffLine {
  kind: "context" | "add" | "remove" | "meta" | string;
  old_line?: number | null;
  new_line?: number | null;
  text: string;
}

export interface VaultGitCommitResult {
  ok: boolean;
  commit: string;
  summary: string;
  status: VaultGitStatus;
}

export interface VaultGitSyncResult {
  ok: boolean;
  operation: "pull" | "push" | string;
  summary: string;
  status: VaultGitStatus;
}

export interface VaultGitRemoteConfigResult {
  ok: boolean;
  remote: string;
  remote_url_redacted: string;
  status: VaultGitStatus;
}

export interface VaultGitDiscardResult {
  ok: boolean;
  path: string;
  mode: "tracked" | "untracked" | "hunk" | string;
  summary: string;
  status: VaultGitStatus;
}

export interface VaultGitAuditLog {
  vault_path: string;
  entries: VaultGitAuditEntry[];
}

export interface VaultGitAuditEntry {
  timestamp_ms: number;
  operation: string;
  summary: string;
  path?: string | null;
  hunk_index?: number | null;
  commit?: string | null;
  remote?: string | null;
}

export type ExternalAiGrantLevel = "session" | "permanent";

export interface ExternalAiWriteGrant {
  id: string;
  level: ExternalAiGrantLevel;
  scope: string;
  note?: string | null;
  created_at: string;
  expires_at?: string | null;
  enabled: boolean;
}

export interface ExternalAiWritePolicy {
  version: number;
  updated_at: string;
  grants: ExternalAiWriteGrant[];
}

export interface ExternalAiWriteGrantResponse {
  ok: boolean;
  grant: ExternalAiWriteGrant;
  policy?: ExternalAiWritePolicy | null;
}

export interface ExternalAiWriteRevokeResponse {
  ok: boolean;
  policy: ExternalAiWritePolicy;
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

export interface PatrolQualitySample {
  page_slug: string;
  title: string;
  confidence: number;
  last_verified?: string | null;
  reason: string;
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
  quality_samples?: PatrolQualitySample[];
  checked_at: string;
}

export interface CleanupProposal {
  issue_kind: string;
  page_slug: string;
  title: string;
  description: string;
  suggested_action: string;
  inbox_action: string;
}

export interface CleanupResponse extends PatrolReport {
  cleanup_proposals: CleanupProposal[];
  inbox_created: number;
  applied: boolean;
}

export interface BreakdownTarget {
  slug: string;
  title: string;
  summary: string;
  body: string;
  word_count: number;
}

export interface BreakdownResponse {
  source_slug: string;
  source_title: string;
  source_word_count: number;
  reason: string;
  targets: BreakdownTarget[];
  applied: boolean;
  written_paths: string[];
}

/** A single source page referenced in a wiki query answer. */
export interface QuerySource {
  slug: string;
  title: string;
  relevance_score: number;
  snippet: string;
}

// Workbench API extensions formerly exported from `lib/tauri.ts`.

export interface BatchFailedItem {
  id: number;
  error: string;
}

export interface BatchResolveInboxResponse {
  success: number[];
  failed: BatchFailedItem[];
  total: number;
  processed: number;
}

export interface CombinedProposalSource {
  inbox_id: number;
  title: string;
  source_raw_id?: number | null;
}

export interface CombinedProposalRequest {
  target_slug: string;
  inbox_ids: number[];
}

export interface CombinedProposalResponse {
  target_slug: string;
  inbox_ids: number[];
  before_markdown: string;
  after_markdown: string;
  summary: string;
  before_hash: string;
  generated_at: number;
  source_titles: CombinedProposalSource[];
}

export interface CombinedApplyRequest {
  target_slug: string;
  inbox_ids: number[];
  expected_before_hash: string;
  after_markdown: string;
  summary: string;
}

export type CombinedApplyOutcome =
  | "applied"
  | "partial_applied"
  | "concurrent_edit"
  | "stale_inbox";

export interface CombinedApplyResponse {
  outcome: CombinedApplyOutcome;
  target_page_slug: string;
  applied_inbox_ids: number[];
  failed_inbox_ids?: number[];
  audit_entry: string;
}

export type CandidateTier = "strong" | "likely" | "weak";

export type CandidateSource =
  | "existing_target"
  | "existing_proposed"
  | "resolved";

export interface CandidateReason {
  code: string;
  weight: number;
  detail: string;
}

export interface TargetCandidate {
  slug: string;
  title: string;
  score: number;
  tier: CandidateTier;
  source: CandidateSource;
  reasons: CandidateReason[];
}

export interface InboxCandidatesResponse {
  inbox_id: number;
  candidates: TargetCandidate[];
}

export interface RelatedPageHit {
  slug: string;
  title: string;
  category: string;
  summary?: string;
  reasons: string[];
  score: number;
}

export interface PageGraphNode {
  slug: string;
  title: string;
  category: string;
}

export interface PageGraph {
  slug: string;
  title: string;
  category: string;
  summary?: string;
  outgoing: PageGraphNode[];
  backlinks: PageGraphNode[];
  related: RelatedPageHit[];
}

export type IngestDecision =
  | { kind: "created_new" }
  | { kind: "reused_with_pending_inbox"; reason: string }
  | { kind: "reused_approved"; reason: string }
  | { kind: "reused_after_reject"; reason: string }
  | { kind: "reused_silent"; reason: string }
  | { kind: "explicit_reingest"; previous_raw_id: number }
  | {
      kind: "refreshed_content";
      previous_raw_id: number;
      previous_content_hash: string;
    }
  | {
      kind: "content_duplicate";
      matching_raw_id: number;
      matching_url: string;
    };

export type RecentIngestOutcomeKind =
  | "ingested"
  | "reused_existing"
  | "inbox_suppressed"
  | "fallback_to_text"
  | "rejected_quality"
  | "fetch_failed"
  | "prerequisite_missing"
  | "invalid_url";

export interface RecentIngestEntry {
  timestamp_ms: number;
  canonical_url: string;
  original_url: string;
  entry_point: string;
  outcome_kind: RecentIngestOutcomeKind;
  decision?: IngestDecision | null;
  raw_id?: number | null;
  inbox_id?: number | null;
  adapter?: string | null;
  duration_ms?: number | null;
  summary: string;
  decision_reason?: string | null;
  content_hash?: string | null;
  content_hash_hit?: boolean | null;
}

export interface RecentIngestStats {
  by_kind: Record<string, number>;
  by_entry_point: Record<string, number>;
}

export interface RecentIngestResponse {
  decisions: RecentIngestEntry[];
  total: number;
  capacity: number;
  stats?: RecentIngestStats;
}

export type LineageEventType =
  | "raw_written"
  | "inbox_appended"
  | "proposal_generated"
  | "wiki_page_applied"
  | "combined_wiki_page_applied"
  | "inbox_rejected"
  | "wechat_message_received"
  | "url_ingested";

export type LineageRef =
  | { kind: "raw"; id: number }
  | { kind: "inbox"; id: number }
  | { kind: "wiki_page"; slug: string; title?: string }
  | { kind: "wechat_message"; event_key: string }
  | { kind: "url_source"; canonical: string };

export interface LineageEvent {
  event_id: string;
  event_type: LineageEventType;
  timestamp_ms: number;
  upstream: LineageRef[];
  downstream: LineageRef[];
  display_title: string;
  metadata: Record<string, unknown>;
}

export interface WikiLineageResponse {
  events: LineageEvent[];
  total_count: number;
}

export interface InboxLineageResponse {
  upstream_events: LineageEvent[];
  downstream_events: LineageEvent[];
}

export interface RawLineageResponse {
  events: LineageEvent[];
}

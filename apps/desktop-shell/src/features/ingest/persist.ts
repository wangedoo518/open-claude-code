// S1 ingest pipeline — HTTP wrappers for the wiki/raw layer.
//
// Thin re-exports of `fetchJson` against the desktop-server routes
// added in S1.2 (`rust/crates/desktop-server/src/lib.rs`). All ingest
// adapters in `features/ingest/adapters/*` end up calling
// `ingestRawEntry` here so the on-disk write happens in exactly one
// place.

import { fetchJson } from "@/lib/desktop/transport";
import type {
  AbsorbLogResponse,
  AbsorbTaskResponse,
  BacklinksFullResponse,
  InboxEntry,
  InboxListResponse,
  InboxResolveAction,
  IngestRawRequest,
  PatrolReport,
  RawDetailResponse,
  RawEntry,
  RawListResponse,
  SchemaResponse,
  SchemaTemplate,
  WikiApproveWithWriteResponse,
  WikiGraphResponse,
  WikiPageDetailResponse,
  WikiPageProposal,
  WikiPagesListResponse,
  WikiProposalResponse,
  WikiSearchResponse,
  WikiSpecialFileResponse,
  WikiStats,
} from "./types";

/**
 * POST `/api/wiki/raw` — write a single entry under `~/.clawwiki/raw/`.
 *
 * Returns the resulting `RawEntry` so the caller can splice an
 * optimistic row into the Raw Library list before the next refetch.
 */
export async function ingestRawEntry(
  request: IngestRawRequest,
): Promise<RawEntry> {
  return fetchJson<RawEntry>("/api/wiki/raw", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

/**
 * GET `/api/wiki/raw` — list every raw entry, sorted by id ascending.
 *
 * Empty wiki returns `{ entries: [] }`. Never throws on missing
 * directory; the backend creates `~/.clawwiki/raw/` lazily on first
 * call.
 */
export async function listRawEntries(): Promise<RawListResponse> {
  return fetchJson<RawListResponse>("/api/wiki/raw");
}

/**
 * GET `/api/wiki/raw/:id` — fetch one entry's metadata + markdown body.
 * Throws on 404.
 */
export async function getRawEntry(id: number): Promise<RawDetailResponse> {
  return fetchJson<RawDetailResponse>(`/api/wiki/raw/${id}`);
}

// ── S4 Inbox ───────────────────────────────────────────────────

/** GET `/api/wiki/inbox` — list every inbox task with counts. */
export async function listInboxEntries(): Promise<InboxListResponse> {
  return fetchJson<InboxListResponse>("/api/wiki/inbox");
}

/**
 * POST `/api/wiki/inbox/:id/resolve` — approve or reject a pending
 * task. Server validates the action and returns the updated entry.
 */
export async function resolveInboxEntry(
  id: number,
  action: InboxResolveAction,
): Promise<InboxEntry> {
  const response = await fetchJson<{ entry: InboxEntry }>(
    `/api/wiki/inbox/${id}/resolve`,
    {
      method: "POST",
      body: JSON.stringify({ action }),
    },
  );
  return response.entry;
}

/** GET `/api/wiki/schema` — read `schema/CLAUDE.md` verbatim. */
export async function getWikiSchema(): Promise<SchemaResponse> {
  return fetchJson<SchemaResponse>("/api/wiki/schema");
}

/**
 * PUT `/api/wiki/schema` (feat M) — overwrite `schema/CLAUDE.md`
 * with new content. Returns the new byte size and disk path.
 * Throws on empty content or disk write failure.
 */
export async function putWikiSchema(content: string): Promise<{
  path: string;
  byte_size: number;
  ok: boolean;
}> {
  return fetchJson<{ path: string; byte_size: number; ok: boolean }>(
    "/api/wiki/schema",
    {
      method: "PUT",
      body: JSON.stringify({ content }),
    },
  );
}

// ── S4 Wiki Maintainer MVP ────────────────────────────────────

/**
 * POST `/api/wiki/inbox/:id/propose` — fires one chat_completion
 * through the Codex broker to produce a `WikiPageProposal` for the
 * raw entry referenced by this inbox task. Does NOT touch disk.
 *
 * Error handling: on 503 (empty broker pool) the caller should
 * show a "add a Codex account" CTA; on 502 (LLM returned bad JSON
 * or invalid shape) the caller should surface the message to the
 * user. `fetchJson` throws on any non-2xx; inspect the thrown
 * error's `.status` if available.
 */
export async function proposeForInboxEntry(
  id: number,
): Promise<WikiProposalResponse> {
  return fetchJson<WikiProposalResponse>(
    `/api/wiki/inbox/${id}/propose`,
    { method: "POST" },
  );
}

/**
 * POST `/api/wiki/inbox/:id/approve-with-write` — persist the
 * proposal to `wiki/concepts/{slug}.md` and flip the inbox entry
 * to `approved`. The frontend re-sends the proposal body because
 * the server does not cache proposals between requests.
 */
export async function approveInboxWithWrite(
  id: number,
  proposal: WikiPageProposal,
): Promise<WikiApproveWithWriteResponse> {
  return fetchJson<WikiApproveWithWriteResponse>(
    `/api/wiki/inbox/${id}/approve-with-write`,
    {
      method: "POST",
      body: JSON.stringify({ proposal }),
    },
  );
}

/** GET `/api/wiki/pages` — list every concept page (no body text). */
export async function listWikiPages(): Promise<WikiPagesListResponse> {
  return fetchJson<WikiPagesListResponse>("/api/wiki/pages");
}

/** GET `/api/wiki/pages/:slug` — fetch a single concept page. */
export async function getWikiPage(slug: string): Promise<WikiPageDetailResponse> {
  return fetchJson<WikiPageDetailResponse>(
    `/api/wiki/pages/${encodeURIComponent(slug)}`,
  );
}

/**
 * GET `/api/wiki/index` — read the auto-maintained content catalog.
 * Returns `exists: false` when the wiki has never been written to
 * (fresh install). Karpathy llm-wiki.md §"Indexing and logging".
 */
export async function getWikiIndex(): Promise<WikiSpecialFileResponse> {
  return fetchJson<WikiSpecialFileResponse>("/api/wiki/index");
}

/**
 * GET `/api/wiki/log` — read the chronological append-only audit
 * trail. Returns `exists: false` when the wiki has never been
 * written to.
 */
export async function getWikiLog(): Promise<WikiSpecialFileResponse> {
  return fetchJson<WikiSpecialFileResponse>("/api/wiki/log");
}

/**
 * GET `/api/wiki/graph` (feat T) — read the wiki graph (raw +
 * concept nodes + derived-from edges). Used by the Graph page to
 * render the cognitive web.
 */
export async function getWikiGraph(): Promise<WikiGraphResponse> {
  return fetchJson<WikiGraphResponse>("/api/wiki/graph");
}

/**
 * GET `/api/wiki/search?q=&limit=` — substring search with
 * weighted field scoring. Empty/whitespace query is valid and
 * returns `{ hits: [], total_matches: 0 }` — the frontend can
 * call it unguarded during debouncing.
 *
 * Results are pre-sorted by score desc, then slug asc for stable
 * tiebreak. `total_matches` is the count BEFORE limit truncation
 * (useful for "X of Y" display).
 */
export async function searchWikiPages(
  query: string,
  limit = 20,
): Promise<WikiSearchResponse> {
  const params = new URLSearchParams();
  if (query) params.set("q", query);
  params.set("limit", String(limit));
  return fetchJson<WikiSearchResponse>(`/api/wiki/search?${params.toString()}`);
}

// ── v2 SKILL API (technical-design.md §2.1–§2.9) ─────────────────

/** POST `/api/wiki/absorb` — trigger batch absorb (§2.1). */
export async function triggerAbsorb(
  entryIds?: number[],
): Promise<AbsorbTaskResponse> {
  return fetchJson<AbsorbTaskResponse>("/api/wiki/absorb", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ entry_ids: entryIds ?? null }),
  });
}

/** GET `/api/wiki/absorb-log` — paginated absorb log (§2.5). */
export async function getAbsorbLog(
  limit = 50,
  offset = 0,
): Promise<AbsorbLogResponse> {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  return fetchJson<AbsorbLogResponse>(`/api/wiki/absorb-log?${params.toString()}`);
}

/** GET `/api/wiki/backlinks` — full backlinks index (§2.6). */
export async function getBacklinksIndex(): Promise<BacklinksFullResponse> {
  return fetchJson<BacklinksFullResponse>("/api/wiki/backlinks");
}

/** GET `/api/wiki/stats` — aggregated wiki statistics (§2.7). */
export async function getWikiStats(): Promise<WikiStats> {
  return fetchJson<WikiStats>("/api/wiki/stats");
}

/** GET `/api/wiki/schema/templates` — schema templates (§2.9). */
export async function getSchemaTemplates(): Promise<SchemaTemplate[]> {
  return fetchJson<SchemaTemplate[]>("/api/wiki/schema/templates");
}

/**
 * POST `/api/wiki/query` — wiki-grounded Q&A (§2.2).
 * Returns a raw `fetch` Response for SSE streaming.
 */
/** POST `/api/wiki/patrol` — run full patrol (§2.4). */
export async function triggerPatrol(): Promise<PatrolReport> {
  return fetchJson<PatrolReport>("/api/wiki/patrol", { method: "POST" });
}

/** GET `/api/wiki/patrol/report` — latest patrol report (§2.8). */
export async function getPatrolReport(): Promise<PatrolReport | null> {
  return fetchJson<PatrolReport | null>("/api/wiki/patrol/report");
}

export async function queryWiki(question: string, maxSources = 5): Promise<Response> {
  const { getDesktopApiBase } = await import("@/lib/desktop/bootstrap");
  const base = await getDesktopApiBase();
  return fetch(`${base}/api/wiki/query`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ question, max_sources: maxSources }),
  });
}

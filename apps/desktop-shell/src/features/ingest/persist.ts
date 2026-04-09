// S1 ingest pipeline — HTTP wrappers for the wiki/raw layer.
//
// Thin re-exports of `fetchJson` against the desktop-server routes
// added in S1.2 (`rust/crates/desktop-server/src/lib.rs`). All ingest
// adapters in `features/ingest/adapters/*` end up calling
// `ingestRawEntry` here so the on-disk write happens in exactly one
// place.

import { fetchJson } from "@/lib/desktop/transport";
import type {
  InboxEntry,
  InboxListResponse,
  InboxResolveAction,
  IngestRawRequest,
  RawDetailResponse,
  RawEntry,
  RawListResponse,
  SchemaResponse,
  WikiApproveWithWriteResponse,
  WikiPageDetailResponse,
  WikiPageProposal,
  WikiPagesListResponse,
  WikiProposalResponse,
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

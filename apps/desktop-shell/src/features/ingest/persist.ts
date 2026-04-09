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

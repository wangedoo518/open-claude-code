// S1 ingest pipeline — HTTP wrappers for the wiki/raw layer.
//
// Thin re-exports of `fetchJson` against the desktop-server wiki routes.
// All ingest
// Callers import this module from `src/api/wiki`; ingest-specific adapters
// can still call `ingestRawEntry` here when they need raw writes.

import { fetchJson } from "@/lib/desktop/transport";
import { getDesktopApiBase } from "@/lib/desktop/bootstrap";
import type {
  AbsorbLogResponse,
  AbsorbTaskResponse,
  BacklinksFullResponse,
  BreakdownResponse,
  CleanupResponse,
  ExternalAiGrantLevel,
  ExternalAiWriteGrantResponse,
  ExternalAiWritePolicy,
  ExternalAiWriteRevokeResponse,
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
  UpdateProposal,
  VaultGitCommitResult,
  VaultGitDiscardResult,
  VaultGitDiff,
  VaultGitRemoteConfigResult,
  VaultGitStatus,
  VaultGitSyncResult,
  WikiApproveWithWriteResponse,
  WikiGraphResponse,
  WikiPageDetailResponse,
  WikiPageProposal,
  WikiPagesListResponse,
  WikiPageWriteResponse,
  WikiProposalResponse,
  WikiSearchResponse,
  WikiSpecialFileResponse,
  WikiStats,
} from "@/api/wiki/types";

export interface WechatRefetchResponse {
  ok: boolean;
  title?: string;
  markdown?: string;
  source?: string;
  raw_id?: number;
  inbox_id?: number | null;
  dedupe?: boolean;
  decision?: string;
  reason?: string;
  error?: string;
  missing_prerequisite?: string;
}

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

export async function refetchWechatArticle(
  url: string,
): Promise<WechatRefetchResponse> {
  const result = await fetchJson<WechatRefetchResponse>(
    "/api/desktop/wechat-fetch",
    {
      method: "POST",
      body: JSON.stringify({ url, ingest: true, force: true }),
    },
    600_000,
  );
  if (!result.ok) {
    throw new Error(
      result.error || result.reason || result.missing_prerequisite || "WeChat refetch failed",
    );
  }
  return result;
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

// ── Q1 Inbox Queue Intelligence: batch resolve ───────────────────
//
// `POST /api/wiki/inbox/batch/resolve` — resolve many inbox entries
// in one HTTP round trip. Consumed by the InboxPage Batch Triage
// mode (Worker B) when a user multi-selects pending tasks and hits
// "批量驳回". The backend (Worker A, `desktop-server::lib.rs`) loops
// over `wiki_store::resolve_inbox_entry` with partial-success
// semantics, so we surface `success[]` + `failed[]` verbatim.
//
// Q1 MVP only accepts `"reject"` (backend returns 400 for approve).
// `"approve"` is included in the TS signature for forward
// compatibility — a later sprint can relax the server guard without
// a breaking frontend change.
//
// Fallback — if the endpoint 404s (older dev server running without
// the Q1 handler), we degrade to N parallel single-id calls via
// `resolveInboxEntry`, collecting the same `{success, failed}`
// shape. This lets the UI keep working while the backend catches
// up. Any other non-2xx from the batch endpoint propagates as an
// `Error` (caller shows the toast).

export interface BatchResolveInboxResult {
  success: number[];
  failed: Array<{ id: number; error: string }>;
}

export async function batchResolveInboxEntries(
  ids: number[],
  action: "reject" | "approve",
  reason?: string,
): Promise<BatchResolveInboxResult> {
  // Direct fetch (not fetchJson) so we can catch a 404 and fall back
  // to per-id single-resolve without the generic error-message path
  // swallowing the status. Imports resolved lazily to match the
  // pattern used by `queryWiki` / `cancelProposal`.
  const base = await getDesktopApiBase();
  const response = await fetch(`${base}/api/wiki/inbox/batch/resolve`, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ ids, action, reason }),
  });

  if (response.ok) {
    const payload = (await response.json()) as {
      success?: number[];
      failed?: Array<{ id: number; error: string }>;
    };
    return {
      success: payload.success ?? [],
      failed: payload.failed ?? [],
    };
  }

  // Backwards-compat fallback: old server → fan out to single resolve.
  if (response.status === 404) {
    const results = await Promise.allSettled(
      ids.map((id) => resolveInboxEntry(id, action)),
    );
    const success: number[] = [];
    const failed: Array<{ id: number; error: string }> = [];
    results.forEach((r, i) => {
      if (r.status === "fulfilled") {
        success.push(ids[i]);
      } else {
        failed.push({
          id: ids[i],
          error: r.reason instanceof Error ? r.reason.message : String(r.reason),
        });
      }
    });
    return { success, failed };
  }

  // Other non-2xx — surface the server error message.
  let message = `batchResolveInboxEntries failed with status ${response.status}`;
  try {
    const text = await response.text();
    if (text) message = text;
  } catch {
    // fall through
  }
  throw new Error(message);
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

/** PUT `/api/wiki/pages/:slug` — overwrite a wiki page's full markdown. */
export async function putWikiPage(
  slug: string,
  content: string,
): Promise<WikiPageWriteResponse> {
  return fetchJson<WikiPageWriteResponse>(
    `/api/wiki/pages/${encodeURIComponent(slug)}`,
    {
      method: "PUT",
      body: JSON.stringify({ content }),
    },
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

/** POST `/api/wiki/cleanup?apply=` - preview/apply patrol-backed cleanup. */
export async function triggerCleanup(apply = false): Promise<CleanupResponse> {
  return fetchJson<CleanupResponse>(`/api/wiki/cleanup?apply=${apply}`, {
    method: "POST",
  });
}

/** POST `/api/wiki/breakdown` - preview/apply deterministic page split. */
export async function breakdownWikiPage(
  slug: string,
  options?: { apply?: boolean; maxTargets?: number },
): Promise<BreakdownResponse> {
  return fetchJson<BreakdownResponse>("/api/wiki/breakdown", {
    method: "POST",
    body: JSON.stringify({
      slug,
      apply: options?.apply ?? false,
      max_targets: options?.maxTargets,
    }),
  });
}

/** GET `/api/wiki/patrol/report` — latest patrol report (§2.8). */
export async function getPatrolReport(): Promise<PatrolReport | null> {
  return fetchJson<PatrolReport | null>("/api/wiki/patrol/report");
}

/** GET `/api/wiki/git/status` — live Buddy Vault Git state. */
export async function getVaultGitStatus(): Promise<VaultGitStatus> {
  return fetchJson<VaultGitStatus>("/api/wiki/git/status");
}

/** GET `/api/wiki/git/diff` — unstaged or staged Buddy Vault diff. */
export async function getVaultGitDiff(staged = false): Promise<VaultGitDiff> {
  const params = new URLSearchParams();
  if (staged) params.set("staged", "true");
  const query = params.toString();
  return fetchJson<VaultGitDiff>(`/api/wiki/git/diff${query ? `?${query}` : ""}`);
}

/** POST `/api/wiki/git/commit` — stage all Vault changes and create a checkpoint. */
export async function commitVaultGit(message: string): Promise<VaultGitCommitResult> {
  return fetchJson<VaultGitCommitResult>("/api/wiki/git/commit", {
    method: "POST",
    body: JSON.stringify({ message }),
  });
}

/** POST `/api/wiki/git/pull` — fast-forward Buddy Vault from its remote. */
export async function pullVaultGit(): Promise<VaultGitSyncResult> {
  return fetchJson<VaultGitSyncResult>("/api/wiki/git/pull", {
    method: "POST",
  });
}

/** POST `/api/wiki/git/push` — push Buddy Vault checkpoints to its remote. */
export async function pushVaultGit(): Promise<VaultGitSyncResult> {
  return fetchJson<VaultGitSyncResult>("/api/wiki/git/push", {
    method: "POST",
  });
}

/** POST `/api/wiki/git/remote` — add or replace the Buddy Vault remote URL. */
export async function setVaultGitRemote(request: {
  remote?: string;
  url: string;
}): Promise<VaultGitRemoteConfigResult> {
  return fetchJson<VaultGitRemoteConfigResult>("/api/wiki/git/remote", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

/** POST `/api/wiki/git/discard` — discard one dirty Buddy Vault path. */
export async function discardVaultGitPath(path: string): Promise<VaultGitDiscardResult> {
  return fetchJson<VaultGitDiscardResult>("/api/wiki/git/discard", {
    method: "POST",
    body: JSON.stringify({ path }),
  });
}

/** POST `/api/wiki/git/discard-hunk` — discard one tracked unstaged hunk. */
export async function discardVaultGitHunk(request: {
  path: string;
  hunk_index: number;
  hunk_header?: string;
}): Promise<VaultGitDiscardResult> {
  return fetchJson<VaultGitDiscardResult>("/api/wiki/git/discard-hunk", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

/** GET `/api/wiki/external-ai/write-policy` — controlled-write grants. */
export async function getExternalAiWritePolicy(): Promise<ExternalAiWritePolicy> {
  return fetchJson<ExternalAiWritePolicy>("/api/wiki/external-ai/write-policy");
}

/** POST `/api/wiki/external-ai/write-policy/grants` — authorize a scoped grant. */
export async function addExternalAiWriteGrant(request: {
  level: ExternalAiGrantLevel;
  scope: string;
  note?: string;
  expires_at?: string;
}): Promise<ExternalAiWriteGrantResponse> {
  return fetchJson<ExternalAiWriteGrantResponse>(
    "/api/wiki/external-ai/write-policy/grants",
    {
      method: "POST",
      body: JSON.stringify(request),
    },
  );
}

/** DELETE `/api/wiki/external-ai/write-policy/grants/:id` — revoke a grant. */
export async function revokeExternalAiWriteGrant(
  id: string,
): Promise<ExternalAiWriteRevokeResponse> {
  return fetchJson<ExternalAiWriteRevokeResponse>(
    `/api/wiki/external-ai/write-policy/grants/${encodeURIComponent(id)}`,
    { method: "DELETE" },
  );
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

// ── W2 update_existing preview/apply flow ────────────────────────
//
// Thin HTTP wrappers over Worker A's proposal endpoints. The flow:
//   1. user selects update_existing + picks target slug
//   2. `createProposal` → LLM generates merged markdown; returned as
//      `UpdateProposal`; backend persists `proposal_status=pending` on
//      the inbox entry so a reload restores the Phase-2 UI.
//   3. user reviews the diff and either:
//        - `applyProposal` → writes to disk, flips status to "applied"
//          and resolves the inbox entry (returns `MaintainOutcome`-ish).
//        - `cancelProposal` → drops proposal_*, returns status 200.

/**
 * POST `/api/wiki/inbox/{id}/proposal` — kick off an LLM merge for
 * the inbox entry against the given target wiki page. Does NOT touch
 * the page on disk; the user must explicitly `applyProposal`.
 */
export async function createProposal(
  inboxId: number,
  targetSlug: string,
): Promise<UpdateProposal> {
  return fetchJson<UpdateProposal>(
    `/api/wiki/inbox/${inboxId}/proposal`,
    {
      method: "POST",
      body: JSON.stringify({ target_slug: targetSlug }),
    },
  );
}

/**
 * POST `/api/wiki/inbox/{id}/proposal/apply` — commit the pending
 * proposal to the target wiki page, marking the inbox entry as
 * `approved` with `maintain_outcome: "updated"`.
 */
export async function applyProposal(
  inboxId: number,
): Promise<{ outcome: string; target_page_slug: string }> {
  return fetchJson<{ outcome: string; target_page_slug: string }>(
    `/api/wiki/inbox/${inboxId}/proposal/apply`,
    { method: "POST" },
  );
}

/**
 * POST `/api/wiki/inbox/{id}/proposal/cancel` — discard the pending
 * proposal (server clears `proposal_*` fields). Returns 200 with no
 * JSON body, so we use a direct `fetch` instead of `fetchJson` to
 * avoid tripping its mandatory `.json()` parse.
 */
export async function cancelProposal(inboxId: number): Promise<void> {
  const base = await getDesktopApiBase();
  const response = await fetch(
    `${base}/api/wiki/inbox/${inboxId}/proposal/cancel`,
    {
      method: "POST",
      headers: { Accept: "application/json" },
    },
  );
  if (!response.ok) {
    let message = `Request failed with status ${response.status}`;
    try {
      const text = await response.text();
      if (text) message = text;
    } catch {
      // fall through with default message
    }
    throw new Error(message);
  }
}

// ── Q2 Target Candidates: fetch with client-side fallback ────────
//
// `GET /api/wiki/inbox/{id}/candidates` — returns the top-3 target
// wiki pages the server thinks this inbox entry should merge into
// (shape: `InboxCandidatesResponse` in
// `domain/wiki/candidate-scoring.ts`). Backs the Q2
// `TargetCandidatePicker` chip row.
//
// Falls back to the TS-side scorer (`target-resolver.ts`) when:
//   • the server 404s (older dev server without the Q2 patch).
// Any other non-2xx propagates as an `Error` so the caller's toast
// surfaces the real failure reason.
//
// `?with_graph=true` asks the server to include graph-derived signals
// (backlinks / related / outgoing). The fallback path cannot compute
// the page graph client-side, so `with_graph` is silently dropped in
// the fallback branch.

import type { InboxCandidatesResponse } from "@/api/wiki/types";

export async function fetchInboxCandidates(
  id: number,
  options?: { with_graph?: boolean },
): Promise<InboxCandidatesResponse> {
  const qs = options?.with_graph ? "?with_graph=true" : "";
  const base = await getDesktopApiBase();
  const response = await fetch(
    `${base}/api/wiki/inbox/${id}/candidates${qs}`,
    { headers: { Accept: "application/json" } },
  );

  if (response.ok) {
    return (await response.json()) as InboxCandidatesResponse;
  }

  // Fallback — the Q2 endpoint isn't deployed on this server; run the
  // TS-port of the scorer against the two list endpoints so the UI
  // still gets candidates. Lazy import so the resolver + its deps
  // aren't paid for in the happy path.
  if (response.status === 404) {
    const { resolveInboxCandidatesClientSide } = await import(
      "@/domain/wiki/target-resolver"
    );
    return resolveInboxCandidatesClientSide(id);
  }

  let message = `fetchInboxCandidates failed with status ${response.status}`;
  try {
    const text = await response.text();
    if (text) message = text;
  } catch {
    // fall through with default message
  }
  throw new Error(message);
}

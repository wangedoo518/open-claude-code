import { fetchJson } from "@/lib/desktop/transport";
import {
  applyProposal as applySingleProposal,
  cancelProposal as cancelSingleProposal,
  createProposal as createSingleProposal,
  fetchInboxCandidates as fetchInboxCandidatesWithFallback,
  getRawEntry,
} from "@/api/wiki/repository";
import type {
  BatchResolveInboxResponse,
  CombinedApplyRequest,
  CombinedApplyResponse,
  CombinedProposalRequest,
  CombinedProposalResponse,
  InboxCandidatesResponse,
  InboxLineageResponse,
  MaintainRequest,
  MaintainResponse,
  PageGraph,
  RawDetailResponse,
  RawLineageResponse,
  UpdateProposal,
  WikiLineageResponse,
} from "@/api/wiki/types";

export type {
  BatchFailedItem,
  BatchResolveInboxResponse,
  CandidateReason,
  CandidateSource,
  CandidateTier,
  CombinedApplyOutcome,
  CombinedApplyRequest,
  CombinedApplyResponse,
  CombinedProposalRequest,
  CombinedProposalResponse,
  CombinedProposalSource,
  InboxCandidatesResponse,
  InboxEntry,
  InboxLineageResponse,
  IngestDecision,
  LineageEvent,
  LineageEventType,
  LineageRef,
  MaintainAction,
  MaintainOutcome,
  MaintainRequest,
  MaintainResponse,
  PageGraph,
  PageGraphNode,
  RawLineageResponse,
  RecentIngestEntry,
  RecentIngestOutcomeKind,
  RecentIngestResponse,
  RecentIngestStats,
  RelatedPageHit,
  TargetCandidate,
  UpdateProposal,
  WikiLineageResponse,
} from "@/api/wiki/types";

export type {
  MaintainRequest as InboxMaintainRequest,
  MaintainResponse as InboxMaintainResponse,
} from "@/api/wiki/types";

export async function fetchRawById(id: number): Promise<RawDetailResponse> {
  return getRawEntry(id);
}

export async function maintainInboxEntry(
  id: number,
  payload: MaintainRequest,
): Promise<MaintainResponse> {
  return fetchJson<MaintainResponse>(`/api/wiki/inbox/${id}/maintain`, {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function batchResolveInboxEntries(
  ids: number[],
  action: "reject" | "approve",
  reason?: string,
): Promise<BatchResolveInboxResponse> {
  return fetchJson<BatchResolveInboxResponse>("/api/wiki/inbox/batch/resolve", {
    method: "POST",
    body: JSON.stringify({ ids, action, reason }),
  });
}

export async function createProposal(
  inboxId: number,
  targetSlug: string,
): Promise<UpdateProposal> {
  return createSingleProposal(inboxId, targetSlug);
}

export async function applyProposal(
  inboxId: number,
): Promise<{ outcome: string; target_page_slug: string }> {
  return applySingleProposal(inboxId);
}

export async function cancelProposal(inboxId: number): Promise<void> {
  return cancelSingleProposal(inboxId);
}

export async function fetchCombinedProposal(
  request: CombinedProposalRequest,
): Promise<CombinedProposalResponse> {
  return fetchJson<CombinedProposalResponse>("/api/wiki/proposal/combined", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function applyCombinedProposal(
  request: CombinedApplyRequest,
): Promise<CombinedApplyResponse> {
  return fetchJson<CombinedApplyResponse>(
    "/api/wiki/proposal/combined/apply",
    {
      method: "POST",
      body: JSON.stringify(request),
    },
  );
}

export async function fetchInboxCandidates(
  id: number,
  options?: { with_graph?: boolean },
): Promise<InboxCandidatesResponse> {
  return fetchInboxCandidatesWithFallback(id, options);
}

export async function getWikiPageGraph(slug: string): Promise<PageGraph> {
  return fetchJson<PageGraph>(
    `/api/wiki/pages/${encodeURIComponent(slug)}/graph`,
  );
}

export async function fetchWikiLineage(
  slug: string,
  options?: { limit?: number; offset?: number },
): Promise<WikiLineageResponse> {
  const parts: string[] = [];
  if (options?.limit !== undefined) {
    parts.push(`limit=${options.limit}`);
  }
  if (options?.offset !== undefined) {
    parts.push(`offset=${options.offset}`);
  }
  const qs = parts.length > 0 ? `?${parts.join("&")}` : "";
  return fetchJson<WikiLineageResponse>(
    `/api/lineage/wiki/${encodeURIComponent(slug)}${qs}`,
  );
}

export async function fetchInboxLineage(
  id: number,
): Promise<InboxLineageResponse> {
  return fetchJson<InboxLineageResponse>(`/api/lineage/inbox/${id}`);
}

export async function fetchRawLineage(id: number): Promise<RawLineageResponse> {
  return fetchJson<RawLineageResponse>(`/api/lineage/raw/${id}`);
}

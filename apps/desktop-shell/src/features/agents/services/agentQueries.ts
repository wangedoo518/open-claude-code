/**
 * React Query hooks for agent detail fetching.
 *
 * Port from clawhub123/src/v2/features/agents/services/agentQueries.ts
 */

import { useQuery } from "@tanstack/react-query";
import type { AgentId, OpenclawAgentDetail } from "@/types/agent";
import {
  fetchOpenclawDetail,
  getDetailRefetchInterval,
} from "./openclawAgentController";

export const agentKeys = {
  all: ["agent"] as const,
  detail: (id: AgentId) => ["agent", "detail", id] as const,
};

/**
 * Fetches the composite OpenClaw agent detail and auto-polls based on state:
 * - 1500ms while a pipeline action is running
 * - 5000ms while the service is running
 * - false (no polling) otherwise
 */
export function useAgentDetailQuery(agentId: AgentId, enabled = true) {
  return useQuery<OpenclawAgentDetail>({
    queryKey: agentKeys.detail(agentId),
    queryFn: fetchOpenclawDetail,
    enabled,
    refetchInterval: (query) =>
      getDetailRefetchInterval(query.state.data ?? undefined),
  });
}

/**
 * React Query mutations for agent pipeline actions.
 *
 * Port from clawhub123/src/v2/features/agents/services/agentMutations.ts
 */

import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { AgentId, AgentPipelineStatus, OpenclawServiceControlResult } from "@/types/agent";
import { openclawActions } from "./openclawAgentController";
import { agentKeys } from "./agentQueries";

function useAgentActionMutation<T>(
  agentId: AgentId,
  actionFn: () => Promise<T>
) {
  const queryClient = useQueryClient();
  return useMutation<T>({
    mutationFn: actionFn,
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: agentKeys.detail(agentId),
      });
    },
  });
}

export function useInstallAgentMutation(agentId: AgentId) {
  return useAgentActionMutation<AgentPipelineStatus>(
    agentId,
    openclawActions.install
  );
}

export function useStartAgentMutation(agentId: AgentId) {
  return useAgentActionMutation<AgentPipelineStatus>(
    agentId,
    openclawActions.start
  );
}

export function useStopAgentMutation(agentId: AgentId) {
  return useAgentActionMutation<OpenclawServiceControlResult>(
    agentId,
    openclawActions.stop
  );
}

export function useUninstallAgentMutation(agentId: AgentId) {
  return useAgentActionMutation<AgentPipelineStatus>(
    agentId,
    openclawActions.uninstall
  );
}

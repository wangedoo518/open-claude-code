import { useQuery } from "@tanstack/react-query";
import { getKefuStatus } from "@/api/desktop/settings";
import type { KefuStatus } from "@/api/desktop/settings";
import { kefuQueryKeys } from "./kefu-query-keys";

export type ChannelStatus =
  | "not_connected"
  | "connecting"
  | "connected"
  | "disconnected"
  | "error";

function deriveChannelStatus(s: KefuStatus): ChannelStatus {
  if (!s.configured) return "not_connected";
  if (s.monitor_running && s.consecutive_failures === 0) return "connected";
  if (s.monitor_running && s.consecutive_failures > 0) return "error";
  if (s.configured && !s.monitor_running && s.account_created)
    return "disconnected";
  return "not_connected";
}

/**
 * Poll kefu status and derive a high-level ChannelStatus enum.
 *
 * @param refetchInterval - polling interval in ms (default 30_000)
 */
export function useKefuStatus(refetchInterval = 30_000) {
  const query = useQuery({
    queryKey: kefuQueryKeys.status(),
    queryFn: getKefuStatus,
    refetchInterval,
    staleTime: refetchInterval,
  });

  const raw = query.data ?? null;
  const channelStatus: ChannelStatus = raw
    ? deriveChannelStatus(raw)
    : "not_connected";

  return {
    ...query,
    raw,
    channelStatus,
  };
}

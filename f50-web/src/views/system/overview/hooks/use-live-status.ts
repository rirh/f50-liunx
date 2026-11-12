import { usePoll } from "@/hooks/use-live";
import { useSeries } from "@/lib/series";
import { useQuery } from "@tanstack/react-query";
import { getStatus, systemKeys } from "../api";
import type { Sample, SystemStatus } from "../types";

export const POLL_MS = 3000;

export interface LiveStatus {
  status: SystemStatus;
  /** The busy share since the previous reading (null on the first). */
  cpu: number | null;
  samples: Sample[];
}

/**
 * `f50.system status`, pushed over `f50.live` (every 2 s) or polled every 3 s when the stream is down, with the
 * history the live provider records from every reading (`lib/series`): the CPU share is its latest point.
 */
export function useLiveStatus() {
  const samples = useSeries("system");
  return useQuery({
    queryKey: systemKeys.status,
    queryFn: getStatus,
    refetchInterval: usePoll(POLL_MS, "system"),
    select: (status: SystemStatus): LiveStatus => ({
      status,
      cpu: samples.at(-1)?.cpu ?? null,
      samples,
    }),
  });
}

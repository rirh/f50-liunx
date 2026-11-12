import { useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { getStatus, systemKeys } from "../api";
import type { CpuTimes, Sample, SystemStatus } from "../types";
import { cpuPercent, memPercent } from "../utils";

export const POLL_MS = 3000;
const KEEP = 40;

export interface LiveStatus {
  status: SystemStatus;
  /** The busy share since the previous poll (null on the first). */
  cpu: number | null;
  samples: Sample[];
}

/**
 * `f50.system status` every 3 s, with the CPU share diffed from the previous reading and the last two minutes kept
 * for the charts. The history lives in a ref filled by the query function, so a render never mutates it.
 */
export function useLiveStatus() {
  const hist = useRef<{ prev: CpuTimes | null; samples: Sample[] }>({
    prev: null,
    samples: [],
  });
  return useQuery({
    queryKey: [...systemKeys.status, "live"],
    queryFn: async (): Promise<LiveStatus> => {
      const status = await getStatus();
      const cpu = cpuPercent(hist.current.prev, status.load);
      hist.current.prev = status.load;
      const sample: Sample = {
        t: Date.now(),
        cpu,
        mem: memPercent(status.memory),
        temp: status.temperatures.hottest,
      };
      hist.current.samples = [...hist.current.samples, sample].slice(-KEEP);
      return { status, cpu, samples: hist.current.samples };
    },
    refetchInterval: POLL_MS,
  });
}

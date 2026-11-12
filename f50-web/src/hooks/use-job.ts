import { useEffect, useRef } from "react";
import { type JobStatus, jobStatus } from "@/lib/jobs";
import { useQuery, useQueryClient } from "@tanstack/react-query";

/**
 * Follows a job until it ends: polled every second while running. `onDone` runs once when it leaves `running`
 * (refresh the page's data there).
 */
export function useJob(
  id: string | null | undefined,
  onDone?: (job: JobStatus) => void,
) {
  const qc = useQueryClient();
  const done = useRef<string | null>(null);
  const query = useQuery({
    queryKey: ["job", id],
    queryFn: () => jobStatus(id as string),
    enabled: !!id,
    refetchInterval: (q) =>
      q.state.data?.state === "running" || !q.state.data ? 1000 : false,
  });
  const job = query.data;
  useEffect(() => {
    if (!job || job.state === "running" || done.current === job.id) return;
    done.current = job.id;
    qc.invalidateQueries({ queryKey: ["jobs"] });
    onDone?.(job);
    // onDone is the caller's callback for this job; re-running on its identity would fire twice
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [job, qc]);
  return query;
}

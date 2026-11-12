import type { JobStarted } from "@/lib/jobs";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useSetAtom } from "jotai";
import { STATUS_KEY, UPDATE_KEY, getStatus } from "./api";
import { activeJobAtom } from "./store/job";

/** `f50.update status`: polled quickly while an update job runs, slowly otherwise (a background check may add a notice). */
export function useUpdateStatus() {
  return useQuery({
    queryKey: STATUS_KEY,
    queryFn: getStatus,
    refetchInterval: (q) => (q.state.data?.job ? 2000 : 15000),
  });
}

/** Shows a job the page just started, and refreshes what it changes. */
export function useStartJob() {
  const setJob = useSetAtom(activeJobAtom);
  const qc = useQueryClient();
  return (title: string) => (r: JobStarted) => {
    setJob({ id: r.job, title });
    qc.invalidateQueries({ queryKey: UPDATE_KEY });
  };
}

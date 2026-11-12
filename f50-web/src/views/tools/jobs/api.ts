import type { JobStatus } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";

export const jobKeys = {
  /** The same key `useJob` invalidates when a job ends. */
  list: ["jobs"] as const,
  status: (id: string) => ["job", id] as const,
};

/** Every job kept, oldest first. */
export const listJobs = () => f50<JobStatus[]>("f50.job", "list");
/** A job's state, result or error, and the tail of its log. */
export const getJob = (id: string) =>
  f50<JobStatus>("f50.job", "status", { id });
/** Stops a running job that may be stopped. */
export const cancelJob = (id: string) =>
  f50<Record<string, never>>("f50.job", "cancel", { id });

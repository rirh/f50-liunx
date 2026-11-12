import { JobProgress } from "@/components/f50/job-progress";
import type { JobStatus } from "@/lib/jobs";
import type { RunningJob } from "./use-job-slot";

/** The job a page started last, until the next one replaces it. */

export function JobSlot({
  job,
  onDone,
}: {
  job: RunningJob | null;
  onDone?: (job: JobStatus) => void;
}) {
  if (!job) return null;
  return (
    <JobProgress key={job.id} id={job.id} title={job.title} onDone={onDone} />
  );
}

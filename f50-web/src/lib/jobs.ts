/** f50d's jobs (`f50.job`): work that outlives a request; the method returns `{ job }`, the page polls it. */
import { t } from "@/i18n";
import { f50 } from "@/lib/ubus";

export type JobState =
  "running" | "done" | "failed" | "cancelled" | "interrupted";

/** The states' names, read in the current language each time one is looked up (`JOB_STATE_LABEL[state]`). */
export const JOB_STATE_LABEL: Readonly<Record<JobState, string>> = {
  get running() {
    return t("app.job.state.running");
  },
  get done() {
    return t("app.job.state.done");
  },
  get failed() {
    return t("app.job.state.failed");
  },
  get cancelled() {
    return t("app.job.state.cancelled");
  },
  get interrupted() {
    return t("app.job.state.interrupted");
  },
};

export interface JobStatus {
  id: string;
  method: string;
  kind: string;
  state: JobState;
  cancellable: boolean;
  started: number;
  finished?: number;
  result?: unknown;
  error?: { code: string; message: string };
  log: string;
}

export interface JobStarted {
  job: string;
}

export const jobStatus = (id: string) =>
  f50<JobStatus>("f50.job", "status", { id });
export const jobList = () => f50<JobStatus[]>("f50.job", "list");
export const jobCancel = (id: string) =>
  f50<Record<string, never>>("f50.job", "cancel", { id });

/** A reply that started a job. */
export function isJobStarted(v: unknown): v is JobStarted {
  return (
    typeof v === "object" &&
    v !== null &&
    typeof (v as JobStarted).job === "string"
  );
}

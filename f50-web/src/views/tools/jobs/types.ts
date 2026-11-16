import { type Key, t } from "@/i18n";

export type { JobState, JobStatus } from "@/lib/jobs";

/** What started a job, by its `kind`. */
const JOB_KIND_LABEL: Record<string, Key> = {
  at: "tools.jobs.kind.at",
  sms: "tools.jobs.kind.sms",
  wifi: "tools.jobs.kind.wifi",
  radio: "tools.jobs.kind.radio",
};

/** What started a job, by its method. */
const JOB_METHOD_LABEL: Record<string, Key> = {
  "f50.at send": "tools.jobs.method.atSend",
  "f50.sms send": "tools.jobs.method.smsSend",
  "f50.sms sync": "sms.actions.sync",
  "f50.wificlient join": "tools.jobs.method.wifiJoin",
  "f50.wificlient leave": "tools.jobs.method.wifiLeave",
  "f50.wificlient scan": "tools.jobs.method.wifiScan",
};

/** A job's `kind` in words, or as it came. */
export function jobKindLabel(kind: string): string {
  const key = Object.hasOwn(JOB_KIND_LABEL, kind) ? JOB_KIND_LABEL[kind] : null;
  return key ? t(key) : kind;
}

/** A job's method in words, or as it came. */
export function jobMethodLabel(method: string): string {
  const key = Object.hasOwn(JOB_METHOD_LABEL, method)
    ? JOB_METHOD_LABEL[method]
    : null;
  return key ? t(key) : method;
}

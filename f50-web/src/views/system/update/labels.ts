import { type Key, t } from "@/i18n";
import type { SigState } from "./types";

/** A job's title key by its method (`f50.job list` gives the method of a job found running). */
const JOB_TITLE: Record<string, Key> = {
  "f50.update check": "maintenance.update.jobTitle.check",
  "f50.update apply": "maintenance.update.jobTitle.apply",
  "f50.update rollback": "maintenance.update.jobTitle.rollback",
  "f50.update upload_done": "maintenance.update.jobTitle.uploadDone",
  "f50.update apply_file": "maintenance.update.jobTitle.applyFile",
};

/** A job's title by its method, or undefined for a method this page does not know. */
export const jobTitle = (method: string): string | undefined => {
  const key = JOB_TITLE[method];
  return key && t(key);
};

const SIG_LABEL = {
  valid: "maintenance.update.sig.valid",
  bad: "maintenance.update.sig.bad",
  unsigned: "maintenance.update.sig.unsigned",
  nokey: "maintenance.update.sig.nokey",
  badkey: "maintenance.update.sig.badkey",
  notool: "maintenance.update.sig.notool",
} satisfies Record<SigState, Key>;

/** A signature check's outcome in words (the raw value for one this page does not know). */
export const sigLabel = (s: SigState): string =>
  (SIG_LABEL as Record<string, Key>)[s] ? t(SIG_LABEL[s]) : s;

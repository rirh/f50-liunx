import { t } from "@/i18n";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { lockKeys, setLock } from "./api";
import type { LockKind } from "./types";

export type OnJob = (id: string, title: string) => void;

export interface LockChange {
  kind: LockKind;
  value: string;
  /** The job's title on the page. */
  title: string;
}

/** One `f50.lock set`: a job (followed by the page) or, for `auto_apply`, a switch saved at once. */
export function useLockSet(onJob: OnJob) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: LockChange) => setLock(c.kind, c.value),
    onSuccess: (r, c) => {
      if ("job" in r) {
        onJob(r.job, c.title);
        if (r.queued) toast.info(t("cellular.locks.queued"));
      } else {
        toast.success(
          r.auto_apply
            ? t("cellular.locks.autoApplyOn")
            : t("cellular.locks.autoApplyOff"),
        );
      }
      qc.invalidateQueries({ queryKey: lockKeys.get });
    },
  });
}

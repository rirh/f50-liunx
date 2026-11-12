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
        if (r.queued) toast.info("已排队：将在当前射频操作结束后执行");
      } else {
        toast.success(
          r.auto_apply ? "已开启启动时自动应用" : "已关闭启动时自动应用",
        );
      }
      qc.invalidateQueries({ queryKey: lockKeys.get });
    },
  });
}

import { ConfirmButton } from "@/components/f50/confirm-button";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { cancelJob, jobKeys } from "../api";
import type { JobStatus } from "../types";
import { JOB_METHOD_LABEL } from "../types";

/** Stops a running job after asking. */
export function CancelJobButton({
  job,
  size = "sm",
}: {
  job: JobStatus;
  size?: "sm" | "xs";
}) {
  const qc = useQueryClient();
  const cancel = useMutation({
    mutationFn: () => cancelJob(job.id),
    onSuccess: () => toast.success("已请求停止任务"),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: jobKeys.list }),
        qc.invalidateQueries({ queryKey: jobKeys.status(job.id) }),
      ]),
  });
  return (
    <ConfirmButton
      size={size}
      variant="outline"
      title="停止任务"
      description={`停止“${JOB_METHOD_LABEL[job.method] ?? job.method}”（${job.id}）？已完成的部分不会撤销。`}
      confirmText="停止"
      pending={cancel.isPending}
      onConfirm={() => cancel.mutate()}
    >
      停止
    </ConfirmButton>
  );
}

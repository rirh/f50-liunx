import { ConfirmButton } from "@/components/f50/confirm-button";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { cancelJob, jobKeys } from "../api";
import type { JobStatus } from "../types";
import { jobMethodLabel } from "../types";

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
    onSuccess: () => toast.success(t("tools.jobs.cancel.requested")),
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
      title={t("tools.jobs.cancel.title")}
      description={t("tools.jobs.cancel.description", {
        name: jobMethodLabel(job.method),
        id: job.id,
      })}
      confirmText={t("common.action.stop")}
      pending={cancel.isPending}
      onConfirm={() => cancel.mutate()}
    >
      {t("common.action.stop")}
    </ConfirmButton>
  );
}

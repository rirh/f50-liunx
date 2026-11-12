import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
import { useJob } from "@/hooks/use-job";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { JOB_STATE_LABEL, type JobStatus, jobCancel } from "@/lib/jobs";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";

/** A job's live state: progress while it runs, the outcome and its log afterwards. */
export function JobProgress({
  id,
  title,
  onDone,
}: {
  id: string;
  title: string;
  onDone?: (job: JobStatus) => void;
}) {
  const { data: job, error } = useJob(id, onDone);
  const cancel = useMutation({
    mutationFn: () => jobCancel(id),
    onSuccess: () => toast.success(t("app.job.stopRequested")),
    onError: (e) => toast.error(errorMessage(e)),
  });

  if (error) {
    return (
      <Alert variant="destructive">
        <AlertTitle>{title}</AlertTitle>
        <AlertDescription>{errorMessage(error)}</AlertDescription>
      </Alert>
    );
  }
  const running = !job || job.state === "running";
  return (
    <Alert variant={job?.state === "failed" ? "destructive" : "default"}>
      <AlertTitle className="flex items-center gap-2">
        {running ? <SweepShine>{title}</SweepShine> : title}
        {job && <Badge variant="secondary">{JOB_STATE_LABEL[job.state]}</Badge>}
      </AlertTitle>
      <AlertDescription className="flex flex-col gap-2">
        {running && <Spinner />}
        {job?.error && <span>{job.error.message}</span>}
        {job?.log && (
          <pre className="max-h-48 overflow-auto rounded-md bg-muted p-2 text-xs whitespace-pre-wrap">
            {job.log}
          </pre>
        )}
        {running && job?.cancellable && (
          <Button
            size="sm"
            variant="outline"
            className="self-start"
            disabled={cancel.isPending}
            aria-busy={cancel.isPending}
            onClick={() => cancel.mutate()}
          >
            {cancel.isPending ? (
              <SweepShine>{t("common.action.stopping")}</SweepShine>
            ) : (
              t("common.action.stop")
            )}
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}

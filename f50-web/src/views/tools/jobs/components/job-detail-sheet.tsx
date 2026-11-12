import { KvList } from "@/components/f50/kv-list";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Separator } from "@/components/ui/separator";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { formatAgo, formatDuration, formatTime } from "@/lib/format";
import { useQuery } from "@tanstack/react-query";
import { CancelJobButton } from "./cancel-job-button";
import { JobStateBadge } from "./job-state-badge";
import { getJob, jobKeys } from "../api";
import { jobKindLabel, jobMethodLabel, type JobStatus } from "../types";

/** One job's status, result or error and log tail; polled every second while it runs. */
export function JobDetailSheet({
  id,
  onClose,
}: {
  id: string | null;
  onClose: () => void;
}) {
  return (
    <Sheet open={id !== null} onOpenChange={(o) => !o && onClose()}>
      <SheetContent className="w-full gap-0 sm:max-w-xl">
        <SheetHeader>
          <SheetTitle>{t("tools.jobs.detail.title")}</SheetTitle>
          <SheetDescription className="font-mono break-all">
            {id}
          </SheetDescription>
        </SheetHeader>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
          {id && <JobDetail id={id} />}
        </div>
      </SheetContent>
    </Sheet>
  );
}

function JobDetail({ id }: { id: string }) {
  const query = useQuery({
    queryKey: jobKeys.status(id),
    queryFn: () => getJob(id),
    refetchInterval: (q) => (q.state.data?.state === "running" ? 1000 : false),
  });
  return (
    <QueryState query={query} skeletonRows={6}>
      {(job) => <JobBody job={job} refreshing={query.isFetching} />}
    </QueryState>
  );
}

function JobBody({ job, refreshing }: { job: JobStatus; refreshing: boolean }) {
  const running = job.state === "running";
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <JobStateBadge state={job.state} />
        {running && refreshing && (
          <SweepShine className="text-xs text-muted-foreground">
            {t("common.action.refreshing")}
          </SweepShine>
        )}
        {running && job.cancellable && (
          <div className="ml-auto">
            <CancelJobButton job={job} />
          </div>
        )}
      </div>
      <KvList
        items={[
          {
            label: t("tools.jobs.field.action"),
            value: jobMethodLabel(job.method),
          },
          {
            label: t("tools.jobs.field.method"),
            value: <code className="font-mono text-xs">{job.method}</code>,
          },
          { label: t("tools.jobs.field.kind"), value: jobKindLabel(job.kind) },
          {
            label: t("tools.jobs.field.cancellable"),
            value: job.cancellable
              ? t("common.state.yes")
              : t("common.state.no"),
          },
          {
            label: t("tools.jobs.field.started"),
            value: formatAgo(job.started),
            title: formatTime(job.started),
          },
          {
            label: t("tools.jobs.field.finished"),
            value: job.finished
              ? formatAgo(job.finished)
              : running
                ? t("tools.jobs.detail.inProgress")
                : "—",
            title: job.finished ? formatTime(job.finished) : undefined,
          },
          {
            label: t("tools.jobs.field.duration"),
            value: job.finished
              ? formatDuration(job.finished - job.started)
              : running
                ? t("tools.jobs.detail.inProgress")
                : "—",
          },
        ]}
      />
      {job.error && (
        <Alert variant="destructive">
          <AlertTitle>
            {t("tools.jobs.detail.error", { code: job.error.code })}
          </AlertTitle>
          <AlertDescription>{job.error.message}</AlertDescription>
        </Alert>
      )}
      {job.result !== undefined && (
        <>
          <Separator />
          <section className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">
              {t("tools.jobs.detail.result")}
            </h3>
            <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 font-mono text-xs whitespace-pre-wrap break-all">
              {JSON.stringify(job.result, null, 2)}
            </pre>
          </section>
        </>
      )}
      <Separator />
      <section className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">{t("tools.jobs.detail.log")}</h3>
        {job.log ? (
          <pre className="max-h-96 overflow-auto rounded-md bg-muted p-3 font-mono text-xs whitespace-pre-wrap break-all">
            {job.log}
          </pre>
        ) : (
          <p className="text-sm text-muted-foreground">
            {t("tools.jobs.detail.noLog")}
          </p>
        )}
      </section>
    </div>
  );
}

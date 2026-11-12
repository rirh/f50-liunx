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
import { formatAgo, formatDuration, formatTime } from "@/lib/format";
import { useQuery } from "@tanstack/react-query";
import { CancelJobButton } from "./cancel-job-button";
import { JobStateBadge } from "./job-state-badge";
import { getJob, jobKeys } from "../api";
import { JOB_KIND_LABEL, JOB_METHOD_LABEL, type JobStatus } from "../types";

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
          <SheetTitle>任务详情</SheetTitle>
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
            刷新中
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
          { label: "操作", value: JOB_METHOD_LABEL[job.method] ?? job.method },
          {
            label: "方法",
            value: <code className="font-mono text-xs">{job.method}</code>,
          },
          { label: "类别", value: JOB_KIND_LABEL[job.kind] ?? job.kind },
          { label: "可停止", value: job.cancellable ? "是" : "否" },
          {
            label: "开始",
            value: formatAgo(job.started),
            title: formatTime(job.started),
          },
          {
            label: "结束",
            value: job.finished
              ? formatAgo(job.finished)
              : running
                ? "进行中"
                : "—",
            title: job.finished ? formatTime(job.finished) : undefined,
          },
          {
            label: "用时",
            value: job.finished
              ? formatDuration(job.finished - job.started)
              : running
                ? "进行中"
                : "—",
          },
        ]}
      />
      {job.error && (
        <Alert variant="destructive">
          <AlertTitle>错误（{job.error.code}）</AlertTitle>
          <AlertDescription>{job.error.message}</AlertDescription>
        </Alert>
      )}
      {job.result !== undefined && (
        <>
          <Separator />
          <section className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">结果</h3>
            <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 font-mono text-xs whitespace-pre-wrap break-all">
              {JSON.stringify(job.result, null, 2)}
            </pre>
          </section>
        </>
      )}
      <Separator />
      <section className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">日志（末尾）</h3>
        {job.log ? (
          <pre className="max-h-96 overflow-auto rounded-md bg-muted p-3 font-mono text-xs whitespace-pre-wrap break-all">
            {job.log}
          </pre>
        ) : (
          <p className="text-sm text-muted-foreground">没有日志</p>
        )}
      </section>
    </div>
  );
}

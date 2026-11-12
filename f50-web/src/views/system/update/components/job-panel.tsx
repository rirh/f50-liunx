import { useEffect } from "react";
import { JobProgress } from "@/components/f50/job-progress";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useJob } from "@/hooks/use-job";
import { type JobStatus, jobList } from "@/lib/jobs";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useAtom } from "jotai";
import { TriangleAlertIcon, XIcon } from "lucide-react";
import { toast } from "sonner";
import { UPDATE_KEY } from "../api";
import { JOB_TITLE } from "../labels";
import { activeJobAtom } from "../store/job";
import type { UpdateStatus } from "../types";

/** Jobs that end by writing the systems or the boot image: they cannot be stopped. */
const UNSTOPPABLE = [
  "f50.update apply",
  "f50.update apply_file",
  "f50.update rollback",
];

/** The update job the page follows: the one started here, or the one found running (status, or `f50.job list`). */
export function JobPanel({ status }: { status: UpdateStatus }) {
  const qc = useQueryClient();
  const [active, setActive] = useAtom(activeJobAtom);
  const jobs = useQuery({ queryKey: ["jobs"], queryFn: jobList });
  const running =
    status.job ??
    jobs.data?.find((j) => j.kind === "update" && j.state === "running")?.id ??
    null;
  const runningMethod = jobs.data?.find((j) => j.id === running)?.method;

  useEffect(() => {
    if (!running) return;
    const title = (runningMethod && JOB_TITLE[runningMethod]) || "更新任务";
    if (active?.id !== running || active.title !== title)
      setActive({ id: running, title });
  }, [running, runningMethod, active, setActive]);

  const { data: job } = useJob(active?.id);
  if (!active) return null;
  const isRunning = !job || job.state === "running";
  const unstoppable = job && UNSTOPPABLE.includes(job.method);

  const onDone = (j: JobStatus) => {
    qc.invalidateQueries({ queryKey: UPDATE_KEY });
    if (j.state === "done") toast.success(`${active.title}已完成`);
    else
      toast.error(`${active.title}${j.state === "failed" ? "失败" : "已结束"}`);
  };

  return (
    <div className="flex flex-col gap-2">
      {isRunning && unstoppable && (
        <Alert variant="destructive">
          <TriangleAlertIcon />
          <AlertTitle>正在写入系统，请勿断电、重启或拔出设备</AlertTitle>
          <AlertDescription>
            此任务无法中途停止。中断可能导致设备无法启动。完成前请保持电源连接并留在此页面。
          </AlertDescription>
        </Alert>
      )}
      <JobProgress id={active.id} title={active.title} onDone={onDone} />
      {!isRunning && (
        <Button
          variant="ghost"
          size="sm"
          className="self-end"
          onClick={() => setActive(null)}
        >
          <XIcon data-icon="inline-start" />
          收起
        </Button>
      )}
    </div>
  );
}

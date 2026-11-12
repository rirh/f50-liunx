import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { SweepShine } from "@/components/ui/sweep-shine";
import { useQuery } from "@tanstack/react-query";
import { ListChecksIcon, RefreshCwIcon } from "lucide-react";
import { jobKeys, listJobs } from "./api";
import { JobDetailSheet } from "./components/job-detail-sheet";
import { JobsTable } from "./components/jobs-table";

/** f50d's background jobs (`f50.job`): polled every 3 s while any runs. */
export default function JobsPage() {
  const query = useQuery({
    queryKey: jobKeys.list,
    queryFn: listJobs,
    refetchInterval: (q) =>
      q.state.data?.some((j) => j.state === "running") ? 3000 : false,
  });
  const [open, setOpen] = useState<string | null>(null);
  const running = query.data?.filter((j) => j.state === "running").length ?? 0;
  const refreshing = query.isFetching && !query.isPending;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="后台任务"
        description="发送短信、长时间 AT 命令、更新等耗时操作在后台运行；设备重启前保留。"
        actions={
          <Button
            variant="outline"
            size="sm"
            disabled={query.isFetching}
            aria-busy={query.isFetching}
            onClick={() => void query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {refreshing ? <SweepShine>刷新中</SweepShine> : "刷新"}
          </Button>
        }
      />
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            任务列表
            {running > 0 && <Badge>{running} 个进行中</Badge>}
          </CardTitle>
          <CardDescription>
            {running > 0
              ? "有任务在运行，每 3 秒自动刷新。"
              : "点击一行查看状态、结果和日志。"}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <QueryState query={query} skeletonRows={5}>
            {(jobs) =>
              jobs.length === 0 ? (
                <Empty>
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <ListChecksIcon />
                    </EmptyMedia>
                    <EmptyTitle>没有任务</EmptyTitle>
                    <EmptyDescription>
                      本次开机以来还没有后台任务。
                    </EmptyDescription>
                  </EmptyHeader>
                </Empty>
              ) : (
                <JobsTable data={jobs} onOpen={setOpen} />
              )
            }
          </QueryState>
        </CardContent>
      </Card>
      <JobDetailSheet id={open} onClose={() => setOpen(null)} />
    </div>
  );
}

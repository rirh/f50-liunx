import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
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
        title={t("tools.jobs.page.title")}
        description={t("tools.jobs.page.description")}
        actions={
          <>
            {running > 0 && (
              <Badge>{t("tools.jobs.list.running", { count: running })}</Badge>
            )}
            <Button
              variant="outline"
              size="sm"
              disabled={query.isFetching}
              aria-busy={query.isFetching}
              onClick={() => void query.refetch()}
            >
              <RefreshCwIcon data-icon="inline-start" />
              {refreshing ? (
                <SweepShine>{t("common.action.refreshing")}</SweepShine>
              ) : (
                t("common.action.refresh")
              )}
            </Button>
          </>
        }
      />
      <Card className="py-1">
        <QueryState query={query} skeletonRows={5}>
          {(jobs) =>
            jobs.length === 0 ? (
              <Empty>
                <EmptyHeader>
                  <EmptyMedia variant="icon">
                    <ListChecksIcon />
                  </EmptyMedia>
                  <EmptyTitle>{t("tools.jobs.list.emptyTitle")}</EmptyTitle>
                  <EmptyDescription>
                    {t("tools.jobs.list.emptyDescription")}
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : (
              <div className="px-2">
                <JobsTable data={jobs} onOpen={setOpen} />
              </div>
            )
          }
        </QueryState>
      </Card>
      <JobDetailSheet id={open} onClose={() => setOpen(null)} />
    </div>
  );
}

import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { SweepShine } from "@/components/ui/sweep-shine";
import {
  errorMessage,
  formatAgo,
  formatDuration,
  formatTime,
} from "@/lib/format";
import { useMutation, useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { toast } from "sonner";
import { OfferItem } from "./offer-item";
import { RELEASES_KEY, check, getReleases } from "../api";
import { useStartJob } from "../hooks";
import type { UpdateStatus } from "../types";

/** Online update: ask for the newest release, see what the last check found, install it. */
export function ReleasesCard({
  status,
  busy,
}: {
  status: UpdateStatus;
  busy: boolean;
}) {
  const releases = useQuery({ queryKey: RELEASES_KEY, queryFn: getReleases });
  const started = useStartJob();
  const checking = useMutation({
    mutationFn: check,
    onSuccess: started("检查更新"),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const age = releases.data?.age;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>在线更新</CardTitle>
        <CardDescription>
          {age != null
            ? `上次检查：${formatDuration(age)}前`
            : "尚未检查过更新"}
        </CardDescription>
        <CardAction>
          <Button
            size="sm"
            variant="outline"
            disabled={busy || checking.isPending}
            aria-busy={checking.isPending}
            onClick={() => checking.mutate()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {checking.isPending ? <SweepShine>检查中</SweepShine> : "检查更新"}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {status.notice && (
          <Alert>
            <AlertTitle>有新版本 {status.notice.tag}</AlertTitle>
            <AlertDescription title={formatTime(status.notice.at)}>
              当前 {status.notice.installed ?? "未知版本"}，
              {formatAgo(status.notice.at)}发现。
            </AlertDescription>
          </Alert>
        )}
        <QueryState query={releases}>
          {(r) =>
            r.check ? (
              <>
                <OfferItem offer={r.check.latest} status={status} busy={busy} />
                {r.check.beta && (
                  <OfferItem offer={r.check.beta} status={status} busy={busy} />
                )}
              </>
            ) : (
              <Empty>
                <EmptyHeader>
                  <EmptyTitle>没有版本信息</EmptyTitle>
                  <EmptyDescription>
                    点击“检查更新”向发布源查询最新版本。
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            )
          }
        </QueryState>
      </CardContent>
    </Card>
  );
}

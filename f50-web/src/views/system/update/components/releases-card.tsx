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
import { t } from "@/i18n";
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
    onSuccess: started(t("maintenance.update.jobTitle.check")),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const age = releases.data?.age;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{t("maintenance.update.releases.title")}</CardTitle>
        <CardDescription>
          {age != null
            ? t("maintenance.update.releases.lastCheck", {
                ago: formatDuration(age),
              })
            : t("maintenance.update.releases.neverChecked")}
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
            {checking.isPending ? (
              <SweepShine>
                {t("maintenance.update.releases.checking")}
              </SweepShine>
            ) : (
              t("maintenance.update.jobTitle.check")
            )}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {status.notice && (
          <Alert>
            <AlertTitle>
              {t("maintenance.update.releases.noticeTitle", {
                tag: status.notice.tag,
              })}
            </AlertTitle>
            <AlertDescription title={formatTime(status.notice.at)}>
              {t("maintenance.update.releases.noticeBody", {
                installed:
                  status.notice.installed ??
                  t("maintenance.update.status.unknownRelease"),
                ago: formatAgo(status.notice.at),
              })}
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
                  <EmptyTitle>
                    {t("maintenance.update.releases.emptyTitle")}
                  </EmptyTitle>
                  <EmptyDescription>
                    {t("maintenance.update.releases.emptyBody")}
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

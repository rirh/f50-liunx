import { Link } from "react-router-dom";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useLiveState } from "@/hooks/use-live";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useQuery } from "@tanstack/react-query";
import { ArrowUpCircleIcon, FlameIcon, RefreshCwIcon } from "lucide-react";
import { getInfo, systemKeys } from "./api";
import { AboutCard } from "./components/about-card";
import { ClientsCard } from "./components/clients-card";
import { StatTiles } from "./components/stat-tiles";
import { TrendCard } from "./components/trend-card";
import { useLiveStatus } from "./hooks/use-live-status";

/**
 * The device at a glance: any alert, the four headline figures, the grouped facts in four columns, the load history, then
 * the clients. Pushed figures need no refresh button; it shows only while the page polls.
 */
export default function Page() {
  const info = useQuery({ queryKey: systemKeys.info, queryFn: getInfo });
  const status = useLiveStatus();
  const live = useLiveState() === "live";
  const s = status.data?.status;
  const failed = info.error ?? status.error;
  return (
    <>
      <PageHeader
        title={t("system.overview.title")}
        description={
          live
            ? t("system.overview.live.descLive")
            : t("system.overview.live.descPoll")
        }
        actions={
          !live && (
            <PendingButton
              size="sm"
              variant="outline"
              pending={info.isFetching || status.isFetching}
              onClick={() => {
                void info.refetch();
                void status.refetch();
              }}
            >
              <RefreshCwIcon data-icon="inline-start" />
              {t("system.overview.refresh")}
            </PendingButton>
          )
        }
      />
      {failed && (
        <Alert variant="destructive">
          <AlertDescription>{errorMessage(failed)}</AlertDescription>
        </Alert>
      )}
      {s?.heat_alarm && (
        <Alert variant="destructive">
          <FlameIcon />
          <AlertTitle>{t("system.overview.live.heatTitle")}</AlertTitle>
          <AlertDescription>
            {t("system.overview.live.heatDescription")}
          </AlertDescription>
        </Alert>
      )}
      {s?.update && (
        <Alert>
          <ArrowUpCircleIcon />
          <AlertTitle>
            {t("system.overview.live.updateTitle", { version: s.update })}
          </AlertTitle>
          <AlertDescription className="flex flex-wrap items-center gap-2">
            {t("system.overview.live.updateDescription")}
            <Button asChild size="xs" variant="outline">
              <Link to="/system/update">
                {t("system.overview.live.updateAction")}
              </Link>
            </Button>
          </AlertDescription>
        </Alert>
      )}
      <StatTiles live={status.data} info={info.data} />
      <AboutCard info={info.data} live={status.data} />
      <TrendCard live={status.data} />
      <ClientsCard />
    </>
  );
}

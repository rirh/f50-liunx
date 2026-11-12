import { useRef } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";
import { useLiveState, usePoll } from "@/hooks/use-live";
import { useStaggerIn } from "@/hooks/use-motion";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { STATUS_KEY, getStatus } from "./api";
import { DataTile } from "./components/data-tile";
import { HeroCard } from "./components/hero-card";
import { SmsTile } from "./components/sms-tile";
import { TileSkeleton } from "./components/tile";
import { UpdateTile } from "./components/update-tile";
import { UplinkTile } from "./components/uplink-tile";
import { UsbTile } from "./components/usb-tile";
import { WifiTile } from "./components/wifi-tile";

const POLL_MS = 3000;

/**
 * The device on one screen: a strip with the four figures people look at first (network, speed, usage, system),
 * then six compact tiles. Each title links to its page.
 */
export default function DashboardPage() {
  const query = useQuery({
    queryKey: STATUS_KEY,
    queryFn: getStatus,
    refetchInterval: usePoll(POLL_MS, "status"),
  });
  const live = useLiveState() === "live";
  const s = query.data;
  const refreshing = query.isFetching && !query.isPending;
  const page = useRef<HTMLDivElement>(null);
  useStaggerIn(page, !!s, "[data-reveal]");

  return (
    <div ref={page} className="flex flex-col gap-4">
      <PageHeader
        title={t("dashboard.title")}
        description={live ? t("dashboard.descLive") : t("dashboard.descPoll")}
        actions={
          <Button
            variant="outline"
            size="sm"
            disabled={query.isFetching}
            aria-busy={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {refreshing ? (
              <SweepShine>{t("common.action.refreshing")}</SweepShine>
            ) : (
              t("common.action.refresh")
            )}
          </Button>
        }
      />
      {query.isError && (
        <Alert variant="destructive">
          <AlertTitle>
            {s ? t("dashboard.staleTitle") : t("dashboard.failedTitle")}
          </AlertTitle>
          <AlertDescription>{errorMessage(query.error)}</AlertDescription>
        </Alert>
      )}
      {s ? (
        <>
          <div data-reveal>
            <HeroCard s={s} />
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div data-reveal>
              <DataTile part={s.data} />
            </div>
            <div data-reveal>
              <WifiTile part={s.wifi} />
            </div>
            <div data-reveal>
              <UplinkTile part={s.wificlient} />
            </div>
            <div data-reveal>
              <UsbTile part={s.usb} />
            </div>
            <div data-reveal>
              <SmsTile part={s.sms} />
            </div>
            <div data-reveal>
              <UpdateTile part={s.update} />
            </div>
          </div>
        </>
      ) : (
        query.isPending && (
          <>
            <TileSkeleton className="h-40" />
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {Array.from({ length: 6 }, (_, i) => (
                <TileSkeleton key={i} />
              ))}
            </div>
          </>
        )
      )}
    </div>
  );
}

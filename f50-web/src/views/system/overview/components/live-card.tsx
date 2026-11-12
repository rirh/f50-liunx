import { Link } from "react-router-dom";
import { AnimatedNumber } from "@/components/f50/animated-number";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useLiveState } from "@/hooks/use-live";
import { t } from "@/i18n";
import { formatBytes, formatDuration } from "@/lib/format";
import { ArrowUpCircleIcon, FlameIcon } from "lucide-react";
import { LiveChart } from "./live-chart";
import { Meter } from "./meter";
import { useLiveStatus } from "../hooks/use-live-status";
import type { LiveStatus } from "../hooks/use-live-status";
import {
  formatCelsius,
  formatMhz,
  formatPercent,
  kb,
  memPercent,
  zoneLabel,
} from "../utils";

function usbRole(s: LiveStatus["status"]["usb_link"]) {
  if (!s) return t("common.state.disconnected");
  return s.role === "device"
    ? t("system.overview.live.usbDevice")
    : t("system.overview.live.usbHost");
}

function usbSpeed(s: LiveStatus["status"]["usb_link"]) {
  if (!s) return undefined;
  const now =
    s.mbps != null
      ? `${s.mbps} Mbps`
      : (s.speed ?? t("system.overview.live.speedUnknown"));
  return s.max ? t("system.overview.live.speedMax", { now, max: s.max }) : now;
}

function LiveBody({ data }: { data: LiveStatus }) {
  const { status: s, cpu, samples } = data;
  const mem = memPercent(s.memory);
  const used =
    s.memory.total_kb != null && s.memory.avail_kb != null
      ? s.memory.total_kb - s.memory.avail_kb
      : null;
  const temps = s.temperatures;
  return (
    <div className="flex flex-col gap-4">
      {s.heat_alarm && (
        <Alert variant="destructive">
          <FlameIcon />
          <AlertTitle>{t("system.overview.live.heatTitle")}</AlertTitle>
          <AlertDescription>
            {t("system.overview.live.heatDescription")}
          </AlertDescription>
        </Alert>
      )}
      {s.update && (
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
      <div className="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-3">
        <Meter
          label="CPU"
          value={
            <AnimatedNumber value={cpu} format={(n) => formatPercent(n, 1)} />
          }
          percent={cpu}
          hint={
            s.load
              ? t("system.overview.live.cpuHint", {
                  n: s.load.n,
                  freqs: s.load.freqs.map((f) => formatMhz(f.cur)).join(" / "),
                })
              : undefined
          }
        />
        <Meter
          label={t("system.overview.live.mem")}
          value={
            <AnimatedNumber value={mem} format={(n) => formatPercent(n, 1)} />
          }
          percent={mem}
          hint={`${formatBytes(kb(used))} / ${formatBytes(kb(s.memory.total_kb))}`}
        />
        <Meter
          label={t("system.overview.live.hottest")}
          value={
            <AnimatedNumber value={temps.hottest} format={formatCelsius} />
          }
          hint={`SoC ${formatCelsius(temps.soc)}`}
        />
        <Meter
          label={t("system.overview.live.modem")}
          value={<AnimatedNumber value={temps.modem} format={formatCelsius} />}
          hint={t("system.overview.live.boardHint", {
            temp: formatCelsius(temps.board),
          })}
        />
        <Meter
          label={t("system.overview.live.wifi")}
          value={s.wifi_clients}
          hint={t("system.overview.live.leasesHint", { n: s.lan_leases })}
        />
        <Meter
          label={t("system.overview.live.conns")}
          value={s.conns ?? "—"}
          hint={t("system.overview.live.connsHint")}
        />
        <Meter
          label={t("system.overview.live.usb")}
          value={usbRole(s.usb_link)}
          hint={usbSpeed(s.usb_link)}
        />
        <Meter
          label={t("system.overview.live.uptime")}
          value={formatDuration(s.uptime)}
        />
      </div>
      <LiveChart samples={samples} />
      {temps.zones.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {temps.zones.map((z) => (
            <Badge key={z.name} variant="outline" className="font-mono">
              {zoneLabel(z.name)} {formatCelsius(z.millicelsius / 1000)}
            </Badge>
          ))}
        </div>
      )}
    </div>
  );
}

/** The live figures, polled every 3 s. */
export function LiveCard() {
  const query = useLiveStatus();
  const live = useLiveState() === "live";
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.overview.live.title")}</CardTitle>
        <CardDescription>
          {live
            ? t("system.overview.live.descLive")
            : t("system.overview.live.descPoll")}
        </CardDescription>
        <CardAction>
          {query.isFetching ? (
            <Badge variant="secondary">{t("common.action.refreshing")}</Badge>
          ) : (
            <Badge variant="outline">{t("system.overview.live.badge")}</Badge>
          )}
        </CardAction>
      </CardHeader>
      <CardContent>
        <QueryState query={query} skeletonRows={6}>
          {(data) => <LiveBody data={data} />}
        </QueryState>
      </CardContent>
    </Card>
  );
}

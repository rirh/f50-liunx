import { useState } from "react";
import { DataTable } from "@/components/f50/data-table";
import { FillMeter } from "@/components/f50/fill-meter";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { type Key, t } from "@/i18n";
import { type TableColumn, columnHelper } from "@/lib/table";
import { cn } from "@/lib/utils";
import { ChevronDownIcon, ThermometerIcon } from "lucide-react";
import type { ThermalZone } from "../types";

const celsius = (mc: number | null) =>
  mc == null ? "—" : `${(mc / 1000).toFixed(1)} °C`;

const TRIP_TEXT: Record<string, Key> = {
  passive: "system.cpu.thermal.trip.passive",
  active: "system.cpu.thermal.trip.active",
  hot: "system.cpu.thermal.trip.hot",
  critical: "system.cpu.thermal.trip.critical",
};

const c = columnHelper<ThermalZone>();

const columns: TableColumn<ThermalZone>[] = [
  c.accessor("zone", {
    header: () => t("system.cpu.thermal.col.zone"),
    cell: (x) => <span className="tabular-nums">{x.getValue()}</span>,
  }),
  c.accessor("temp", {
    header: () => t("system.cpu.thermal.col.temp"),
    sortUndefined: "last",
    cell: (x) => <span className="tabular-nums">{celsius(x.getValue())}</span>,
  }),
  c.accessor((r) => r.mode ?? "", {
    id: "mode",
    header: () => t("system.cpu.thermal.col.mode"),
    cell: (x) =>
      x.getValue() === "enabled" ? (
        <Badge variant="secondary">{t("system.cpu.thermal.enabled")}</Badge>
      ) : x.getValue() ? (
        <Badge variant="outline">{t("system.cpu.thermal.disabled")}</Badge>
      ) : (
        "—"
      ),
  }),
  c.display({
    id: "trips",
    header: () => t("system.cpu.thermal.col.trips"),
    cell: (x) => {
      const trips = x.row.original.trips;
      if (!trips.length)
        return (
          <span className="text-muted-foreground">
            {t("common.state.none")}
          </span>
        );
      const temp = x.row.original.temp;
      return (
        <div className="flex flex-wrap gap-1">
          {trips.map((trip, i) => (
            <Badge
              key={i}
              variant={
                temp != null &&
                trip.temp != null &&
                trip.temp > 0 &&
                temp >= trip.temp
                  ? "destructive"
                  : "outline"
              }
              className="font-mono"
            >
              {TRIP_TEXT[trip.type] ? t(TRIP_TEXT[trip.type]) : trip.type}{" "}
              {celsius(trip.temp)}
            </Badge>
          ))}
        </div>
      );
    },
  }),
  c.display({
    id: "cooling",
    header: () => t("system.cpu.thermal.col.cooling"),
    cell: (x) => {
      const cool = x.row.original.cooling;
      if (!cool.length)
        return (
          <span className="text-muted-foreground">
            {t("common.state.none")}
          </span>
        );
      return (
        <div className="flex flex-wrap gap-1">
          {cool.map((d, i) => (
            <Badge
              key={i}
              variant={d.state > 0 ? "destructive" : "outline"}
              className="font-mono"
            >
              {d.type} {d.state}/{d.max}
            </Badge>
          ))}
        </div>
      );
    },
  }),
];

/** Zones shown before "show all": the hottest few say enough. */
const FIRST = 6;

/** Known parts of a zone's name, longest match first, and what a person calls them. */
const ZONE_NAMES: [RegExp, Key][] = [
  [/^(soc|ap|tsens)/, "system.cpu.thermal.zone.soc"],
  [/^(cpu|big|mid|little|lit|cluster|prime)/, "system.cpu.thermal.zone.cpu"],
  [/^gpu/, "system.cpu.thermal.zone.gpu"],
  [/^board/, "system.cpu.thermal.zone.board"],
  [/^(chg|charger)/, "system.cpu.thermal.zone.chg"],
  [/^(battery|bat)/, "system.cpu.thermal.zone.battery"],
  [/^nr/, "system.cpu.thermal.zone.nr"],
  [/^lte/, "system.cpu.thermal.zone.lte"],
  [/^pa/, "system.cpu.thermal.zone.pa"],
  [/^(modem|mdm)/, "system.cpu.thermal.zone.modem"],
  [/^(ddr|dram|mem)/, "system.cpu.thermal.zone.ddr"],
  [/^skin/, "system.cpu.thermal.zone.skin"],
];

/** "soc-thmzone" -> "处理器"; a name nothing matches keeps its kernel name. */
function zoneName(zone: string): string {
  const base = zone
    .toLowerCase()
    .replace(/[-_]?(thmzone|thermal|therm|tz)$/, "");
  const hit = ZONE_NAMES.find(([re]) => re.test(base));
  return hit ? t(hit[1]) : zone;
}

const deg = (mc: number) => `${(mc / 1000).toFixed(1)} °C`;

/** How far the nearest passive trip point (where the kernel starts slowing the CPU) is above its zone's temperature. */
function headroom(zones: ThermalZone[]): number | null {
  let best: number | null = null;
  for (const z of zones) {
    if (z.temp == null) continue;
    for (const trip of z.trips) {
      if (trip.type !== "passive" || trip.temp == null || trip.temp <= 0)
        continue;
      const d = trip.temp - z.temp;
      if (best == null || d < best) best = d;
    }
  }
  return best;
}

/**
 * The answer first: the hottest reading, whether that is fine, and how far the CPU is from being slowed down. Then
 * each part's temperature as a filled bar (amber from 70 °C, red from 90 °C), hottest first; the kernel's trip points
 * and cooling devices are folded away for whoever needs them.
 */
export function ThermalCard({
  zones,
  throttling,
}: {
  zones: ThermalZone[];
  throttling: boolean;
}) {
  const [all, setAll] = useState(false);
  const sorted = [...zones].sort((a, b) => (b.temp ?? -1e9) - (a.temp ?? -1e9));
  const hottest = sorted.find((z) => z.temp != null) ?? null;
  const room = headroom(zones);
  const hot = hottest?.temp ?? 0;
  const level =
    throttling || hot >= 85_000 ? "hot" : hot >= 70_000 ? "warm" : "ok";
  const shown = all ? sorted : sorted.slice(0, FIRST);

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.cpu.thermal.title")}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {hottest?.temp != null ? (
          <div className="flex items-center gap-3">
            <span
              className={cn(
                "flex size-11 shrink-0 items-center justify-center rounded-xl",
                level === "hot"
                  ? "bg-destructive/12 text-destructive"
                  : level === "warm"
                    ? "bg-amber-500/14 text-amber-600 dark:text-amber-400"
                    : "bg-emerald-500/12 text-emerald-600 dark:text-emerald-400",
              )}
            >
              <ThermometerIcon aria-hidden className="size-5" />
            </span>
            <div className="flex min-w-0 flex-col gap-0.5">
              <div className="flex items-baseline gap-2">
                <span className="text-2xl leading-none font-semibold tabular-nums">
                  {deg(hottest.temp)}
                </span>
                <span className="text-sm font-medium">
                  {t(`system.cpu.thermal.${level}`)}
                </span>
              </div>
              <span className="text-xs text-muted-foreground">
                {t("system.cpu.thermal.hottest")}{" "}
                {t("system.cpu.thermal.hottestAt", {
                  zone: zoneName(hottest.zone),
                })}
                {" · "}
                {throttling
                  ? t("system.cpu.thermal.throttling")
                  : room == null
                    ? null
                    : room <= 0
                      ? t("system.cpu.thermal.atTrip")
                      : t("system.cpu.thermal.headroom", { deg: deg(room) })}
              </span>
            </div>
          </div>
        ) : (
          <span className="text-sm text-muted-foreground">
            {t("system.cpu.thermal.empty")}
          </span>
        )}

        {sorted.length > 0 && (
          <div className="flex flex-col gap-2">
            <span className="text-xs font-medium text-muted-foreground">
              {t("system.cpu.thermal.zones")}
            </span>
            <div className="grid gap-2 sm:grid-cols-2">
              {shown.map((z, i) => (
                <FillMeter
                  key={`${z.zone}-${i}`}
                  label={zoneName(z.zone)}
                  value={z.temp == null ? "—" : deg(z.temp)}
                  percent={z.temp == null ? null : z.temp / 1000}
                  hint={z.zone}
                />
              ))}
            </div>
            {sorted.length > FIRST && (
              <Button
                variant="ghost"
                size="sm"
                className="self-start"
                onClick={() => setAll(!all)}
              >
                {all
                  ? t("system.cpu.thermal.showLess")
                  : t("system.cpu.thermal.showAll", { count: sorted.length })}
              </Button>
            )}
          </div>
        )}

        {zones.length > 0 && (
          <Collapsible className="border-t pt-3">
            <CollapsibleTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="group -ml-2 text-muted-foreground"
              >
                <ChevronDownIcon
                  data-icon="inline-start"
                  className="transition-transform group-data-[state=open]:rotate-180"
                />
                {t("system.cpu.thermal.details")}
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="flex flex-col gap-2 pt-2">
              <p className="text-xs text-muted-foreground">
                {t("system.cpu.thermal.description")}
              </p>
              <DataTable
                data={sorted}
                columns={columns}
                getRowId={(r, i) => `${r.zone}-${i}`}
                empty={t("system.cpu.thermal.empty")}
              />
            </CollapsibleContent>
          </Collapsible>
        )}
      </CardContent>
    </Card>
  );
}

import { DataTable } from "@/components/f50/data-table";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { type Key, t } from "@/i18n";
import { type TableColumn, columnHelper } from "@/lib/table";
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
    cell: (x) => <span className="font-mono">{x.getValue()}</span>,
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

export function ThermalCard({ zones }: { zones: ThermalZone[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.cpu.thermal.title")}</CardTitle>
        <CardDescription>{t("system.cpu.thermal.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <DataTable
          data={zones}
          columns={columns}
          getRowId={(r, i) => `${r.zone}-${i}`}
          empty={t("system.cpu.thermal.empty")}
        />
      </CardContent>
    </Card>
  );
}

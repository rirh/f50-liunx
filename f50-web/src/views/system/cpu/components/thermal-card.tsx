import { DataTable } from "@/components/f50/data-table";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { type TableColumn, columnHelper } from "@/lib/table";
import type { ThermalZone } from "../types";

const celsius = (mc: number | null) =>
  mc == null ? "—" : `${(mc / 1000).toFixed(1)} °C`;

const TRIP_TEXT: Record<string, string> = {
  passive: "被动降频",
  active: "主动散热",
  hot: "过热",
  critical: "临界",
};

const c = columnHelper<ThermalZone>();

const columns: TableColumn<ThermalZone>[] = [
  c.accessor("zone", {
    header: "温区",
    cell: (x) => <span className="font-mono">{x.getValue()}</span>,
  }),
  c.accessor("temp", {
    header: "温度",
    sortUndefined: "last",
    cell: (x) => <span className="tabular-nums">{celsius(x.getValue())}</span>,
  }),
  c.accessor((r) => r.mode ?? "", {
    id: "mode",
    header: "状态",
    cell: (x) =>
      x.getValue() === "enabled" ? (
        <Badge variant="secondary">启用</Badge>
      ) : x.getValue() ? (
        <Badge variant="outline">停用</Badge>
      ) : (
        "—"
      ),
  }),
  c.display({
    id: "trips",
    header: "触发点",
    cell: (x) => {
      const trips = x.row.original.trips;
      if (!trips.length)
        return <span className="text-muted-foreground">无</span>;
      const temp = x.row.original.temp;
      return (
        <div className="flex flex-wrap gap-1">
          {trips.map((t, i) => (
            <Badge
              key={i}
              variant={
                temp != null && t.temp != null && t.temp > 0 && temp >= t.temp
                  ? "destructive"
                  : "outline"
              }
              className="font-mono"
            >
              {TRIP_TEXT[t.type] ?? t.type} {celsius(t.temp)}
            </Badge>
          ))}
        </div>
      );
    },
  }),
  c.display({
    id: "cooling",
    header: "散热设备",
    cell: (x) => {
      const cool = x.row.original.cooling;
      if (!cool.length)
        return <span className="text-muted-foreground">无</span>;
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
        <CardTitle>温控</CardTitle>
        <CardDescription>
          内核温区、触发点与绑定的散热设备（状态大于 0 表示正在降频或散热）
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DataTable
          data={zones}
          columns={columns}
          getRowId={(r, i) => `${r.zone}-${i}`}
          empty="没有温区信息"
        />
      </CardContent>
    </Card>
  );
}

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import {
  createColumnHelper,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import type { CycleUsage } from "../types";

const features = tableFeatures({});
const col = createColumnHelper<typeof features, CycleUsage>();
const columns = col.columns([
  col.accessor((r) => `${r.start} ~ ${r.last}`, {
    id: "period",
    header: () => t("cellular.traffic.cycles.period"),
  }),
  col.accessor("rx", {
    header: () => t("cellular.traffic.download"),
    cell: (c) => formatBytes(c.getValue()),
  }),
  col.accessor("tx", {
    header: () => t("cellular.traffic.upload"),
    cell: (c) => formatBytes(c.getValue()),
  }),
  col.accessor("used", {
    header: () => t("cellular.traffic.cycles.total"),
    cell: (c) => formatBytes(c.getValue()),
  }),
]);

/** Up to twelve billing cycles, the current first. */
export function CyclesTable({ cycles }: { cycles: CycleUsage[] }) {
  const table = useTable({
    features,
    columns,
    data: cycles,
    getRowId: (r) => r.start,
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.traffic.cycles.title")}</CardTitle>
        <CardDescription>
          {t("cellular.traffic.cycles.description", { count: cycles.length })}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="overflow-x-auto rounded-lg border">
          <Table>
            <TableHeader>
              {table.getHeaderGroups().map((g) => (
                <TableRow key={g.id}>
                  {g.headers.map((h) => (
                    <TableHead key={h.id}>
                      <table.FlexRender header={h} />
                    </TableHead>
                  ))}
                </TableRow>
              ))}
            </TableHeader>
            <TableBody>
              {table.getRowModel().rows.map((r) => (
                <TableRow key={r.id}>
                  {r.getAllCells().map((c) => (
                    <TableCell
                      key={c.id}
                      className="whitespace-nowrap tabular-nums"
                    >
                      <table.FlexRender cell={c} />
                    </TableCell>
                  ))}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  );
}

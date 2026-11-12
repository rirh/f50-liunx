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
    header: "周期",
  }),
  col.accessor("rx", {
    header: "下载",
    cell: (c) => formatBytes(c.getValue()),
  }),
  col.accessor("tx", {
    header: "上传",
    cell: (c) => formatBytes(c.getValue()),
  }),
  col.accessor("used", {
    header: "合计",
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
        <CardTitle>历史周期</CardTitle>
        <CardDescription>最近 {cycles.length} 个计费周期</CardDescription>
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

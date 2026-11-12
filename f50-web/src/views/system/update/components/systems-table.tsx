import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  createColumnHelper,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import type { SystemCopy } from "../types";

const features = tableFeatures({});
const col = createColumnHelper<typeof features, SystemCopy>();

const columns = [
  col.display({
    id: "name",
    header: "系统",
    cell: ({ row }) => (
      <span className="flex items-center gap-2">
        <span className="font-mono">{row.original.name}</span>
        {row.original.running && <Badge variant="secondary">运行中</Badge>}
      </span>
    ),
  }),
  col.display({
    id: "release",
    header: "版本",
    cell: ({ row }) => (
      <span className="font-mono">{row.original.release ?? "—"}</span>
    ),
  }),
  col.display({
    id: "previous",
    header: "回滚备份",
    cell: ({ row }) => {
      const p = row.original.previous;
      if (!p) return <span className="text-muted-foreground">无</span>;
      return (
        <span className="flex items-center gap-2">
          <span className="font-mono">{p.release ?? "未知版本"}</span>
          {p.running && <Badge variant="outline">当前运行</Badge>}
        </span>
      );
    },
  }),
];

/** The systems on the Linux partition and the copy each keeps for a rollback. */
export function SystemsTable({ systems }: { systems: SystemCopy[] }) {
  const table = useTable({
    features,
    columns,
    data: systems,
    getRowId: (x) => x.name,
  });
  return (
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
        {table.getRowModel().rows.length ? (
          table.getRowModel().rows.map((r) => (
            <TableRow key={r.id}>
              {r.getAllCells().map((c) => (
                <TableCell key={c.id}>
                  <table.FlexRender cell={c} />
                </TableCell>
              ))}
            </TableRow>
          ))
        ) : (
          <TableRow>
            <TableCell
              colSpan={columns.length}
              className="text-center text-muted-foreground"
            >
              没有已安装的系统
            </TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  );
}

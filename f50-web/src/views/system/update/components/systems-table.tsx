import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { t } from "@/i18n";
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
    header: () => t("maintenance.update.systems.name"),
    cell: ({ row }) => (
      <span className="flex items-center gap-2">
        <span className="tabular-nums">{row.original.name}</span>
        {row.original.running && (
          <Badge variant="secondary">{t("common.state.running")}</Badge>
        )}
      </span>
    ),
  }),
  col.display({
    id: "release",
    header: () => t("maintenance.update.systems.release"),
    cell: ({ row }) => (
      <span className="tabular-nums">{row.original.release ?? "—"}</span>
    ),
  }),
  col.display({
    id: "previous",
    header: () => t("maintenance.update.systems.previous"),
    cell: ({ row }) => {
      const p = row.original.previous;
      if (!p)
        return (
          <span className="text-muted-foreground">
            {t("common.state.none")}
          </span>
        );
      return (
        <span className="flex items-center gap-2">
          <span className="tabular-nums">
            {p.release ?? t("maintenance.update.status.unknownRelease")}
          </span>
          {p.running && (
            <Badge variant="outline">
              {t("maintenance.update.systems.running")}
            </Badge>
          )}
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
              {t("maintenance.update.systems.empty")}
            </TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  );
}

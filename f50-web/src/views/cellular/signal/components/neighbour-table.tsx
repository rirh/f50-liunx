import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
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
  createSortedRowModel,
  rowSortingFeature,
  sortFn_basic,
  sortFn_text,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import { ArrowDownIcon, ArrowUpDownIcon, ArrowUpIcon } from "lucide-react";
import type { Neighbour } from "../types";

const features = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: { basic: sortFn_basic, text: sortFn_text },
});

const col = createColumnHelper<typeof features, Neighbour>();
const show = (v: number | null | undefined) => (v == null ? "—" : String(v));
/** A missing reading sorts below every real one. */
const orLow = (v: number | null | undefined) => v ?? Number.NEGATIVE_INFINITY;

const columns = col.columns([
  col.accessor("rat", {
    header: () => t("cellular.signal.neighbours.rat"),
    sortFn: "text",
    cell: (c) => (
      <Badge variant="secondary">{c.getValue() === "nr" ? "NR" : "LTE"}</Badge>
    ),
  }),
  col.accessor("band", {
    header: () => t("cellular.signal.neighbours.band"),
    sortFn: "basic",
    cell: (c) => `${c.row.original.rat === "nr" ? "n" : "B"}${c.getValue()}`,
  }),
  col.accessor("arfcn", { header: "ARFCN", sortFn: "basic" }),
  col.accessor("pci", { header: "PCI", sortFn: "basic" }),
  col.accessor((r) => orLow(r.rsrp), {
    id: "rsrp",
    header: "RSRP",
    sortFn: "basic",
    cell: (c) => show(c.row.original.rsrp),
  }),
  col.accessor((r) => orLow(r.rsrq), {
    id: "rsrq",
    header: "RSRQ",
    sortFn: "basic",
    cell: (c) => show(c.row.original.rsrq),
  }),
  col.accessor((r) => orLow(r.sinr), {
    id: "sinr",
    header: "SINR",
    sortFn: "basic",
    cell: (c) => show(c.row.original.sinr),
  }),
]);

/** The neighbour cells, NR first as read; sortable by any column. */
export function NeighbourTable({ rows }: { rows: Neighbour[] }) {
  const table = useTable({
    features,
    columns,
    data: rows,
    getRowId: (r, i) => `${r.rat}-${r.arfcn}-${r.pci}-${i}`,
  });

  if (rows.length === 0) {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>{t("cellular.signal.neighbours.emptyTitle")}</EmptyTitle>
          <EmptyDescription>
            {t("cellular.signal.neighbours.emptyDescription")}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  return (
    <div className="overflow-x-auto rounded-lg border">
      <Table>
        <TableHeader>
          {table.getHeaderGroups().map((g) => (
            <TableRow key={g.id}>
              {g.headers.map((h) => {
                const sorted = h.column.getIsSorted();
                return (
                  <TableHead key={h.id}>
                    <Button
                      variant="ghost"
                      size="xs"
                      className="-ml-2"
                      onClick={h.column.getToggleSortingHandler()}
                      aria-sort={
                        sorted === "asc"
                          ? "ascending"
                          : sorted === "desc"
                            ? "descending"
                            : "none"
                      }
                    >
                      <table.FlexRender header={h} />
                      {sorted === "asc" ? (
                        <ArrowUpIcon data-icon="inline-end" />
                      ) : sorted === "desc" ? (
                        <ArrowDownIcon data-icon="inline-end" />
                      ) : (
                        <ArrowUpDownIcon data-icon="inline-end" />
                      )}
                    </Button>
                  </TableHead>
                );
              })}
            </TableRow>
          ))}
        </TableHeader>
        <TableBody>
          {table.getRowModel().rows.map((r) => (
            <TableRow key={r.id}>
              {r.getAllCells().map((c) => (
                <TableCell key={c.id} className="tabular-nums">
                  <table.FlexRender cell={c} />
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

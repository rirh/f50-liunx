import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { type TableColumn, tableFeaturesSet } from "@/lib/table";
import { type RowData, useTable } from "@tanstack/react-table";
import { ArrowDownIcon, ArrowUpIcon } from "lucide-react";

/**
 * A small client-side table: TanStack Table v9 for sorting and the global filter, shadcn's Table to draw. `filter` is
 * the global filter text (the caller owns the input). Keep `columns` at module scope (or memoized).
 */
export function DataTable<T extends RowData>({
  data,
  columns,
  getRowId,
  filter = "",
  empty = "暂无数据",
}: {
  data: T[];
  columns: TableColumn<T>[];
  getRowId: (row: T, index: number) => string;
  filter?: string;
  empty?: string;
}) {
  const table = useTable({
    features: tableFeaturesSet,
    columns,
    data,
    getRowId,
    globalFilterFn: "includesString",
    state: { globalFilter: filter },
    onGlobalFilterChange: () => undefined,
  });
  const rows = table.getRowModel().rows;
  return (
    <div className="overflow-x-auto rounded-md border">
      <Table>
        <TableHeader>
          {table.getHeaderGroups().map((hg) => (
            <TableRow key={hg.id}>
              {hg.headers.map((h) => {
                const sorted = h.column.getIsSorted();
                return (
                  <TableHead
                    key={h.id}
                    className="whitespace-nowrap"
                    aria-sort={
                      sorted
                        ? sorted === "asc"
                          ? "ascending"
                          : "descending"
                        : undefined
                    }
                  >
                    {h.isPlaceholder ? null : h.column.getCanSort() ? (
                      <Button
                        variant="ghost"
                        size="xs"
                        className="-ml-2"
                        onClick={h.column.getToggleSortingHandler()}
                      >
                        <table.FlexRender header={h} />
                        {sorted === "asc" && (
                          <ArrowUpIcon data-icon="inline-end" />
                        )}
                        {sorted === "desc" && (
                          <ArrowDownIcon data-icon="inline-end" />
                        )}
                      </Button>
                    ) : (
                      <table.FlexRender header={h} />
                    )}
                  </TableHead>
                );
              })}
            </TableRow>
          ))}
        </TableHeader>
        <TableBody>
          {rows.length ? (
            rows.map((row) => (
              <TableRow key={row.id}>
                {row.getAllCells().map((cell) => (
                  <TableCell key={cell.id}>
                    <table.FlexRender cell={cell} />
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell
                colSpan={columns.length}
                className="h-16 text-center text-muted-foreground"
              >
                {empty}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </div>
  );
}

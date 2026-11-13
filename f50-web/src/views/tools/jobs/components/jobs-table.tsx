import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { t } from "@/i18n";
import { formatAgo, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
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
import { CancelJobButton } from "./cancel-job-button";
import { JobStateBadge } from "./job-state-badge";
import { jobKindLabel, jobMethodLabel, type JobStatus } from "../types";

const features = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: { basic: sortFn_basic, text: sortFn_text },
});

const col = createColumnHelper<typeof features, JobStatus>();

function Time({ unix, fallback }: { unix?: number; fallback: string }) {
  if (!unix) return <span className="text-muted-foreground">{fallback}</span>;
  return (
    <time
      className="whitespace-nowrap"
      title={formatTime(unix)}
      dateTime={new Date(unix * 1000).toISOString()}
    >
      {formatAgo(unix)}
    </time>
  );
}

const columns = col.columns([
  col.accessor("method", {
    header: () => t("tools.jobs.field.action"),
    sortFn: "text",
    cell: (info) => (
      // one line: the id is in the tooltip and the details
      <span title={info.row.original.id}>
        {jobMethodLabel(info.getValue())}
      </span>
    ),
  }),
  col.accessor("kind", {
    header: () => t("tools.jobs.field.kind"),
    sortFn: "text",
    cell: (info) => (
      <span className="text-muted-foreground">
        {jobKindLabel(info.getValue())}
      </span>
    ),
  }),
  col.accessor("state", {
    header: () => t("tools.jobs.field.state"),
    sortFn: "text",
    cell: (info) => <JobStateBadge state={info.getValue()} />,
  }),
  col.accessor("started", {
    header: () => t("tools.jobs.field.started"),
    sortFn: "basic",
    cell: (info) => <Time unix={info.getValue()} fallback="—" />,
  }),
  col.accessor((j) => j.finished ?? 0, {
    id: "finished",
    header: () => t("tools.jobs.field.finished"),
    sortFn: "basic",
    cell: (info) => (
      <Time
        unix={info.row.original.finished}
        fallback={
          info.row.original.state === "running"
            ? t("tools.jobs.detail.inProgress")
            : "—"
        }
      />
    ),
  }),
  col.display({
    id: "actions",
    header: () => (
      <span className="sr-only">{t("tools.jobs.field.action")}</span>
    ),
    cell: ({ row }) =>
      row.original.state === "running" && row.original.cancellable ? (
        // the row opens the details; the stop button must not
        <span
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => e.stopPropagation()}
        >
          <CancelJobButton job={row.original} size="xs" />
        </span>
      ) : row.original.cancellable ? null : (
        <span className="text-xs text-muted-foreground">
          {t("tools.jobs.table.notCancellable")}
        </span>
      ),
  }),
]);

/** Jobs newest first; a click (or Enter) on a row opens its details. */
export function JobsTable({
  data,
  onOpen,
}: {
  data: JobStatus[];
  onOpen: (id: string) => void;
}) {
  const table = useTable({
    features,
    columns,
    data,
    getRowId: (j) => j.id,
    initialState: { sorting: [{ id: "started", desc: true }] },
  });

  return (
    <Table>
      <TableHeader>
        {table.getHeaderGroups().map((g) => (
          <TableRow key={g.id}>
            {g.headers.map((h) => {
              const sorted = h.column.getIsSorted();
              const hideOnPhone =
                h.column.id === "kind" || h.column.id === "finished";
              return (
                <TableHead
                  key={h.id}
                  className={cn(
                    "h-8 text-xs text-muted-foreground",
                    hideOnPhone && "hidden sm:table-cell",
                  )}
                >
                  {h.isPlaceholder ? null : h.column.getCanSort() ? (
                    <Button
                      variant="ghost"
                      size="xs"
                      className="-ml-2 text-xs text-muted-foreground"
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
        {table.getRowModel().rows.map((row) => (
          <TableRow
            key={row.id}
            tabIndex={0}
            className="cursor-pointer"
            onClick={() => onOpen(row.original.id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onOpen(row.original.id);
              }
            }}
          >
            {row.getAllCells().map((cell) => (
              <TableCell
                key={cell.id}
                className={cn(
                  "py-1.5 tabular-nums",
                  (cell.column.id === "kind" ||
                    cell.column.id === "finished") &&
                    "hidden sm:table-cell",
                )}
              >
                <table.FlexRender cell={cell} />
              </TableCell>
            ))}
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

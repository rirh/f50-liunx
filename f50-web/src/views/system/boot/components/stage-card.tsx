import { DataTable } from "@/components/f50/data-table";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { type Key, t } from "@/i18n";
import { formatTime } from "@/lib/format";
import { type TableColumn, columnHelper } from "@/lib/table";
import type { Outcome, Stage, StageReport, Step } from "../types";

const STAGE_TEXT = {
  early: {
    label: "maintenance.boot.stage.early.label",
    description: "maintenance.boot.stage.early.description",
  },
  vendor: {
    label: "maintenance.boot.stage.vendor.label",
    description: "maintenance.boot.stage.vendor.description",
  },
  hardware: {
    label: "maintenance.boot.stage.hardware.label",
    description: "maintenance.boot.stage.hardware.description",
  },
  post: {
    label: "maintenance.boot.stage.post.label",
    description: "maintenance.boot.stage.post.description",
  },
} satisfies Record<Stage, { label: Key; description: Key }>;

const OUTCOME: Record<
  Outcome,
  { label: Key; variant: "secondary" | "outline" | "destructive" }
> = {
  done: { label: "maintenance.boot.outcome.done", variant: "secondary" },
  skipped: { label: "maintenance.boot.outcome.skipped", variant: "outline" },
  failed: { label: "maintenance.boot.outcome.failed", variant: "destructive" },
};

const c = columnHelper<Step>();
const columns: TableColumn<Step>[] = [
  c.accessor("name", {
    header: () => t("maintenance.boot.step.name"),
    enableSorting: false,
    cell: (x) => (
      <span className="tabular-nums whitespace-nowrap">{x.getValue()}</span>
    ),
  }),
  c.accessor("outcome", {
    header: () => t("maintenance.boot.step.outcome"),
    enableSorting: false,
    cell: (x) => {
      const o = OUTCOME[x.row.original.outcome];
      return <Badge variant={o.variant}>{t(o.label)}</Badge>;
    },
  }),
  c.accessor("detail", {
    header: () => t("maintenance.boot.step.detail"),
    enableSorting: false,
    cell: (x) => (
      <span className="block max-w-xl text-xs break-words whitespace-normal">
        {x.getValue() || "—"}
      </span>
    ),
  }),
];

export function StageCard({
  stage,
  report,
  filter,
}: {
  stage: Stage;
  report?: StageReport;
  filter: string;
}) {
  const failed =
    report?.steps.filter((s) => s.outcome === "failed").length ?? 0;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t(STAGE_TEXT[stage].label)}</CardTitle>
        <CardDescription>
          {t(STAGE_TEXT[stage].description)}
          {report && (
            <>
              {" · "}
              {report.started
                ? formatTime(report.started)
                : t("maintenance.boot.stage.noClock")}
              {report.uptime != null &&
                ` · ${t("maintenance.boot.stage.uptime", { n: report.uptime.toFixed(1) })}`}
              {` · ${t("maintenance.boot.stage.duration", { n: (report.duration_ms / 1000).toFixed(1) })}`}
            </>
          )}
        </CardDescription>
        <CardAction>
          {!report ? (
            <Badge variant="outline">
              {t("maintenance.boot.stage.noReport")}
            </Badge>
          ) : report.ok ? (
            <Badge variant="secondary">
              {failed
                ? t("maintenance.boot.stage.okWithFailures", { count: failed })
                : t("maintenance.boot.stage.ok")}
            </Badge>
          ) : (
            <Badge variant="destructive">{t("common.state.failed")}</Badge>
          )}
        </CardAction>
      </CardHeader>
      {report && (
        <CardContent>
          <DataTable
            data={report.steps}
            columns={columns}
            getRowId={(s, i) => `${i}-${s.name}`}
            filter={filter}
            empty={
              filter
                ? t("maintenance.boot.stage.noMatch")
                : t("maintenance.boot.stage.noSteps")
            }
          />
        </CardContent>
      )}
    </Card>
  );
}

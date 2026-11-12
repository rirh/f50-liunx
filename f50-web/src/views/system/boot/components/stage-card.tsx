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
import { formatTime } from "@/lib/format";
import { type TableColumn, columnHelper } from "@/lib/table";
import type { Outcome, Stage, StageReport, Step } from "../types";

const STAGE_TEXT: Record<Stage, { label: string; description: string }> = {
  early: { label: "早期（preinit）", description: "启动早期的日志记录器" },
  vendor: {
    label: "厂商（S09）",
    description: "厂商守护进程的 chroot 与基带控制",
  },
  hardware: {
    label: "硬件（S12）",
    description: "模块加载助手、Wi-Fi 法规与厂商驱动",
  },
  post: { label: "收尾（S99）", description: "延时后把本次启动标记为成功" },
};

const OUTCOME: Record<
  Outcome,
  { label: string; variant: "secondary" | "outline" | "destructive" }
> = {
  done: { label: "完成", variant: "secondary" },
  skipped: { label: "跳过", variant: "outline" },
  failed: { label: "失败", variant: "destructive" },
};

const c = columnHelper<Step>();
const columns: TableColumn<Step>[] = [
  c.accessor("name", {
    header: "步骤",
    enableSorting: false,
    cell: (x) => (
      <span className="font-mono whitespace-nowrap">{x.getValue()}</span>
    ),
  }),
  c.accessor("outcome", {
    header: "结果",
    enableSorting: false,
    cell: (x) => {
      const o = OUTCOME[x.row.original.outcome];
      return <Badge variant={o.variant}>{o.label}</Badge>;
    },
  }),
  c.accessor("detail", {
    header: "说明",
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
        <CardTitle>{STAGE_TEXT[stage].label}</CardTitle>
        <CardDescription>
          {STAGE_TEXT[stage].description}
          {report && (
            <>
              {" · "}
              {report.started ? formatTime(report.started) : "时钟未同步"}
              {report.uptime != null &&
                ` · 开机后 ${report.uptime.toFixed(1)} 秒`}
              {` · 用时 ${(report.duration_ms / 1000).toFixed(1)} 秒`}
            </>
          )}
        </CardDescription>
        <CardAction>
          {!report ? (
            <Badge variant="outline">无记录</Badge>
          ) : report.ok ? (
            <Badge variant="secondary">
              {failed ? `成功（${failed} 项非致命失败）` : "成功"}
            </Badge>
          ) : (
            <Badge variant="destructive">失败</Badge>
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
            empty={filter ? "没有匹配的步骤" : "此阶段没有步骤"}
          />
        </CardContent>
      )}
    </Card>
  );
}

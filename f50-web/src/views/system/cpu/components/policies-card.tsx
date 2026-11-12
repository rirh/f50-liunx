import { DataTable } from "@/components/f50/data-table";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { type TableColumn, columnHelper } from "@/lib/table";
import type { CpuPolicy } from "../types";

const mhz = (n: number | null) => (n == null ? "—" : `${n} MHz`);
const c = columnHelper<CpuPolicy>();

const columns: TableColumn<CpuPolicy>[] = [
  c.accessor("name", {
    header: "策略",
    cell: (x) => <span className="font-mono">{x.getValue()}</span>,
  }),
  c.accessor("cpus", {
    header: "核心",
    enableSorting: false,
    cell: (x) => <span className="font-mono">{x.getValue()}</span>,
  }),
  c.accessor((r) => r.governor ?? "", {
    id: "governor",
    header: "调度器",
    cell: (x) => x.getValue() || "—",
  }),
  c.accessor("cur", {
    header: "当前频率",
    cell: (x) => {
      const r = x.row.original;
      const pct = r.cur != null && r.hwmax ? (r.cur / r.hwmax) * 100 : null;
      return (
        <div className="flex min-w-28 flex-col gap-1">
          <span className="tabular-nums">{mhz(r.cur)}</span>
          {pct != null && (
            <Progress value={pct} aria-label={`${r.name} 当前频率`} />
          )}
        </div>
      );
    },
  }),
  c.accessor((r) => `${mhz(r.min)} – ${mhz(r.max)}`, {
    id: "limit",
    header: "频率限制",
    enableSorting: false,
  }),
  c.accessor((r) => `${mhz(r.hwmin)} – ${mhz(r.hwmax)}`, {
    id: "hw",
    header: "硬件范围",
    enableSorting: false,
  }),
];

export function PoliciesCard({ policies }: { policies: CpuPolicy[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>频率策略</CardTitle>
        <CardDescription>
          每个 CPU 簇的调度器与频率（每 5 秒刷新）
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DataTable
          data={policies}
          columns={columns}
          getRowId={(r) => r.name}
          empty="内核没有提供 CPU 调频（cpufreq）"
        />
      </CardContent>
    </Card>
  );
}

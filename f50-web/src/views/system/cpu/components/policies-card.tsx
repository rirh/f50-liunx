import { DataTable } from "@/components/f50/data-table";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { t } from "@/i18n";
import { type TableColumn, columnHelper } from "@/lib/table";
import type { CpuPolicy } from "../types";

const mhz = (n: number | null) => (n == null ? "—" : `${n} MHz`);
const c = columnHelper<CpuPolicy>();

const columns: TableColumn<CpuPolicy>[] = [
  c.accessor("name", {
    header: () => t("system.cpu.policies.col.name"),
    cell: (x) => <span className="font-mono">{x.getValue()}</span>,
  }),
  c.accessor("cpus", {
    header: () => t("system.cpu.policies.col.cpus"),
    enableSorting: false,
    cell: (x) => <span className="font-mono">{x.getValue()}</span>,
  }),
  c.accessor((r) => r.governor ?? "", {
    id: "governor",
    header: () => t("system.cpu.policies.col.governor"),
    cell: (x) => x.getValue() || "—",
  }),
  c.accessor("cur", {
    header: () => t("system.cpu.policies.col.cur"),
    cell: (x) => {
      const r = x.row.original;
      const pct = r.cur != null && r.hwmax ? (r.cur / r.hwmax) * 100 : null;
      return (
        <div className="flex min-w-28 flex-col gap-1">
          <span className="tabular-nums">{mhz(r.cur)}</span>
          {pct != null && (
            <Progress
              value={pct}
              aria-label={t("system.cpu.policies.curLabel", { name: r.name })}
            />
          )}
        </div>
      );
    },
  }),
  c.accessor((r) => `${mhz(r.min)} – ${mhz(r.max)}`, {
    id: "limit",
    header: () => t("system.cpu.policies.col.limit"),
    enableSorting: false,
  }),
  c.accessor((r) => `${mhz(r.hwmin)} – ${mhz(r.hwmax)}`, {
    id: "hw",
    header: () => t("system.cpu.policies.col.hw"),
    enableSorting: false,
  }),
];

export function PoliciesCard({ policies }: { policies: CpuPolicy[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.cpu.policies.title")}</CardTitle>
        <CardDescription>
          {t("system.cpu.policies.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DataTable
          data={policies}
          columns={columns}
          getRowId={(r) => r.name}
          empty={t("system.cpu.policies.empty")}
        />
      </CardContent>
    </Card>
  );
}

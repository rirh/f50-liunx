import { DataTable } from "@/components/f50/data-table";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import { columnHelper } from "@/lib/table";
import type { WifiStation } from "../types";

const col = columnHelper<WifiStation>();
const columns = [
  col.accessor("name", {
    header: () => t("network.wifi.clients.name"),
    cell: (c) => c.getValue() ?? "—",
  }),
  col.accessor("ip", {
    header: () => t("network.wifi.clients.ip"),
    cell: (c) => <span className="font-mono">{c.getValue() ?? "—"}</span>,
  }),
  col.accessor("mac", {
    header: () => t("network.wifi.clients.mac"),
    cell: (c) => <span className="font-mono">{c.getValue()}</span>,
  }),
];

export function WifiClientsTable({ clients }: { clients: WifiStation[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("network.wifi.clients.title")}</CardTitle>
        <CardDescription>
          {t("network.wifi.clients.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DataTable
          columns={columns}
          data={clients}
          getRowId={(c) => c.mac}
          empty={t("network.wifi.clients.empty")}
        />
      </CardContent>
    </Card>
  );
}

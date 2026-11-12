import { DataTable } from "@/components/f50/data-table";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { columnHelper } from "@/lib/table";
import type { WifiStation } from "../types";

const col = columnHelper<WifiStation>();
const columns = [
  col.accessor("name", { header: "名称", cell: (c) => c.getValue() ?? "—" }),
  col.accessor("ip", {
    header: "IP 地址",
    cell: (c) => <span className="font-mono">{c.getValue() ?? "—"}</span>,
  }),
  col.accessor("mac", {
    header: "MAC 地址",
    cell: (c) => <span className="font-mono">{c.getValue()}</span>,
  }),
];

export function WifiClientsTable({ clients }: { clients: WifiStation[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>已连接设备</CardTitle>
        <CardDescription>
          hostapd 报告的终端，地址和名称来自 DHCP 租约
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DataTable
          columns={columns}
          data={clients}
          getRowId={(c) => c.mac}
          empty="没有设备连接到热点"
        />
      </CardContent>
    </Card>
  );
}

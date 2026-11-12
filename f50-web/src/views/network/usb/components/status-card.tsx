import { DataTable } from "@/components/f50/data-table";
import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { columnHelper } from "@/lib/table";
import { NET_BOUND_LABEL, NET_MODE_LABEL, ROLE_LABEL } from "../labels";
import type { UsbDevice, UsbInfo } from "../types";

const col = columnHelper<UsbDevice>();
const columns = [
  col.accessor("port", {
    header: "端口",
    cell: (c) => <span className="font-mono">{c.getValue() ?? "—"}</span>,
  }),
  col.accessor(
    (d) => [d.manufacturer, d.product].filter(Boolean).join(" ") || "—",
    { id: "name", header: "设备" },
  ),
  col.accessor("id", {
    header: "ID",
    cell: (c) => <span className="font-mono">{c.getValue()}</span>,
  }),
  col.accessor("speed", {
    header: "速率",
    cell: (c) => (c.getValue() ? `${c.getValue()} Mbit/s` : "—"),
  }),
];

export function UsbStatusCard({ info }: { info: UsbInfo }) {
  const link = info.link;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex flex-wrap items-center gap-2">
          USB-C 端口
          <Badge variant="secondary">
            {ROLE_LABEL[info.role] ?? info.role}
          </Badge>
        </CardTitle>
        <CardDescription>角色、USB 网络与连接速率</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <KvList
          items={[
            {
              label: "开机角色",
              value: info.role_boot === "host" ? "主机" : "设备（默认）",
            },
            {
              label: "当前网络功能",
              value: NET_BOUND_LABEL[info.net_current] ?? info.net_current,
            },
            {
              label: "本次开机模式",
              value: info.net_now ? NET_MODE_LABEL[info.net_now] : "—",
            },
            {
              label: "下次开机模式",
              value: `${NET_MODE_LABEL[info.net_mode]}${info.net_scope === "once" ? "（仅一次）" : ""}`,
            },
            {
              label: "电脑端网卡名",
              value: info.net_name === "eth" ? "ethN" : "usbN",
            },
            {
              label: "控制器",
              value: link ? (
                <span className="flex flex-wrap items-center gap-2">
                  <span className="font-mono">{link.udc}</span>
                  <Badge variant={link.bound ? "default" : "secondary"}>
                    {link.bound ? "已绑定" : "未绑定"}
                  </Badge>
                </span>
              ) : (
                "—"
              ),
            },
            { label: "连接状态", value: link?.state },
            {
              label: "连接速率",
              value: link?.speed
                ? `${link.speed}${link.max_speed ? `（最高 ${link.max_speed}）` : ""}`
                : "—",
            },
          ]}
        />
        {info.devices.length > 0 && (
          <DataTable
            columns={columns}
            data={info.devices}
            getRowId={(d, i) => d.port ?? String(i)}
          />
        )}
      </CardContent>
    </Card>
  );
}

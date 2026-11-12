import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { dhcpRange } from "../schema";
import type { LanInfo } from "../types";

export function LanStatusCard({ info }: { info: LanInfo }) {
  const configured = `${info.address}/${info.prefix}`;
  const pending = info.live != null && info.live !== configured;
  const range =
    info.dhcp.start != null && info.dhcp.limit != null
      ? dhcpRange(
          info.address,
          info.netmask,
          String(info.dhcp.start),
          String(info.dhcp.limit),
        )
      : null;
  return (
    <Card>
      <CardHeader>
        <CardTitle>当前局域网</CardTitle>
        <CardDescription>路由器地址与 DHCP 地址池</CardDescription>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            {
              label: "路由器地址",
              value: <span className="font-mono">{configured}</span>,
            },
            {
              label: "子网掩码",
              value: <span className="font-mono">{info.netmask}</span>,
            },
            {
              label: "br-lan 实际地址",
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  <span className="font-mono">{info.live ?? "—"}</span>
                  {pending && <Badge variant="secondary">待生效</Badge>}
                </span>
              ),
            },
            {
              label: "USB 主机地址",
              value: <span className="font-mono">{info.usb_host_address}</span>,
            },
            {
              label: "出厂地址",
              value: <span className="font-mono">{info.default_address}</span>,
            },
            {
              label: "DHCP",
              value: (
                <Badge variant={info.dhcp.enabled ? "default" : "secondary"}>
                  {info.dhcp.enabled ? "已启用" : "已关闭"}
                </Badge>
              ),
            },
            {
              label: "地址池",
              value: range && <span className="font-mono">{range}</span>,
            },
            { label: "租期", value: info.dhcp.leasetime },
          ]}
        />
      </CardContent>
    </Card>
  );
}

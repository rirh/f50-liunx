import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { LeaveDialog } from "./leave-dialog";
import { signalText } from "../security";
import type { ClientMode, ClientStatus, FirewallState } from "../types";

const MODE_LABEL: Record<ClientMode, string> = {
  off: "无线电空闲",
  client: "Wi-Fi 上网",
  hotspot: "热点",
};
const FW_LABEL: Record<FirewallState, string> = {
  none: "无规则",
  closed: "已关闭（不转发）",
  shared: "已共享给局域网",
  unknown: "无法读取",
};

export function ClientStatusCard({
  status,
  busy,
  onLeaveStarted,
}: {
  status: ClientStatus;
  /** A join or leave job is running. */
  busy: boolean;
  onLeaveStarted: (job: string) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex flex-wrap items-center gap-2">
          {status.ssid ?? "未连接"}
          {status.mode === "client" && (
            <Badge variant={status.connected ? "default" : "secondary"}>
              {status.connected ? "已连接" : "连接中"}
            </Badge>
          )}
        </CardTitle>
        <CardDescription>无线电当前：{MODE_LABEL[status.mode]}</CardDescription>
        <CardAction>
          <LeaveDialog
            disabled={busy || !status.active}
            onStarted={onLeaveStarted}
          />
        </CardAction>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            { label: "信号", value: signalText(status.signal) },
            {
              label: "地址",
              value: status.address && (
                <span className="font-mono">{status.address}</span>
              ),
            },
            {
              label: "防火墙",
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  {FW_LABEL[status.firewall]}
                  {!status.firewall_ok && (
                    <Badge variant="destructive">与状态不符</Badge>
                  )}
                </span>
              ),
            },
            {
              label: "已保存网络",
              value: status.saved ? (
                <span className="flex flex-wrap items-center gap-2">
                  {status.saved.ssid}
                  <Badge variant="outline">
                    {status.saved.at_boot ? "开机自动连接" : "开机不连接"}
                  </Badge>
                </span>
              ) : (
                "无"
              ),
            },
          ]}
        />
      </CardContent>
    </Card>
  );
}

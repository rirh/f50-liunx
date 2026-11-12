import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { RadioTowerIcon } from "lucide-react";
import { Figure, Tile } from "./tile";
import { UPLINK_MODE_LABEL } from "../labels";
import type { Part, WifiClientInfo } from "../types";

export function UplinkTile({
  part,
}: {
  part: Part<WifiClientInfo> | undefined;
}) {
  return (
    <Tile
      title="Wi-Fi 上行"
      icon={RadioTowerIcon}
      to="/network/wifi-client"
      part={part}
    >
      {(c) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure
              value={
                c.mode === "client"
                  ? (c.ssid ?? "—")
                  : UPLINK_MODE_LABEL[c.mode]
              }
              caption={
                c.mode === "client"
                  ? c.connected
                    ? "已连接"
                    : "连接中"
                  : "未作为上行使用"
              }
            />
            {c.mode === "client" && (
              <Badge variant={c.connected ? "default" : "outline"}>
                {c.connected ? "在线" : "未连通"}
              </Badge>
            )}
          </div>
          {c.active && !c.firewall_ok && (
            <Badge variant="destructive">防火墙规则异常</Badge>
          )}
          <KvList
            items={[
              {
                label: "信号",
                value: c.signal != null ? `${c.signal} dBm` : "—",
              },
              {
                label: "地址",
                value: c.address ? (
                  <span className="font-mono text-xs">{c.address}</span>
                ) : (
                  "—"
                ),
              },
              {
                label: "已保存网络",
                value: c.saved
                  ? `${c.saved.ssid}${c.saved.at_boot ? "（开机连接）" : ""}`
                  : "无",
              },
            ]}
          />
        </>
      )}
    </Tile>
  );
}

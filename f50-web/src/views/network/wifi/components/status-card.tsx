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
import { WifiPowerControl } from "./power-control";
import type { WifiInfo, WifiSecurity } from "../types";

const SECURITY_LABEL: Record<WifiSecurity, string> = {
  wpa2: "WPA2-PSK",
  "wpa2-wpa3": "WPA2/WPA3 混合",
  wpa3: "WPA3-SAE",
  open: "开放（无密码）",
  other: "其他",
};

export function WifiStatusCard({ info }: { info: WifiInfo }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex flex-wrap items-center gap-2">
          {info.ssid ?? "未命名热点"}
          <Badge variant={info.enabled ? "default" : "secondary"}>
            {info.enabled ? "已开启" : "已关闭"}
          </Badge>
          {info.client_mode && <Badge variant="outline">无线电用于上网</Badge>}
        </CardTitle>
        <CardDescription>热点当前状态</CardDescription>
        <CardAction>
          <WifiPowerControl info={info} />
        </CardAction>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            { label: "频段", value: `${info.band} GHz` },
            {
              label: "信道",
              value: `${info.channel === "auto" ? "自动" : info.channel}${
                info.channel_live != null ? `（当前 ${info.channel_live}）` : ""
              }`,
            },
            { label: "模式", value: info.htmode },
            { label: "国家/地区", value: info.country },
            { label: "安全", value: SECURITY_LABEL[info.security] },
            { label: "密码", value: info.password_set ? "已设置" : "未设置" },
            { label: "隐藏 SSID", value: info.hidden ? "是" : "否" },
            { label: "hostapd", value: info.status ?? "未运行" },
            { label: "已连接设备", value: `${info.clients.length} 台` },
          ]}
        />
      </CardContent>
    </Card>
  );
}

import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { UsbIcon } from "lucide-react";
import { Figure, Tile } from "./tile";
import { USB_ROLE_LABEL } from "../labels";
import type { Part, UsbInfo } from "../types";

export function UsbTile({ part }: { part: Part<UsbInfo> | undefined }) {
  return (
    <Tile title="USB" icon={UsbIcon} to="/network/usb" part={part}>
      {(u) => {
        const linked = u.link?.state === "configured";
        return (
          <>
            <div className="flex items-center justify-between gap-3">
              <Figure
                value={USB_ROLE_LABEL[u.role] ?? u.role}
                caption={
                  u.role === "host"
                    ? `${u.devices.length} 个设备`
                    : `网络模式 ${u.net_current || u.net_mode}`
                }
              />
              {u.role === "device" && (
                <Badge variant={linked ? "default" : "outline"}>
                  {linked ? "已连接电脑" : "未连接"}
                </Badge>
              )}
            </div>
            <KvList
              items={[
                { label: "链路速率", value: u.link?.speed ?? "—" },
                { label: "网络模式", value: u.net_mode },
                { label: "ADB", value: u.adb.on ? "已开启" : "已关闭" },
              ]}
            />
          </>
        );
      }}
    </Tile>
  );
}

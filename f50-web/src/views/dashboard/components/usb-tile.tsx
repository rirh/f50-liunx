import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { t } from "@/i18n";
import { UsbIcon } from "lucide-react";
import { Figure, Tile } from "./tile";
import { USB_ROLE_LABEL, label } from "../labels";
import type { Part, UsbInfo } from "../types";

export function UsbTile({ part }: { part: Part<UsbInfo> | undefined }) {
  return (
    <Tile
      title={t("dashboard.usb.title")}
      icon={UsbIcon}
      to="/network/usb"
      part={part}
    >
      {(u) => {
        const linked = u.link?.state === "configured";
        return (
          <>
            <div className="flex items-center justify-between gap-3">
              <Figure
                value={
                  // a kernel without role switching still runs the port as a device while the gadget is bound
                  u.role === "unavailable" && u.link?.bound
                    ? label(USB_ROLE_LABEL, "device")
                    : label(USB_ROLE_LABEL, u.role)
                }
                caption={
                  u.role === "host"
                    ? t("dashboard.usb.devices", { count: u.devices.length })
                    : t("dashboard.usb.netMode", {
                        mode: u.net_current || u.net_mode,
                      })
                }
              />
              {(u.role === "device" ||
                (u.role === "unavailable" && u.link?.bound)) && (
                <Badge variant={linked ? "default" : "outline"}>
                  {linked
                    ? t("dashboard.usb.linked")
                    : t("dashboard.usb.unlinked")}
                </Badge>
              )}
            </div>
            <KvList
              dense
              items={[
                {
                  label: t("dashboard.usb.speed"),
                  value: u.link?.speed ?? "—",
                },
                {
                  label: t("dashboard.usb.adb"),
                  value: u.adb.on
                    ? t("common.state.enabled")
                    : t("common.state.disabled"),
                },
              ]}
            />
          </>
        );
      }}
    </Tile>
  );
}

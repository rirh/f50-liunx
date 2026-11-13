import { DataTable } from "@/components/f50/data-table";
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
import { t } from "@/i18n";
import { columnHelper } from "@/lib/table";
import { UsbNetDialog } from "./net-card";
import { UsbRoleDialog } from "./role-card";
import { netBoundLabel, netModeLabel, roleLabel } from "../labels";
import type { UsbDevice, UsbInfo } from "../types";

const col = columnHelper<UsbDevice>();
const columns = [
  col.accessor("port", {
    header: () => t("network.usb.status.port"),
    cell: (c) => <span className="tabular-nums">{c.getValue() ?? "—"}</span>,
  }),
  col.accessor(
    (d) => [d.manufacturer, d.product].filter(Boolean).join(" ") || "—",
    { id: "name", header: () => t("network.usb.status.device") },
  ),
  col.accessor("id", {
    header: "ID",
    cell: (c) => <span className="tabular-nums">{c.getValue()}</span>,
  }),
  col.accessor("speed", {
    header: () => t("network.usb.status.speed"),
    cell: (c) => (c.getValue() ? `${c.getValue()} Mbit/s` : "—"),
  }),
];

/** The port's role; a kernel without role switching still runs it as a device while the gadget is bound. */
function roleText(info: UsbInfo) {
  if (info.role === "unavailable" && info.link?.bound)
    return t("network.usb.status.deviceFixed");
  return roleLabel(info.role);
}

/** The port and its gadget as they are; the role and network settings it shows open from its header in dialogs. */
export function UsbStatusCard({ info }: { info: UsbInfo }) {
  const link = info.link;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex flex-wrap items-center gap-2">
          {t("network.usb.status.title")}
          <Badge variant="secondary">{roleText(info)}</Badge>
        </CardTitle>
        <CardDescription>{t("network.usb.status.description")}</CardDescription>
        <CardAction className="flex flex-wrap justify-end gap-2">
          <UsbRoleDialog info={info} />
          <UsbNetDialog info={info} />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <KvList
          items={[
            {
              label: t("network.usb.status.roleBoot"),
              value:
                info.role_boot === "host"
                  ? t("network.usb.status.host")
                  : t("network.usb.status.deviceDefault"),
            },
            {
              label: t("network.usb.status.netCurrent"),
              value: netBoundLabel(info.net_current),
            },
            {
              label: t("network.usb.status.netNow"),
              value: info.net_now ? netModeLabel(info.net_now) : "—",
            },
            {
              label: t("network.usb.status.netNext"),
              value: `${netModeLabel(info.net_mode)}${info.net_scope === "once" ? t("network.usb.status.onceSuffix") : ""}`,
            },
            {
              label: t("network.usb.status.netName"),
              value: info.net_name === "eth" ? "ethN" : "usbN",
            },
            {
              label: t("network.usb.status.controller"),
              value: link ? (
                <span className="flex flex-wrap items-center gap-2">
                  <span className="tabular-nums">{link.udc}</span>
                  <Badge variant={link.bound ? "default" : "secondary"}>
                    {link.bound
                      ? t("network.usb.status.bound")
                      : t("network.usb.status.unbound")}
                  </Badge>
                </span>
              ) : (
                "—"
              ),
            },
            { label: t("network.usb.status.linkState"), value: link?.state },
            {
              label: t("network.usb.status.linkSpeed"),
              value: link?.speed
                ? `${link.speed}${link.max_speed ? t("network.usb.status.maxSpeed", { speed: link.max_speed }) : ""}`
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

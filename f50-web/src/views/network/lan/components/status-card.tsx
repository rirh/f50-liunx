import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
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
        <CardTitle>{t("network.lan.status.title")}</CardTitle>
        <CardDescription>{t("network.lan.status.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            {
              label: t("network.lan.status.address"),
              value: <span className="font-mono">{configured}</span>,
            },
            {
              label: t("network.lan.status.netmask"),
              value: <span className="font-mono">{info.netmask}</span>,
            },
            {
              label: t("network.lan.status.live"),
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  <span className="font-mono">{info.live ?? "—"}</span>
                  {pending && (
                    <Badge variant="secondary">
                      {t("network.lan.status.pending")}
                    </Badge>
                  )}
                </span>
              ),
            },
            {
              label: t("network.lan.status.usbHost"),
              value: <span className="font-mono">{info.usb_host_address}</span>,
            },
            {
              label: t("network.lan.status.default"),
              value: <span className="font-mono">{info.default_address}</span>,
            },
            {
              label: "DHCP",
              value: (
                <Badge variant={info.dhcp.enabled ? "default" : "secondary"}>
                  {info.dhcp.enabled
                    ? t("common.state.enabled")
                    : t("common.state.disabled")}
                </Badge>
              ),
            },
            {
              label: t("network.lan.status.pool"),
              value: range && <span className="font-mono">{range}</span>,
            },
            {
              label: t("network.lan.status.leasetime"),
              value: info.dhcp.leasetime,
            },
          ]}
        />
      </CardContent>
    </Card>
  );
}

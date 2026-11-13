import { Group, Row } from "@/components/f50/grouped-list";
import { Badge } from "@/components/ui/badge";
import { t } from "@/i18n";
import { dhcpRange } from "../schema";
import type { LanInfo } from "../types";

/** The LAN as configured and as live, as two settings-style groups: the router's addresses, then DHCP. */
export function LanStatus({ info }: { info: LanInfo }) {
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
    <div className="flex max-w-2xl flex-col gap-6">
      <Group title={t("network.lan.status.router")}>
        <Row label={t("network.lan.status.address")}>{configured}</Row>
        <Row label={t("network.lan.status.netmask")}>{info.netmask}</Row>
        <Row label={t("network.lan.status.live")}>
          <span className="inline-flex items-center gap-2">
            {pending && (
              <Badge variant="secondary">
                {t("network.lan.status.pending")}
              </Badge>
            )}
            {info.live ?? "—"}
          </span>
        </Row>
        <Row label={t("network.lan.status.usbHost")}>
          {info.usb_host_address}
        </Row>
        <Row label={t("network.lan.status.default")}>
          {info.default_address}
        </Row>
      </Group>
      <Group title="DHCP">
        <Row label="DHCP">
          <span
            className={
              info.dhcp.enabled
                ? "text-[#248A3D] dark:text-[#30D158]"
                : "text-muted-foreground"
            }
          >
            {info.dhcp.enabled
              ? t("common.state.enabled")
              : t("common.state.disabled")}
          </span>
        </Row>
        <Row label={t("network.lan.status.pool")}>{range}</Row>
        <Row label={t("network.lan.status.leasetime")}>
          {info.dhcp.leasetime}
        </Row>
      </Group>
    </div>
  );
}

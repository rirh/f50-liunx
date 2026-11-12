import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { t } from "@/i18n";
import { RadioTowerIcon } from "lucide-react";
import { Figure, Tile } from "./tile";
import { UPLINK_MODE_LABEL, label } from "../labels";
import type { Part, WifiClientInfo } from "../types";

export function UplinkTile({
  part,
}: {
  part: Part<WifiClientInfo> | undefined;
}) {
  return (
    <Tile
      title={t("dashboard.uplink.title")}
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
                  : label(UPLINK_MODE_LABEL, c.mode)
              }
              caption={
                c.mode === "client"
                  ? c.connected
                    ? t("common.state.connected")
                    : t("dashboard.uplink.connecting")
                  : t("dashboard.uplink.unused")
              }
            />
            {c.mode === "client" && (
              <Badge variant={c.connected ? "default" : "outline"}>
                {c.connected
                  ? t("dashboard.uplink.online")
                  : t("dashboard.uplink.offline")}
              </Badge>
            )}
          </div>
          {c.active && !c.firewall_ok && (
            <Badge variant="destructive">
              {t("dashboard.uplink.firewall")}
            </Badge>
          )}
          {c.mode === "client" ? (
            <KvList
              dense
              items={[
                {
                  label: t("dashboard.uplink.signal"),
                  value: c.signal != null ? `${c.signal} dBm` : "—",
                },
                {
                  label: t("dashboard.uplink.address"),
                  value: c.address ? (
                    <span className="font-mono">{c.address}</span>
                  ) : (
                    "—"
                  ),
                },
              ]}
            />
          ) : (
            <KvList
              dense
              items={[
                {
                  label: t("dashboard.uplink.saved"),
                  value: c.saved
                    ? c.saved.at_boot
                      ? t("dashboard.uplink.atBoot", { ssid: c.saved.ssid })
                      : c.saved.ssid
                    : t("common.state.none"),
                },
              ]}
            />
          )}
        </>
      )}
    </Tile>
  );
}

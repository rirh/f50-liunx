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
import { t, type Key } from "@/i18n";
import type { UseQueryResult } from "@tanstack/react-query";
import { WifiPowerControl } from "./power-control";
import { WifiSettingsDialog } from "./settings-form";
import type { WifiCredentials, WifiInfo, WifiSecurity } from "../types";

const SECURITY_LABEL = {
  wpa2: "network.wifi.security.wpa2",
  "wpa2-wpa3": "network.wifi.security.wpa2-wpa3",
  wpa3: "network.wifi.security.wpa3",
  open: "network.wifi.security.open",
  other: "network.wifi.security.other",
} satisfies Record<WifiSecurity, Key>;

/** The hotspot as it is; power applies at once, the settings open in a dialog from the same header. */
export function WifiStatusCard({
  info,
  credentials,
}: {
  info: WifiInfo;
  credentials: UseQueryResult<WifiCredentials>;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex flex-wrap items-center gap-2">
          {info.ssid ?? t("network.wifi.status.unnamed")}
          <Badge variant={info.enabled ? "default" : "secondary"}>
            {info.enabled
              ? t("common.state.enabled")
              : t("common.state.disabled")}
          </Badge>
          {info.client_mode && (
            <Badge variant="outline">
              {t("network.wifi.status.clientMode")}
            </Badge>
          )}
        </CardTitle>
        <CardDescription>
          {t("network.wifi.status.description")}
        </CardDescription>
        <CardAction className="flex flex-wrap justify-end gap-2">
          <WifiSettingsDialog
            info={info}
            password={credentials.data?.password ?? ""}
          />
          <WifiPowerControl info={info} />
        </CardAction>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            { label: t("network.wifi.status.band"), value: `${info.band} GHz` },
            {
              label: t("network.wifi.status.channel"),
              value: `${info.channel === "auto" ? t("common.state.auto") : info.channel}${
                info.channel_live != null
                  ? t("network.wifi.status.channelLive", {
                      channel: info.channel_live,
                    })
                  : ""
              }`,
            },
            { label: t("network.wifi.status.mode"), value: info.htmode },
            { label: t("network.wifi.status.country"), value: info.country },
            {
              label: t("network.wifi.status.security"),
              value: t(SECURITY_LABEL[info.security]),
            },
            {
              label: t("network.wifi.status.password"),
              value: info.password_set
                ? t("network.wifi.status.set")
                : t("network.wifi.status.unset"),
            },
            {
              label: t("network.wifi.status.hidden"),
              value: info.hidden ? t("common.state.yes") : t("common.state.no"),
            },
            {
              label: "hostapd",
              value: info.status ?? t("common.state.stopped"),
            },
            {
              label: t("network.wifi.status.clients"),
              value: t("common.unit.devices", { count: info.clients.length }),
            },
          ]}
        />
      </CardContent>
    </Card>
  );
}

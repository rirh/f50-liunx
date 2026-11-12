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
import { LeaveDialog } from "./leave-dialog";
import { signalText } from "../security";
import type { ClientMode, ClientStatus, FirewallState } from "../types";

const MODE_LABEL = {
  off: "network.client.status.mode.off",
  client: "network.client.status.mode.client",
  hotspot: "network.client.status.mode.hotspot",
} satisfies Record<ClientMode, Key>;
const FW_LABEL = {
  none: "network.client.status.firewall.none",
  closed: "network.client.status.firewall.closed",
  shared: "network.client.status.firewall.shared",
  unknown: "network.client.status.firewall.unknown",
} satisfies Record<FirewallState, Key>;

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
          {status.ssid ?? t("common.state.disconnected")}
          {status.mode === "client" && (
            <Badge variant={status.connected ? "default" : "secondary"}>
              {status.connected
                ? t("common.state.connected")
                : t("network.client.status.connecting")}
            </Badge>
          )}
        </CardTitle>
        <CardDescription>
          {t("network.client.status.radio", {
            mode: t(MODE_LABEL[status.mode]),
          })}
        </CardDescription>
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
            {
              label: t("network.client.status.signal"),
              value: signalText(status.signal),
            },
            {
              label: t("network.client.status.address"),
              value: status.address && (
                <span className="font-mono">{status.address}</span>
              ),
            },
            {
              label: t("network.client.status.firewallLabel"),
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  {t(FW_LABEL[status.firewall])}
                  {!status.firewall_ok && (
                    <Badge variant="destructive">
                      {t("network.client.status.firewallMismatch")}
                    </Badge>
                  )}
                </span>
              ),
            },
            {
              label: t("network.client.status.saved"),
              value: status.saved ? (
                <span className="flex flex-wrap items-center gap-2">
                  {status.saved.ssid}
                  <Badge variant="outline">
                    {status.saved.at_boot
                      ? t("network.client.status.atBoot")
                      : t("network.client.status.notAtBoot")}
                  </Badge>
                </span>
              ) : (
                t("common.state.none")
              ),
            },
          ]}
        />
      </CardContent>
    </Card>
  );
}

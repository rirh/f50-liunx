import { useMemo } from "react";
import { DataTable } from "@/components/f50/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { t } from "@/i18n";
import { columnHelper } from "@/lib/table";
import {
  SECURITY_LABEL,
  joinable,
  signalText,
  whyNotJoinable,
} from "../security";
import type { ScanNetwork } from "../types";

const col = columnHelper<ScanNetwork>();

/** The networks in range; WPA3-only and other unsupported ones are marked and cannot be picked. */
export function NetworksTable({
  networks,
  disabled,
  onJoin,
}: {
  networks: ScanNetwork[];
  disabled: boolean;
  onJoin: (n: ScanNetwork) => void;
}) {
  const columns = useMemo(
    () => [
      col.accessor("ssid", {
        header: t("network.client.networks.name"),
        cell: (c) => <span className="break-all">{c.getValue()}</span>,
      }),
      col.accessor("signal", {
        header: t("network.client.networks.signal"),
        cell: (c) => (
          <span className="whitespace-nowrap">{signalText(c.getValue())}</span>
        ),
      }),
      col.accessor("security", {
        header: t("network.client.networks.security"),
        cell: (c) => {
          const why = whyNotJoinable(c.getValue());
          return (
            <span
              className="flex flex-wrap items-center gap-1"
              title={why ?? undefined}
            >
              <Badge variant={why ? "destructive" : "secondary"}>
                {t(SECURITY_LABEL[c.getValue()])}
              </Badge>
              {why && (
                <span className="text-xs text-muted-foreground">
                  {t("network.client.networks.notJoinable")}
                </span>
              )}
            </span>
          );
        },
      }),
      col.display({
        id: "actions",
        header: () => (
          <span className="sr-only">{t("network.shared.actions")}</span>
        ),
        cell: (c) => (
          <Button
            size="xs"
            variant="outline"
            disabled={disabled || !joinable(c.row.original.security)}
            onClick={() => onJoin(c.row.original)}
          >
            {t("network.client.networks.join")}
          </Button>
        ),
      }),
    ],
    [disabled, onJoin],
  );
  return (
    <DataTable
      columns={columns}
      data={networks}
      getRowId={(n) => n.ssid}
      empty={t("network.client.networks.empty")}
    />
  );
}

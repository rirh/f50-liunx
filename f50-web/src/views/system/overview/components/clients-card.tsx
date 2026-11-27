import { useMemo, useState } from "react";
import { DataTable } from "@/components/f50/data-table";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useLiveTopic } from "@/hooks/use-live";
import { t } from "@/i18n";
import { formatBytes, formatDuration } from "@/lib/format";
import { type TableColumn, columnHelper } from "@/lib/table";
import type { WifiStation } from "@/views/network/wifi/types";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon, SearchIcon } from "lucide-react";
import { getClients, systemKeys } from "../api";
import type { Clients, ConnectedClient, Lease } from "../types";
import { ClientLimitDialog } from "./client-limit-dialog";

const st = columnHelper<ConnectedClient>();
const compactColumns: TableColumn<ConnectedClient>[] = [
  st.accessor((r) => r.host || r.mac, {
    id: "device",
    header: () => t("system.overview.clients.col.name"),
    cell: (c) => {
      const client = c.row.original;
      return (
        <div className="space-y-0.5 whitespace-normal">
          <div className="break-all font-medium">{c.getValue()}</div>
          <div className="text-xs text-muted-foreground tabular-nums">
            {[
              t(`system.overview.clients.connection.${client.connection}`),
              client.ip,
              client.host ? client.mac : null,
            ]
              .filter(Boolean)
              .join(" · ")}
          </div>
        </div>
      );
    },
  }),
];

const stationColumns: TableColumn<ConnectedClient>[] = [
  st.accessor((r) => r.host ?? "", {
    id: "host",
    header: () => t("system.overview.clients.col.name"),
    cell: (c) => (
      <div className="flex flex-col gap-1">
        <span>{c.getValue() || "—"}</span>
        <span className="text-xs text-muted-foreground">
          {t(`system.overview.clients.connection.${c.row.original.connection}`)}
        </span>
      </div>
    ),
  }),
  st.accessor((r) => r.ip ?? "", {
    id: "ip",
    header: () => t("system.overview.clients.col.ip"),
    cell: (c) => <span className="tabular-nums">{c.getValue() || "—"}</span>,
  }),
  st.accessor("mac", {
    header: "MAC",
    cell: (c) => <span className="tabular-nums">{c.getValue()}</span>,
  }),
  st.accessor("signal", {
    header: () => t("system.overview.clients.col.signal"),
    sortUndefined: "last",
    cell: (c) => (c.getValue() == null ? "—" : `${c.getValue()} dBm`),
  }),
  st.accessor((r) => r.tx ?? "", {
    id: "tx",
    header: () => t("system.overview.clients.col.tx"),
    enableSorting: false,
    cell: (c) => c.getValue() || "—",
  }),
  st.accessor((r) => r.rx ?? "", {
    id: "rx",
    header: () => t("system.overview.clients.col.rx"),
    enableSorting: false,
    cell: (c) => c.getValue() || "—",
  }),
  st.accessor("conn_min", {
    header: () => t("system.overview.clients.col.connected"),
    cell: (c) =>
      c.getValue() == null ? "—" : formatDuration((c.getValue() ?? 0) * 60),
  }),
];

const ls = columnHelper<Lease>();
const leaseColumns: TableColumn<Lease>[] = [
  ls.accessor((r) => r.host ?? "", {
    id: "host",
    header: () => t("system.overview.clients.col.name"),
    cell: (c) => c.getValue() || "—",
  }),
  ls.accessor("ip", {
    header: () => t("system.overview.clients.col.ip"),
    cell: (c) => <span className="tabular-nums">{c.getValue()}</span>,
  }),
  ls.accessor("mac", {
    header: "MAC",
    cell: (c) => <span className="tabular-nums">{c.getValue()}</span>,
  }),
  ls.accessor("left", {
    header: () => t("system.overview.clients.col.left"),
    cell: (c) =>
      c.getValue() > 0 ? (
        formatDuration(c.getValue())
      ) : (
        <Badge variant="outline">{t("system.overview.clients.expired")}</Badge>
      ),
  }),
];

/** Connected LAN devices, their limits and the DHCP leases (admin access). */
export function ClientsCard({
  compact = false,
  wifiClients,
}: {
  compact?: boolean;
  wifiClients?: WifiStation[];
}) {
  const query = useQuery({
    queryKey: systemKeys.clients,
    queryFn: getClients,
    refetchInterval: 15_000,
  });
  const [filter, setFilter] = useState("");
  const [editing, setEditing] = useState<ConnectedClient | null>(null);
  const limits = query.data?.limits;
  const columns = useMemo(() => {
    const base = compact ? compactColumns : stationColumns;
    if (!limits) return base;
    return [
      ...base,
      st.display({
        id: "limits",
        header: () => t("system.overview.clients.limits.title"),
        cell: ({ row: { original: client } }) => {
          const limit = limits[client.mac];
          const capped = !!limit?.cap_bytes;
          const exceeded = capped && limit.used_bytes >= limit.cap_bytes;
          const rate = (value?: number) =>
            value
              ? `${value / 1_000} Mbps`
              : t("system.overview.clients.limits.unlimited");
          return (
            <div className="flex items-center justify-between gap-3">
              <div className="flex flex-col items-start gap-1 whitespace-nowrap text-xs tabular-nums">
                <span>
                  {t("system.overview.clients.limits.cap")}{" "}
                  {capped
                    ? `${formatBytes(limit.used_bytes)} / ${formatBytes(limit.cap_bytes)}`
                    : t("system.overview.clients.limits.unlimited")}
                </span>
                <span>
                  ↑ {rate(limit?.upload_kbps)} · ↓ {rate(limit?.download_kbps)}
                </span>
                {exceeded && (
                  <Badge variant="outline">
                    {t("system.overview.clients.limits.reached")}
                  </Badge>
                )}
              </div>
              <Button
                size="xs"
                variant="outline"
                onClick={() => setEditing(client)}
              >
                {t("common.action.edit")}
              </Button>
            </div>
          );
        },
      }),
    ];
  }, [compact, limits]);
  const renderConnected = (data?: Clients) => {
    const connected =
      data?.connected ??
      data?.wifi.map((s) => ({ ...s, connection: "wifi" as const })) ??
      [];
    const devices = new Map(connected.map((client) => [client.mac, client]));
    // The dashboard can show hotspot stations immediately from its live status.
    for (const { mac, ip, name } of wifiClients ?? []) {
      const key = mac.toLowerCase();
      const known = devices.get(key);
      devices.set(key, {
        connection: "wifi",
        mac: key,
        ip: ip || known?.ip || null,
        host: name || known?.host || null,
        signal: known?.signal ?? null,
        tx: known?.tx ?? null,
        rx: known?.rx ?? null,
        conn_min: known?.conn_min ?? null,
      });
    }
    return (
      <div className="space-y-3">
        {data?.limits_active === false && (
          <Alert variant="destructive">
            <AlertDescription>
              {t("system.overview.clients.limits.unavailable")}
            </AlertDescription>
          </Alert>
        )}
        <DataTable
          data={[...devices.values()]}
          columns={columns}
          getRowId={(r) => r.mac}
          filter={filter}
          empty={t("system.overview.clients.emptyConnected")}
        />
      </div>
    );
  };
  // the list refreshes itself every 15 s; a manual refresh only while the page is not live
  const live = useLiveTopic("system");
  return (
    <Card size={compact ? "sm" : "default"}>
      <CardHeader>
        <CardTitle>{t("system.overview.clients.title")}</CardTitle>
        {!compact && (
          <CardDescription>
            {t("system.overview.clients.description")}
          </CardDescription>
        )}
        {!live && (
          <CardAction>
            <PendingButton
              size="sm"
              variant="outline"
              pending={query.isFetching}
              onClick={() => query.refetch()}
            >
              <RefreshCwIcon data-icon="inline-start" />
              {t("common.action.refresh")}
            </PendingButton>
          </CardAction>
        )}
      </CardHeader>
      <CardContent>
        {compact && wifiClients?.length ? (
          renderConnected(query.data)
        ) : (
          <QueryState query={query} skeletonRows={compact ? 1 : 4}>
            {(data) =>
              compact ? (
                renderConnected(data)
              ) : (
                <Tabs defaultValue="connected" className="flex flex-col gap-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <TabsList>
                      <TabsTrigger value="connected">
                        {t("system.overview.clients.connectedTab", {
                          n: data.connected?.length ?? data.wifi.length,
                        })}
                      </TabsTrigger>
                      <TabsTrigger value="leases">
                        {t("system.overview.clients.leasesTab", {
                          n: data.leases.length,
                        })}
                      </TabsTrigger>
                    </TabsList>
                    <InputGroup className="w-full sm:w-56">
                      <InputGroupInput
                        value={filter}
                        onChange={(e) => setFilter(e.target.value)}
                        placeholder={t("system.overview.clients.filterPlaceholder")}
                        aria-label={t("system.overview.clients.filterLabel")}
                      />
                      <InputGroupAddon>
                        <SearchIcon />
                      </InputGroupAddon>
                    </InputGroup>
                  </div>
                  <TabsContent value="connected">
                    {renderConnected(data)}
                  </TabsContent>
                  <TabsContent value="leases">
                    <DataTable
                      data={data.leases}
                      columns={leaseColumns}
                      getRowId={(r) => `${r.mac}-${r.ip}`}
                      filter={filter}
                      empty={t("system.overview.clients.emptyLeases")}
                    />
                  </TabsContent>
                </Tabs>
              )
            }
          </QueryState>
        )}
        {editing && (
          <ClientLimitDialog
            client={editing}
            limit={limits?.[editing.mac]}
            onClose={() => setEditing(null)}
            onSaved={() => void query.refetch()}
          />
        )}
      </CardContent>
    </Card>
  );
}

import { useState } from "react";
import { DataTable } from "@/components/f50/data-table";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Badge } from "@/components/ui/badge";
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
import { useLiveState } from "@/hooks/use-live";
import { t } from "@/i18n";
import { formatDuration } from "@/lib/format";
import { type TableColumn, columnHelper } from "@/lib/table";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon, SearchIcon } from "lucide-react";
import { getClients, systemKeys } from "../api";
import type { Lease, Station } from "../types";

const st = columnHelper<Station>();
const stationColumns: TableColumn<Station>[] = [
  st.accessor((r) => r.host ?? "", {
    id: "host",
    header: () => t("system.overview.clients.col.name"),
    cell: (c) => c.getValue() || "—",
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

/** Who is on the hotspot and who holds a DHCP lease (private: a write-grant method, read on demand). */
export function ClientsCard() {
  const query = useQuery({
    queryKey: systemKeys.clients,
    queryFn: getClients,
    refetchInterval: 15_000,
  });
  const [filter, setFilter] = useState("");
  // the list refreshes itself every 15 s; a manual refresh only while the page is not live
  const live = useLiveState() === "live";
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.overview.clients.title")}</CardTitle>
        <CardDescription>
          {t("system.overview.clients.description")}
        </CardDescription>
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
        <QueryState query={query} skeletonRows={4}>
          {(data) => (
            <Tabs defaultValue="wifi" className="flex flex-col gap-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <TabsList>
                  <TabsTrigger value="wifi">
                    {t("system.overview.clients.wifiTab", {
                      n: data.wifi.length,
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
              <TabsContent value="wifi">
                <DataTable
                  data={data.wifi}
                  columns={stationColumns}
                  getRowId={(r) => r.mac}
                  filter={filter}
                  empty={t("system.overview.clients.emptyWifi")}
                />
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
          )}
        </QueryState>
      </CardContent>
    </Card>
  );
}

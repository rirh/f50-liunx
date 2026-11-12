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
    header: "名称",
    cell: (c) => c.getValue() || "—",
  }),
  st.accessor((r) => r.ip ?? "", {
    id: "ip",
    header: "IP 地址",
    cell: (c) => <span className="font-mono">{c.getValue() || "—"}</span>,
  }),
  st.accessor("mac", {
    header: "MAC",
    cell: (c) => <span className="font-mono">{c.getValue()}</span>,
  }),
  st.accessor("signal", {
    header: "信号",
    sortUndefined: "last",
    cell: (c) => (c.getValue() == null ? "—" : `${c.getValue()} dBm`),
  }),
  st.accessor((r) => r.tx ?? "", {
    id: "tx",
    header: "发送速率",
    enableSorting: false,
    cell: (c) => c.getValue() || "—",
  }),
  st.accessor((r) => r.rx ?? "", {
    id: "rx",
    header: "接收速率",
    enableSorting: false,
    cell: (c) => c.getValue() || "—",
  }),
  st.accessor("conn_min", {
    header: "已连接",
    cell: (c) =>
      c.getValue() == null ? "—" : formatDuration((c.getValue() ?? 0) * 60),
  }),
];

const ls = columnHelper<Lease>();
const leaseColumns: TableColumn<Lease>[] = [
  ls.accessor((r) => r.host ?? "", {
    id: "host",
    header: "名称",
    cell: (c) => c.getValue() || "—",
  }),
  ls.accessor("ip", {
    header: "IP 地址",
    cell: (c) => <span className="font-mono">{c.getValue()}</span>,
  }),
  ls.accessor("mac", {
    header: "MAC",
    cell: (c) => <span className="font-mono">{c.getValue()}</span>,
  }),
  ls.accessor("left", {
    header: "剩余时间",
    cell: (c) =>
      c.getValue() > 0 ? (
        formatDuration(c.getValue())
      ) : (
        <Badge variant="outline">已过期</Badge>
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
  return (
    <Card>
      <CardHeader>
        <CardTitle>终端</CardTitle>
        <CardDescription>
          Wi-Fi 热点上的终端与 DHCP 租约（每 15 秒刷新）
        </CardDescription>
        <CardAction>
          <PendingButton
            size="sm"
            variant="outline"
            pending={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            刷新
          </PendingButton>
        </CardAction>
      </CardHeader>
      <CardContent>
        <QueryState query={query} skeletonRows={4}>
          {(data) => (
            <Tabs defaultValue="wifi" className="flex flex-col gap-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <TabsList>
                  <TabsTrigger value="wifi">
                    Wi-Fi 终端 ({data.wifi.length})
                  </TabsTrigger>
                  <TabsTrigger value="leases">
                    DHCP 租约 ({data.leases.length})
                  </TabsTrigger>
                </TabsList>
                <InputGroup className="w-full sm:w-56">
                  <InputGroupInput
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                    placeholder="按名称、IP、MAC 筛选"
                    aria-label="筛选终端"
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
                  empty="没有已连接的 Wi-Fi 终端"
                />
              </TabsContent>
              <TabsContent value="leases">
                <DataTable
                  data={data.leases}
                  columns={leaseColumns}
                  getRowId={(r) => `${r.mac}-${r.ip}`}
                  filter={filter}
                  empty="没有 DHCP 租约"
                />
              </TabsContent>
            </Tabs>
          )}
        </QueryState>
      </CardContent>
    </Card>
  );
}

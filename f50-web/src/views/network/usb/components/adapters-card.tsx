import { useMemo } from "react";
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
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { errorMessage } from "@/lib/format";
import { columnHelper } from "@/lib/table";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { toast } from "sonner";
import { addAdapter, getAdapters, usbKeys } from "../api";
import type { UsbAdapter } from "../types";

const col = columnHelper<UsbAdapter>();

/** USB network adapters in the host role, and adding one to the LAN bridge. */
export function UsbAdaptersCard({ hostRole }: { hostRole: boolean }) {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: usbKeys.adapters,
    queryFn: getAdapters,
    enabled: hostRole,
    refetchInterval: 10_000,
  });
  const add = useMutation({
    mutationFn: addAdapter,
    onSuccess: (r) => {
      toast.success(`${r.added} 已加入局域网`);
      qc.invalidateQueries({ queryKey: usbKeys.adapters });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const adding = add.isPending ? add.variables : null;

  const columns = useMemo(
    () => [
      col.accessor("name", {
        header: "接口",
        cell: (c) => <span className="font-mono">{c.getValue()}</span>,
      }),
      col.accessor("carrier", {
        header: "网线",
        cell: (c) => (
          <Badge variant={c.getValue() ? "default" : "secondary"}>
            {c.getValue() ? "已连接" : "未连接"}
          </Badge>
        ),
      }),
      col.accessor("in_lan", {
        header: "局域网",
        cell: (c) =>
          c.getValue() ? (
            <Badge>已加入</Badge>
          ) : c.row.original.eligible ? (
            <Badge variant="outline">未加入</Badge>
          ) : (
            <Badge
              variant="secondary"
              title={c.row.original.owner ?? undefined}
            >
              {c.row.original.owner === "unknown"
                ? "归属未知"
                : `属于 ${c.row.original.owner}`}
            </Badge>
          ),
      }),
      col.display({
        id: "actions",
        header: () => <span className="sr-only">操作</span>,
        cell: (c) => {
          const a = c.row.original;
          if (a.in_lan || !a.eligible) return null;
          return (
            <PendingButton
              size="xs"
              variant="outline"
              pending={adding === a.name}
              disabled={adding !== null}
              onClick={() => add.mutate(a.name)}
            >
              {adding === a.name ? "加入中" : "加入局域网"}
            </PendingButton>
          );
        },
      }),
    ],
    [adding, add],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>USB 网卡</CardTitle>
        <CardDescription>
          主机模式下接入的 USB 有线网卡，可桥接到局域网。
        </CardDescription>
        {hostRole && (
          <CardAction>
            <PendingButton
              size="sm"
              variant="outline"
              pending={query.isFetching}
              onClick={() => query.refetch()}
            >
              <RefreshCwIcon data-icon="inline-start" />
              {query.isFetching ? "刷新中" : "刷新"}
            </PendingButton>
          </CardAction>
        )}
      </CardHeader>
      <CardContent>
        {hostRole ? (
          <QueryState query={query}>
            {(r) => (
              <DataTable
                columns={columns}
                data={r.devices}
                getRowId={(a) => a.name}
                empty="没有发现 USB 网卡（确认 Hub 已供电）"
              />
            )}
          </QueryState>
        ) : (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>仅主机模式可用</EmptyTitle>
              <EmptyDescription>
                将端口切换到主机模式后，这里会列出接入的 USB 网卡。
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </CardContent>
    </Card>
  );
}

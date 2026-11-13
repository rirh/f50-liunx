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
import { t } from "@/i18n";
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
      toast.success(t("network.usb.adapters.added", { name: r.added }));
      qc.invalidateQueries({ queryKey: usbKeys.adapters });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const adding = add.isPending ? add.variables : null;

  const columns = useMemo(
    () => [
      col.accessor("name", {
        header: t("network.usb.adapters.name"),
        cell: (c) => <span className="tabular-nums">{c.getValue()}</span>,
      }),
      col.accessor("carrier", {
        header: t("network.usb.adapters.carrier"),
        cell: (c) => (
          <Badge variant={c.getValue() ? "default" : "secondary"}>
            {c.getValue()
              ? t("common.state.connected")
              : t("common.state.disconnected")}
          </Badge>
        ),
      }),
      col.accessor("in_lan", {
        header: t("network.usb.adapters.lan"),
        cell: (c) =>
          c.getValue() ? (
            <Badge>{t("network.usb.adapters.inLan")}</Badge>
          ) : c.row.original.eligible ? (
            <Badge variant="outline">
              {t("network.usb.adapters.notInLan")}
            </Badge>
          ) : (
            <Badge
              variant="secondary"
              title={c.row.original.owner ?? undefined}
            >
              {c.row.original.owner === "unknown"
                ? t("network.usb.adapters.ownerUnknown")
                : t("network.usb.adapters.owner", {
                    owner: c.row.original.owner,
                  })}
            </Badge>
          ),
      }),
      col.display({
        id: "actions",
        header: () => (
          <span className="sr-only">{t("network.shared.actions")}</span>
        ),
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
              {adding === a.name
                ? t("network.usb.adapters.adding")
                : t("network.usb.adapters.add")}
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
        <CardTitle>{t("network.usb.adapters.title")}</CardTitle>
        <CardDescription>
          {t("network.usb.adapters.description")}
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
              {query.isFetching
                ? t("common.action.refreshing")
                : t("common.action.refresh")}
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
                empty={t("network.usb.adapters.empty")}
              />
            )}
          </QueryState>
        ) : (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>
                {t("network.usb.adapters.hostOnly.title")}
              </EmptyTitle>
              <EmptyDescription>
                {t("network.usb.adapters.hostOnly.description")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </CardContent>
    </Card>
  );
}

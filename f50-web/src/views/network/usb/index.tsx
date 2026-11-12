import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { getUsb, usbKeys } from "./api";
import { UsbAdaptersCard } from "./components/adapters-card";
import { UsbAdbCard } from "./components/adb-card";
import { UsbNetCard } from "./components/net-card";
import { UsbRoleCard } from "./components/role-card";
import { UsbStatusCard } from "./components/status-card";

export default function UsbPage() {
  const query = useQuery({ queryKey: usbKeys.info, queryFn: getUsb });
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="USB"
        description="USB-C 端口角色、USB 网络、USB 网卡与 adb"
        actions={
          <PendingButton
            variant="outline"
            size="sm"
            pending={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {query.isFetching ? "刷新中" : "刷新"}
          </PendingButton>
        }
      />
      <QueryState query={query} skeletonRows={8}>
        {(info) => (
          <div className="grid gap-4 lg:grid-cols-2">
            <UsbStatusCard info={info} />
            <UsbRoleCard key={`${info.role}|${info.role_boot}`} info={info} />
            <UsbNetCard
              key={`${info.net_mode}|${info.net_scope}|${info.net_name}`}
              info={info}
            />
            <UsbAdbCard adb={info.adb} hostRole={info.role === "host"} />
            <div className="lg:col-span-2">
              <UsbAdaptersCard hostRole={info.role === "host"} />
            </div>
          </div>
        )}
      </QueryState>
    </div>
  );
}

import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { getInfo, systemKeys } from "./api";
import { ClientsCard } from "./components/clients-card";
import { InfoCard } from "./components/info-card";
import { LiveCard } from "./components/live-card";

export default function Page() {
  const info = useQuery({ queryKey: systemKeys.info, queryFn: getInfo });
  return (
    <>
      <PageHeader
        title="系统信息"
        description="设备信息、实时负载与已连接的终端"
        actions={
          <PendingButton
            size="sm"
            variant="outline"
            pending={info.isFetching}
            onClick={() => info.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            刷新信息
          </PendingButton>
        }
      />
      <div className="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
        <InfoCard query={info} />
        <LiveCard />
      </div>
      <ClientsCard />
    </>
  );
}

import { PageHeader } from "@/components/f50/page-header";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";
import { errorMessage } from "@/lib/format";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { STATUS_KEY, getStatus } from "./api";
import { CellularTile } from "./components/cellular-tile";
import { DataTile } from "./components/data-tile";
import { SmsTile } from "./components/sms-tile";
import { SystemTile } from "./components/system-tile";
import { TileSkeleton } from "./components/tile";
import { TrafficTile } from "./components/traffic-tile";
import { UpdateTile } from "./components/update-tile";
import { UplinkTile } from "./components/uplink-tile";
import { UsbTile } from "./components/usb-tile";
import { WifiTile } from "./components/wifi-tile";

const POLL_MS = 3000;

export default function DashboardPage() {
  const query = useQuery({
    queryKey: STATUS_KEY,
    queryFn: getStatus,
    refetchInterval: POLL_MS,
  });
  const s = query.data;
  const refreshing = query.isFetching && !query.isPending;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="仪表盘"
        description="设备状态一览，每 3 秒自动刷新"
        actions={
          <Button
            variant="outline"
            size="sm"
            disabled={query.isFetching}
            aria-busy={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {refreshing ? <SweepShine>刷新中</SweepShine> : "刷新"}
          </Button>
        }
      />
      {query.isError && (
        <Alert variant="destructive">
          <AlertTitle>
            {s ? "刷新失败，显示的是上次的数据" : "读取失败"}
          </AlertTitle>
          <AlertDescription>{errorMessage(query.error)}</AlertDescription>
        </Alert>
      )}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
        {s ? (
          <>
            <CellularTile modem={s.modem} sim={s.sim} />
            <DataTile part={s.data} />
            <TrafficTile part={s.traffic} />
            <WifiTile part={s.wifi} />
            <UplinkTile part={s.wificlient} />
            <UsbTile part={s.usb} />
            <SystemTile part={s.system} />
            <SmsTile part={s.sms} />
            <UpdateTile part={s.update} />
          </>
        ) : (
          query.isPending &&
          Array.from({ length: 9 }, (_, i) => <TileSkeleton key={i} />)
        )}
      </div>
    </div>
  );
}

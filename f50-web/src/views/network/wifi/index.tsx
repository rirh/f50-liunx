import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { getWifi, wifiKeys } from "./api";
import { WifiClientsTable } from "./components/clients-table";
import { WifiSettingsForm } from "./components/settings-form";
import { WifiStatusCard } from "./components/status-card";

export default function WifiPage() {
  const query = useQuery({ queryKey: wifiKeys.info, queryFn: getWifi });
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Wi-Fi 热点"
        description="热点名称、密码、频段与信道"
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
      <Alert>
        <AlertTitle>应用设置会重启 Wi-Fi</AlertTitle>
        <AlertDescription>
          保存后 Wi-Fi 约 2
          秒后重新加载，所有热点客户端会断开；若当前浏览器通过热点连接，请准备用新名称或密码重新连接。
        </AlertDescription>
      </Alert>
      <QueryState query={query} skeletonRows={8}>
        {(info) => (
          <div className="grid gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
            <div className="flex min-w-0 flex-col gap-4">
              <WifiStatusCard info={info} />
              <WifiClientsTable clients={info.clients} />
            </div>
            <WifiSettingsForm
              key={JSON.stringify([
                info.ssid,
                info.band,
                info.channel,
                info.htmode,
                info.country,
                info.hidden,
                info.password_set,
                info.security,
              ])}
              info={info}
            />
          </div>
        )}
      </QueryState>
    </div>
  );
}

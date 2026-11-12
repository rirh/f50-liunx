import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { t } from "@/i18n";
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
        title={t("network.wifi.title")}
        description={t("network.wifi.description")}
        actions={
          <PendingButton
            variant="outline"
            size="sm"
            pending={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {query.isFetching
              ? t("common.action.refreshing")
              : t("common.action.refresh")}
          </PendingButton>
        }
      />
      <Alert>
        <AlertTitle>{t("network.wifi.restartAlert.title")}</AlertTitle>
        <AlertDescription>
          {t("network.wifi.restartAlert.description")}
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

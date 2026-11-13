import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { getWifi, getWifiCredentials, wifiKeys } from "./api";
import { WifiClientsTable } from "./components/clients-table";
import { WifiStatusCard } from "./components/status-card";

/**
 * The hotspot as it is on the left, where the page is read first, and who is on it beside it. Its settings open from
 * the status card in a dialog (with the notice that saving restarts the radio), so they take no room here.
 */
export default function WifiPage() {
  const query = useQuery({ queryKey: wifiKeys.info, queryFn: getWifi });
  const credentials = useQuery({
    queryKey: wifiKeys.credentials,
    queryFn: getWifiCredentials,
    retry: false,
  });
  const fetching = query.isFetching || credentials.isFetching;
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("network.wifi.title")}
        description={t("network.wifi.description")}
        actions={
          <PendingButton
            variant="outline"
            size="sm"
            pending={fetching}
            onClick={() => {
              query.refetch();
              credentials.refetch();
            }}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {fetching
              ? t("common.action.refreshing")
              : t("common.action.refresh")}
          </PendingButton>
        }
      />
      <QueryState query={query} skeletonRows={8}>
        {(info) => (
          <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
            <WifiStatusCard info={info} credentials={credentials} />
            <WifiClientsTable clients={info.clients} />
          </div>
        )}
      </QueryState>
    </div>
  );
}

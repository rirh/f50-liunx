import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { getTraffic, trafficKeys } from "./api";
import { CyclesTable } from "./components/cycles-table";
import { SettingsDialog } from "./components/settings-card";
import { SummaryCard } from "./components/summary-card";
import { UsageChart } from "./components/usage-chart";

/**
 * This cycle's figures beside the daily usage that explains them, then the past cycles. The plan's settings (set once,
 * changed rarely) open from the header in a dialog.
 */
export default function TrafficPage() {
  const query = useQuery({
    queryKey: trafficKeys.get,
    queryFn: getTraffic,
    refetchInterval: 60_000,
  });
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("cellular.traffic.title")}
        description={t("cellular.traffic.description")}
        actions={query.data && <SettingsDialog t={query.data} />}
      />
      <QueryState query={query} skeletonRows={10}>
        {(tr) => (
          <div className="flex flex-col gap-4">
            <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
              <SummaryCard t={tr} />
              <UsageChart days={tr.days} />
            </div>
            <CyclesTable cycles={tr.cycles} />
          </div>
        )}
      </QueryState>
    </div>
  );
}

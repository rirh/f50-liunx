import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { getTraffic, trafficKeys } from "./api";
import { CyclesTable } from "./components/cycles-table";
import { SettingsCard } from "./components/settings-card";
import { SummaryCard } from "./components/summary-card";
import { UsageChart } from "./components/usage-chart";

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
      />
      <QueryState query={query} skeletonRows={10}>
        {(tr) => (
          <div className="flex flex-col gap-4">
            <div className="grid items-start gap-4 lg:grid-cols-2">
              <SummaryCard t={tr} />
              <SettingsCard
                key={`${tr.reset_day}|${tr.cap.bytes}|${tr.cap.warn}|${tr.cap.cut}|${tr.plan}`}
                t={tr}
              />
            </div>
            <UsageChart days={tr.days} />
            <CyclesTable cycles={tr.cycles} />
          </div>
        )}
      </QueryState>
    </div>
  );
}

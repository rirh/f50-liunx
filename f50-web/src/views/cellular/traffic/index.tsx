import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
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
        title="流量统计"
        description="蜂窝数据按日与按计费周期的用量（每分钟统计一次）"
      />
      <QueryState query={query} skeletonRows={10}>
        {(t) => (
          <div className="flex flex-col gap-4">
            <div className="grid items-start gap-4 lg:grid-cols-2">
              <SummaryCard t={t} />
              <SettingsCard
                key={`${t.reset_day}|${t.cap.bytes}|${t.cap.warn}|${t.cap.cut}|${t.plan}`}
                t={t}
              />
            </div>
            <UsageChart days={t.days} />
            <CyclesTable cycles={t.cycles} />
          </div>
        )}
      </QueryState>
    </div>
  );
}

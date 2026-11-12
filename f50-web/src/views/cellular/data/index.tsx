import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { useQuery } from "@tanstack/react-query";
import { dataKeys, getData } from "./api";
import { SettingsCard } from "./components/settings-card";
import { StatusCard } from "./components/status-card";

export default function DataPage() {
  const query = useQuery({
    queryKey: dataKeys.get,
    queryFn: getData,
    refetchInterval: 5000,
  });
  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="移动数据" description="蜂窝数据连接、APN 与数据漫游" />
      <QueryState query={query} skeletonRows={8}>
        {(d) => (
          <div className="grid items-start gap-4 lg:grid-cols-2">
            <StatusCard d={d} />
            <SettingsCard
              key={`${d.slot}|${d.apn}|${d.auth}|${d.username}|${d.pdp}|${d.roaming}|${d.password_set}`}
              d={d}
            />
          </div>
        )}
      </QueryState>
    </div>
  );
}

import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
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
      <PageHeader
        title={t("cellular.data.title")}
        description={t("cellular.data.description")}
      />
      <QueryState query={query} skeletonRows={8}>
        {(d) => (
          <div className="grid items-start gap-4 lg:grid-cols-2">
            <StatusCard d={d} />
            <SettingsCard d={d} />
          </div>
        )}
      </QueryState>
    </div>
  );
}

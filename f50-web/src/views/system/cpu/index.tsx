import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { cpuKey, getCpu } from "./api";
import { PoliciesCard } from "./components/policies-card";
import { ProfileCard } from "./components/profile-card";
import { ThermalCard } from "./components/thermal-card";

export default function Page() {
  const query = useQuery({
    queryKey: cpuKey,
    queryFn: getCpu,
    refetchInterval: 5000,
  });
  return (
    <>
      <PageHeader title="CPU" description={t("system.cpu.description")} />
      <QueryState query={query} skeletonRows={8}>
        {(data) => (
          <div className="flex flex-col gap-6">
            <ProfileCard data={data} />
            <PoliciesCard policies={data.policies} />
            <ThermalCard zones={data.thermal} />
          </div>
        )}
      </QueryState>
    </>
  );
}

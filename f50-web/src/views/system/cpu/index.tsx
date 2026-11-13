import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { cpuKey, getCpu } from "./api";
import { PoliciesCard } from "./components/policies-card";
import { ProfileCard } from "./components/profile-card";
import { ThermalCard } from "./components/thermal-card";

/**
 * Three questions, in the order they are asked: which mode (pick one of three), how fast each group of cores runs,
 * and whether it is too hot. Kernel detail (trip points, cooling devices) is folded under the temperatures.
 */
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
            <div className="grid items-start gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
              <PoliciesCard policies={data.policies} />
              <ThermalCard zones={data.thermal} throttling={data.throttling} />
            </div>
          </div>
        )}
      </QueryState>
    </>
  );
}

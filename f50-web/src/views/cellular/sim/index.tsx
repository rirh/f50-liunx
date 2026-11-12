import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { useQuery } from "@tanstack/react-query";
import { getSim, simKeys } from "./api";
import { CardRadioCard } from "./components/radio-card";
import { SlotCard } from "./components/slot-card";

export default function SimPage() {
  const query = useQuery({
    queryKey: simKeys.get,
    queryFn: getSim,
    refetchInterval: 10_000,
  });
  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="SIM 卡" description="卡槽选择与每张卡的射频设置" />
      <QueryState query={query} skeletonRows={8}>
        {(sim) => (
          <div className="grid gap-4 lg:grid-cols-2">
            <SlotCard key={sim.default} sim={sim} />
            <CardRadioCard sim={sim} />
          </div>
        )}
      </QueryState>
    </div>
  );
}

import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useQuery } from "@tanstack/react-query";
import { getLeds, ledKey } from "./api";
import { BehaviourCard } from "./components/behaviour-card";
import { LampRow } from "./components/lamp-row";
import { TimeoutCard } from "./components/timeout-card";
import type { LedSettings } from "./types";

function Lamps({ data }: { data: LedSettings }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>指示灯</CardTitle>
        <CardDescription>开关与亮度（拖动亮度滑块松开后保存）</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4 md:grid-cols-2">
        {data.lamps.map((lamp) => (
          <LampRow
            key={lamp}
            lamp={lamp}
            on={data[lamp]}
            brightness={
              lamp === "network"
                ? data.network_brightness
                : data.wifi_brightness
            }
            dimmable={data.dimmable.includes(lamp)}
          />
        ))}
      </CardContent>
    </Card>
  );
}

export default function Page() {
  const query = useQuery({ queryKey: ledKey, queryFn: getLeds });
  return (
    <>
      <PageHeader
        title="指示灯"
        description="指示灯开关、亮度、自动熄灭与告警效果，修改立即生效"
      />
      <QueryState query={query} skeletonRows={6}>
        {(data) => (
          <div className="flex flex-col gap-6">
            <Lamps data={data} />
            <div className="grid gap-6 lg:grid-cols-2">
              <TimeoutCard data={data} />
              <BehaviourCard data={data} />
            </div>
          </div>
        )}
      </QueryState>
    </>
  );
}

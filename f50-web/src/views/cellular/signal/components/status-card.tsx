import { KvList } from "@/components/f50/kv-list";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useQuery } from "@tanstack/react-query";
import { SignalBars } from "./signal-bars";
import { formatAgeMs, unit } from "../../shared/format";
import { getStatus, signalKeys } from "../api";
import { CFUN_LABEL, RAT_LABEL, SIM_STATE_LABEL } from "../labels";
import type { ModemStatus } from "../types";

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-lg bg-muted/50 p-2">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="truncate font-medium tabular-nums">{value}</span>
    </div>
  );
}

function StatusBody({ s }: { s: ModemStatus }) {
  const sig = s.signal;
  return (
    <div className="flex flex-col gap-4">
      {s.error && (
        <Alert variant="destructive">
          <AlertTitle>调制解调器未应答</AlertTitle>
          <AlertDescription>{s.error}</AlertDescription>
        </Alert>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant={s.registered ? "default" : "destructive"}>
          {s.registered ? "已注册" : "未注册"}
        </Badge>
        {s.rat && <Badge variant="secondary">{RAT_LABEL[s.rat]}</Badge>}
        {s.roaming && <Badge variant="outline">漫游</Badge>}
        {s.restarting && <Badge variant="outline">射频重启中</Badge>}
        {!s.radio_allowed && <Badge variant="destructive">射频已关闭</Badge>}
        <span className="ms-auto flex items-center gap-2 text-sm text-muted-foreground">
          <SignalBars bars={s.bars} />
          {s.bars}/5
        </span>
      </div>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Metric label="RSRP" value={unit(sig?.rsrp, "dBm")} />
        <Metric label="RSRQ" value={unit(sig?.rsrq, "dB")} />
        <Metric label="SINR" value={unit(sig?.sinr, "dB")} />
        <Metric label="RSSI" value={unit(sig?.rssi, "dBm")} />
      </div>
      <KvList
        items={[
          { label: "运营商", value: s.operator ?? "—" },
          { label: "SIM 卡槽", value: `SIM${s.slot}` },
          {
            label: "SIM 状态",
            value: s.sim_state ? SIM_STATE_LABEL[s.sim_state] : "未检测",
          },
          {
            label: "射频 (CFUN)",
            value:
              s.cfun != null ? (CFUN_LABEL[s.cfun] ?? String(s.cfun)) : "—",
          },
        ]}
      />
    </div>
  );
}

export function StatusCard() {
  const query = useQuery({
    queryKey: signalKeys.status,
    queryFn: getStatus,
    refetchInterval: 2000,
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>网络状态</CardTitle>
        <CardDescription>
          每 2 秒刷新 · 读数 {formatAgeMs(query.data?.age_ms)}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState query={query} skeletonRows={6}>
          {(s) => <StatusBody s={s} />}
        </QueryState>
      </CardContent>
    </Card>
  );
}

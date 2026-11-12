import { AnimatedNumber } from "@/components/f50/animated-number";
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
import { usePoll } from "@/hooks/use-live";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { SignalBars } from "./signal-bars";
import { formatAgeMs } from "../../shared/format";
import { getStatus, signalKeys } from "../api";
import { CFUN_LABEL, RAT_LABEL, SIM_STATE_LABEL } from "../labels";
import type { ModemStatus } from "../types";

/** One reading, easing to each new value (dBm and dB keep one decimal). */
function Metric({
  label,
  value,
  u,
}: {
  label: string;
  value: number | null | undefined;
  u: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-lg bg-muted/50 p-2">
      <span className="text-xs text-muted-foreground">{label}</span>
      <AnimatedNumber
        value={value}
        format={(n) => (n == null ? "—" : `${n.toFixed(1)} ${u}`)}
        className="truncate font-medium"
      />
    </div>
  );
}

function cfunText(cfun: number) {
  const k = CFUN_LABEL[cfun];
  return k ? t(k) : String(cfun);
}

function StatusBody({ s }: { s: ModemStatus }) {
  const sig = s.signal;
  return (
    <div className="flex flex-col gap-4">
      {s.error && (
        <Alert variant="destructive">
          <AlertTitle>{t("cellular.signal.status.noAnswer")}</AlertTitle>
          <AlertDescription>{s.error}</AlertDescription>
        </Alert>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant={s.registered ? "default" : "destructive"}>
          {s.registered
            ? t("cellular.signal.status.registered")
            : t("cellular.signal.status.unregistered")}
        </Badge>
        {s.rat && <Badge variant="secondary">{RAT_LABEL[s.rat]}</Badge>}
        {s.roaming && (
          <Badge variant="outline">{t("cellular.signal.status.roaming")}</Badge>
        )}
        {s.restarting && (
          <Badge variant="outline">
            {t("cellular.signal.status.restarting")}
          </Badge>
        )}
        {!s.radio_allowed && (
          <Badge variant="destructive">
            {t("cellular.signal.status.radioOff")}
          </Badge>
        )}
        <span className="ms-auto flex items-center gap-2 text-sm text-muted-foreground">
          <SignalBars bars={s.bars} />
          {s.bars}/5
        </span>
      </div>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Metric label="RSRP" value={sig?.rsrp} u="dBm" />
        <Metric label="RSRQ" value={sig?.rsrq} u="dB" />
        <Metric label="SINR" value={sig?.sinr} u="dB" />
        <Metric label="RSSI" value={sig?.rssi} u="dBm" />
      </div>
      <KvList
        items={[
          { label: t("cellular.shared.operator"), value: s.operator ?? "—" },
          { label: t("cellular.shared.simSlot"), value: `SIM${s.slot}` },
          {
            label: t("cellular.signal.status.simState"),
            value: s.sim_state
              ? t(SIM_STATE_LABEL[s.sim_state])
              : t("cellular.shared.notDetected"),
          },
          {
            label: t("cellular.signal.status.cfun"),
            value: s.cfun != null ? cfunText(s.cfun) : "—",
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
    refetchInterval: usePoll(2000, "modem"),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.signal.status.title")}</CardTitle>
        <CardDescription>
          {t("cellular.signal.status.description", {
            age: formatAgeMs(query.data?.age_ms),
          })}
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

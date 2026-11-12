import { KvList } from "@/components/f50/kv-list";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { t, type Key } from "@/i18n";
import { formatBytes, formatTime } from "@/lib/format";
import type { CapLevel, TrafficStatus } from "../types";

const LEVEL: Record<
  CapLevel,
  { label: Key; variant: "secondary" | "outline" | "destructive" }
> = {
  none: { label: "cellular.traffic.level.none", variant: "outline" },
  ok: { label: "cellular.traffic.level.ok", variant: "secondary" },
  warn: { label: "cellular.traffic.level.warn", variant: "destructive" },
  over: { label: "cellular.traffic.level.over", variant: "destructive" },
};

function Stat({
  label,
  value,
  sub,
}: {
  label: string;
  value: string;
  sub?: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-lg bg-muted/50 p-3">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-lg leading-tight font-semibold break-words tabular-nums">
        {value}
      </span>
      {sub && (
        <span className="text-xs break-words text-muted-foreground tabular-nums">
          {sub}
        </span>
      )}
    </div>
  );
}

export function SummaryCard({ t: tr }: { t: TrafficStatus }) {
  const { cycle, cap, today } = tr;
  const pct =
    cap.bytes > 0 ? Math.min(100, (cycle.used / cap.bytes) * 100) : null;
  const level = LEVEL[cap.level];
  return (
    <Card>
      <CardHeader>
        <CardTitle>{tr.plan || t("cellular.traffic.summary.title")}</CardTitle>
        <CardDescription>
          {t("cellular.traffic.summary.description", {
            start: cycle.start,
            last: cycle.last,
            count: cycle.days_left,
          })}
        </CardDescription>
        <CardAction>
          <Badge variant={level.variant}>{t(level.label)}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {cap.cut_active && (
          <Alert variant="destructive">
            <AlertTitle>{t("cellular.traffic.summary.cutTitle")}</AlertTitle>
            <AlertDescription>
              {t("cellular.traffic.summary.cutDescription")}
            </AlertDescription>
          </Alert>
        )}
        {!tr.clock && (
          <Alert>
            <AlertTitle>{t("cellular.traffic.summary.clockTitle")}</AlertTitle>
            <AlertDescription>
              {t("cellular.traffic.summary.clockDescription", {
                bytes: formatBytes(tr.pending.rx + tr.pending.tx),
              })}
            </AlertDescription>
          </Alert>
        )}
        <div className="grid grid-cols-[repeat(auto-fill,minmax(8rem,1fr))] gap-2">
          <Stat
            label={t("cellular.traffic.summary.used")}
            value={formatBytes(cycle.used)}
            sub={
              cap.bytes > 0
                ? t("cellular.traffic.summary.cap", {
                    bytes: formatBytes(cap.bytes),
                  })
                : undefined
            }
          />
          <Stat
            label={t("cellular.traffic.summary.today")}
            value={formatBytes(today.rx + today.tx)}
            sub={`↓ ${formatBytes(today.rx)} ↑ ${formatBytes(today.tx)}`}
          />
          <Stat
            label={t("cellular.traffic.summary.cycleDownload")}
            value={formatBytes(cycle.rx)}
            sub={t("cellular.traffic.summary.uploadOf", {
              bytes: formatBytes(cycle.tx),
            })}
          />
        </div>
        {pct != null && (
          <div className="flex flex-col gap-1.5">
            <Progress
              value={pct}
              aria-label={t("cellular.traffic.summary.progress")}
            />
            <span className="text-xs text-muted-foreground tabular-nums">
              {t("cellular.traffic.summary.warnAt", {
                pct: pct.toFixed(1),
                warn: cap.warn,
              })}
              {cap.cut ? t("cellular.traffic.summary.cutAt") : ""}
            </span>
          </div>
        )}
        <KvList
          items={[
            {
              label: t("cellular.traffic.summary.device"),
              value: `${tr.device}${tr.available ? "" : t("cellular.traffic.summary.offline")}`,
            },
            {
              label: t("cellular.traffic.summary.since"),
              value: tr.since ? formatTime(tr.since) : "—",
            },
            {
              label: t("cellular.traffic.summary.resetDay"),
              value: t("cellular.traffic.summary.resetDayValue", {
                n: tr.reset_day,
              }),
            },
          ]}
        />
      </CardContent>
    </Card>
  );
}

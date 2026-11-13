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

/** One row of the breakdown: download, upload and their sum, right-aligned in monospace so the columns line up. */
function Line({ label, rx, tx }: { label: string; rx: number; tx: number }) {
  return (
    <>
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right font-mono">{formatBytes(rx)}</span>
      <span className="text-right font-mono">{formatBytes(tx)}</span>
      <span className="text-right font-mono font-medium">
        {formatBytes(rx + tx)}
      </span>
    </>
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
        <div className="flex items-baseline justify-between gap-3">
          <span className="text-sm text-muted-foreground">
            {t("cellular.traffic.summary.used")}
          </span>
          <span className="min-w-0 truncate">
            <span className="font-mono text-2xl font-semibold tracking-tight">
              {formatBytes(cycle.used)}
            </span>
            <span className="ms-1.5 text-sm text-muted-foreground">
              /{" "}
              {cap.bytes > 0
                ? formatBytes(cap.bytes)
                : t("cellular.traffic.level.none")}
            </span>
          </span>
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
        <div className="grid grid-cols-[auto_1fr_1fr_1fr] gap-x-4 gap-y-2 rounded-lg bg-muted/40 px-3 py-2.5 text-[13px] tabular-nums">
          <span />
          <span className="text-right text-xs text-muted-foreground">
            ↓ {t("cellular.traffic.download")}
          </span>
          <span className="text-right text-xs text-muted-foreground">
            ↑ {t("cellular.traffic.upload")}
          </span>
          <span className="text-right text-xs text-muted-foreground">
            {t("cellular.traffic.summary.total")}
          </span>
          <Line
            label={t("cellular.traffic.summary.today")}
            rx={today.rx}
            tx={today.tx}
          />
          <Line
            label={t("cellular.traffic.summary.cycle")}
            rx={cycle.rx}
            tx={cycle.tx}
          />
        </div>
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

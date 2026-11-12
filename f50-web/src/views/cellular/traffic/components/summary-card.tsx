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
import { formatBytes, formatTime } from "@/lib/format";
import type { CapLevel, TrafficStatus } from "../types";

const LEVEL: Record<
  CapLevel,
  { label: string; variant: "secondary" | "outline" | "destructive" }
> = {
  none: { label: "不限量", variant: "outline" },
  ok: { label: "正常", variant: "secondary" },
  warn: { label: "接近上限", variant: "destructive" },
  over: { label: "已超额", variant: "destructive" },
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
      <span className="truncate text-lg font-semibold tabular-nums">
        {value}
      </span>
      {sub && (
        <span className="truncate text-xs text-muted-foreground">{sub}</span>
      )}
    </div>
  );
}

export function SummaryCard({ t }: { t: TrafficStatus }) {
  const { cycle, cap, today } = t;
  const pct =
    cap.bytes > 0 ? Math.min(100, (cycle.used / cap.bytes) * 100) : null;
  const level = LEVEL[cap.level];
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t.plan || "本期用量"}</CardTitle>
        <CardDescription>
          计费周期 {cycle.start} 至 {cycle.last} · 剩余 {cycle.days_left} 天
        </CardDescription>
        <CardAction>
          <Badge variant={level.variant}>{level.label}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {cap.cut_active && (
          <Alert variant="destructive">
            <AlertTitle>移动数据已被限额切断</AlertTitle>
            <AlertDescription>
              本周期用量已达上限；可在「移动数据」页手动重新开启，或调整上限。
            </AlertDescription>
          </Alert>
        )}
        {!t.clock && (
          <Alert>
            <AlertTitle>时钟未同步</AlertTitle>
            <AlertDescription>
              同步前的流量暂记为待分配（
              {formatBytes(t.pending.rx + t.pending.tx)}），计入本周期。
            </AlertDescription>
          </Alert>
        )}
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          <Stat
            label="本期已用"
            value={formatBytes(cycle.used)}
            sub={cap.bytes > 0 ? `上限 ${formatBytes(cap.bytes)}` : undefined}
          />
          <Stat
            label="今日"
            value={formatBytes(today.rx + today.tx)}
            sub={`↓ ${formatBytes(today.rx)} ↑ ${formatBytes(today.tx)}`}
          />
          <Stat
            label="本期下载 / 上传"
            value={formatBytes(cycle.rx)}
            sub={`上传 ${formatBytes(cycle.tx)}`}
          />
        </div>
        {pct != null && (
          <div className="flex flex-col gap-1.5">
            <Progress value={pct} aria-label="本期用量占上限" />
            <span className="text-xs text-muted-foreground tabular-nums">
              {pct.toFixed(1)}% · {cap.warn}% 时提醒
              {cap.cut ? " · 到达上限断网" : ""}
            </span>
          </div>
        )}
        <KvList
          items={[
            {
              label: "统计设备",
              value: `${t.device}${t.available ? "" : "（当前不在线）"}`,
            },
            { label: "开始统计", value: t.since ? formatTime(t.since) : "—" },
            { label: "每月重置日", value: `${t.reset_day} 日` },
          ]}
        />
      </CardContent>
    </Card>
  );
}

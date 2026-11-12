import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { formatBytes } from "@/lib/format";
import { ArrowUpDownIcon } from "lucide-react";
import { Figure, Tile } from "./tile";
import type { Part, TrafficView } from "../types";

const LEVEL: Record<
  string,
  { label: string; variant: "secondary" | "outline" | "destructive" }
> = {
  ok: { label: "正常", variant: "secondary" },
  warn: { label: "接近上限", variant: "outline" },
  over: { label: "已超出", variant: "destructive" },
};

export function TrafficTile({ part }: { part: Part<TrafficView> | undefined }) {
  return (
    <Tile
      title="流量统计"
      icon={ArrowUpDownIcon}
      to="/cellular/traffic"
      part={part}
    >
      {(t) => {
        const today = t.today.rx + t.today.tx;
        const capped = t.cap.bytes > 0;
        const pct = capped
          ? Math.min(100, (t.cycle.used / t.cap.bytes) * 100)
          : 0;
        const level = LEVEL[t.cap.level];
        return (
          <>
            <div className="grid grid-cols-2 gap-3">
              <Figure value={formatBytes(today)} caption="今日" />
              <Figure
                value={formatBytes(t.cycle.used)}
                caption={`本周期（剩 ${t.cycle.days_left} 天）`}
              />
            </div>
            {capped && (
              <div className="flex flex-col gap-1">
                <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
                  <span className="tabular-nums">
                    {formatBytes(t.cycle.used)} / {formatBytes(t.cap.bytes)}（
                    {pct.toFixed(0)}%）
                  </span>
                  {level && (
                    <Badge variant={level.variant}>{level.label}</Badge>
                  )}
                </div>
                <Progress value={pct} aria-label="本周期用量" />
              </div>
            )}
            <div className="flex flex-wrap gap-1">
              {!capped && <Badge variant="outline">未设上限</Badge>}
              {t.cap.cut_active && (
                <Badge variant="destructive">已因超额断网</Badge>
              )}
              {!t.available && <Badge variant="outline">计数暂不可用</Badge>}
              {!t.clock && <Badge variant="outline">时钟未同步</Badge>}
            </div>
            <KvList
              items={[
                {
                  label: "今日下行/上行",
                  value: `${formatBytes(t.today.rx)} / ${formatBytes(t.today.tx)}`,
                },
                { label: "周期", value: `${t.cycle.start} 至 ${t.cycle.last}` },
              ]}
            />
          </>
        );
      }}
    </Tile>
  );
}

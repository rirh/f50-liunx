import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import { t } from "@/i18n";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { formatClock, rateText } from "../../lib/format";
import type { Rate } from "../../lib/rates";

const SPAN = 60_000;

const chartConfig = () =>
  ({
    down: { label: t("proxy.data.traffic.down"), color: "var(--primary)" },
    up: { label: t("proxy.data.traffic.up"), color: "var(--chart-3)" },
  }) satisfies ChartConfig;

/** A round scale ≥ the peak: 1, 2 or 5 × 1024ⁿ bytes/s, at least 10 KB/s (one-node's traffic chart). */
function niceMax(peak: number): number {
  const floor = 10 * 1024;
  let unit = 1;
  while (peak / unit >= 1000) unit *= 1024;
  for (const step of [unit, unit * 1024])
    for (const m of [1, 2, 5, 10, 20, 50, 100, 200, 500])
      if (m * step >= Math.max(peak * 1.15, floor)) return m * step;
  return Math.max(peak, floor);
}

/** The last 60 seconds of download and upload through the proxy, newest on the right. */
export function TrafficChart({ rates, now }: { rates: Rate[]; now: number }) {
  const config = chartConfig();
  const max = niceMax(Math.max(0, ...rates.map((r) => Math.max(r.up, r.down))));
  const end = rates.at(-1)?.t ?? now;
  return (
    <ChartContainer config={config} className="aspect-auto h-44 w-full lg:h-72">
      <AreaChart
        data={rates}
        margin={{ left: 0, right: 8, top: 10, bottom: 0 }}
      >
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="t"
          type="number"
          domain={[end - SPAN, end]}
          hide
          allowDataOverflow
        />
        <YAxis
          domain={[0, max]}
          ticks={[0, max / 2, max]}
          width={64}
          tickLine={false}
          axisLine={false}
          tickFormatter={(v: number) => rateText(v)}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(_, p) => {
                const ts = p?.[0]?.payload?.t as number | undefined;
                return ts ? formatClock(ts) : "";
              }}
              formatter={(v, name) => (
                <span className="flex w-full justify-between gap-4">
                  <span className="text-muted-foreground">
                    {config[name as keyof typeof config]?.label}
                  </span>
                  <span className="tabular-nums">
                    {typeof v === "number" ? rateText(v) : "—"}
                  </span>
                </span>
              )}
            />
          }
        />
        <Area
          dataKey="up"
          type="monotone"
          stroke="var(--color-up)"
          fill="var(--color-up)"
          fillOpacity={0.12}
          isAnimationActive={false}
        />
        <Area
          dataKey="down"
          type="monotone"
          stroke="var(--color-down)"
          fill="var(--color-down)"
          fillOpacity={0.2}
          isAnimationActive={false}
        />
      </AreaChart>
    </ChartContainer>
  );
}

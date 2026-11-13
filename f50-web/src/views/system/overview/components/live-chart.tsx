import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import { t } from "@/i18n";
import { format } from "date-fns";
import { Area, AreaChart, CartesianGrid, Line, XAxis, YAxis } from "recharts";
import type { Sample } from "../types";

const chartConfig = () =>
  ({
    cpu: { label: "CPU", color: "var(--primary)" },
    mem: { label: t("system.overview.chart.mem"), color: "var(--chart-2)" },
    temp: {
      label: t("system.overview.chart.temp"),
      color: "var(--destructive)",
    },
  }) satisfies ChartConfig;

/** CPU and memory share (left axis, %) and the hottest zone (right axis, °C) over the last readings. */
export function LiveChart({ samples }: { samples: Sample[] }) {
  const config = chartConfig();
  return (
    <ChartContainer config={config} className="aspect-auto h-40 w-full">
      <AreaChart
        data={samples}
        margin={{ left: 0, right: 8, top: 10, bottom: 0 }}
      >
        <CartesianGrid vertical={false} />
        <XAxis dataKey="t" hide />
        <YAxis
          domain={[0, 100]}
          width={44}
          tickLine={false}
          axisLine={false}
          tickFormatter={(v) => `${v}%`}
        />
        <YAxis
          yAxisId="temp"
          orientation="right"
          domain={[30, 90]}
          width={40}
          tickLine={false}
          axisLine={false}
          tickCount={4}
          tickFormatter={(v) => `${v}°`}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(_, p) => {
                const ts = p?.[0]?.payload?.t as number | undefined;
                return ts ? format(ts, "HH:mm:ss") : "";
              }}
              formatter={(v, name) => (
                <span className="flex w-full justify-between gap-4">
                  <span className="text-muted-foreground">
                    {config[name as keyof typeof config]?.label}
                  </span>
                  <span className="tabular-nums">
                    {typeof v === "number"
                      ? `${v.toFixed(1)}${name === "temp" ? " °C" : "%"}`
                      : "—"}
                  </span>
                </span>
              )}
            />
          }
        />
        <Area
          dataKey="mem"
          type="monotone"
          stroke="var(--color-mem)"
          fill="var(--color-mem)"
          fillOpacity={0.15}
          isAnimationActive={false}
          connectNulls
        />
        <Area
          dataKey="cpu"
          type="monotone"
          stroke="var(--color-cpu)"
          fill="var(--color-cpu)"
          fillOpacity={0.2}
          isAnimationActive={false}
          connectNulls
        />
        <Line
          yAxisId="temp"
          dataKey="temp"
          type="monotone"
          stroke="var(--color-temp)"
          strokeWidth={1.5}
          strokeDasharray="4 3"
          dot={false}
          isAnimationActive={false}
          connectNulls
        />
      </AreaChart>
    </ChartContainer>
  );
}

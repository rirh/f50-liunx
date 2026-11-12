import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import { format } from "date-fns";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
import type { Sample } from "../types";

const config = {
  cpu: { label: "CPU", color: "var(--primary)" },
  mem: { label: "内存", color: "var(--chart-2)" },
} satisfies ChartConfig;

/** CPU and memory share over the last polls. */
export function LiveChart({ samples }: { samples: Sample[] }) {
  return (
    <ChartContainer config={config} className="aspect-auto h-40 w-full">
      <AreaChart
        data={samples}
        margin={{ left: 0, right: 8, top: 8, bottom: 0 }}
      >
        <CartesianGrid vertical={false} />
        <XAxis dataKey="t" hide />
        <YAxis
          domain={[0, 100]}
          width={32}
          tickLine={false}
          axisLine={false}
          tickFormatter={(v) => `${v}%`}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(_, p) => {
                const t = p?.[0]?.payload?.t as number | undefined;
                return t ? format(t, "HH:mm:ss") : "";
              }}
              formatter={(v, name) => (
                <span className="flex w-full justify-between gap-4">
                  <span className="text-muted-foreground">
                    {config[name as keyof typeof config]?.label}
                  </span>
                  <span className="font-mono tabular-nums">
                    {typeof v === "number" ? `${v.toFixed(1)}%` : "—"}
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
      </AreaChart>
    </ChartContainer>
  );
}

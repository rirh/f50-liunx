import { useMemo } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import type { DayUsage } from "../types";

/** Bars keep this width; they only narrow when 31 days don't fit. */
const BAR_WIDTH = 24;

const chartConfig = () =>
  ({
    rx: { label: t("cellular.traffic.download"), color: "var(--chart-1)" },
    tx: { label: t("cellular.traffic.upload"), color: "var(--chart-2)" },
  }) satisfies ChartConfig;

/** Daily usage of the last 31 days, download and upload stacked. */
export function UsageChart({ days }: { days: DayUsage[] }) {
  const data = useMemo(() => days.slice(0, 31).reverse(), [days]);
  const config = useMemo(() => chartConfig(), []);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.traffic.chart.title")}</CardTitle>
        <CardDescription>
          {t("cellular.traffic.chart.description", { count: data.length })}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{t("cellular.traffic.chart.emptyTitle")}</EmptyTitle>
              <EmptyDescription>
                {t("cellular.traffic.chart.emptyDescription")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <ChartContainer config={config} className="aspect-auto h-64 w-full">
            <BarChart data={data} margin={{ left: 4, right: 4 }}>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="date"
                tickLine={false}
                axisLine={false}
                minTickGap={16}
                tickFormatter={(d: string) => d.slice(5)}
              />
              <YAxis
                tickLine={false}
                axisLine={false}
                width={64}
                tickFormatter={(v: number) => formatBytes(v, 0)}
              />
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    formatter={(v, name) => (
                      <div className="flex w-full justify-between gap-4">
                        <span className="text-muted-foreground">
                          {config[name as keyof typeof config]?.label ?? name}
                        </span>
                        <span className="font-mono font-medium tabular-nums">
                          {formatBytes(Number(v))}
                        </span>
                      </div>
                    )}
                  />
                }
              />
              <ChartLegend content={<ChartLegendContent />} />
              <Bar
                dataKey="rx"
                stackId="a"
                fill="var(--color-rx)"
                maxBarSize={BAR_WIDTH}
              />
              <Bar
                dataKey="tx"
                stackId="a"
                fill="var(--color-tx)"
                maxBarSize={BAR_WIDTH}
                radius={[3, 3, 0, 0]}
              />
            </BarChart>
          </ChartContainer>
        )}
      </CardContent>
    </Card>
  );
}

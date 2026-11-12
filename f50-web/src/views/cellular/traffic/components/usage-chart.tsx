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
import { formatBytes } from "@/lib/format";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import type { DayUsage } from "../types";

const config = {
  rx: { label: "下载", color: "var(--chart-1)" },
  tx: { label: "上传", color: "var(--chart-2)" },
} satisfies ChartConfig;

/** Daily usage of the last 31 days, download and upload stacked. */
export function UsageChart({ days }: { days: DayUsage[] }) {
  const data = useMemo(() => days.slice(0, 31).reverse(), [days]);
  return (
    <Card>
      <CardHeader>
        <CardTitle>每日用量</CardTitle>
        <CardDescription>最近 {data.length} 天（按本地日期）</CardDescription>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>暂无数据</EmptyTitle>
              <EmptyDescription>
                时钟同步并经过一轮统计后显示。
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
              <Bar dataKey="rx" stackId="a" fill="var(--color-rx)" />
              <Bar
                dataKey="tx"
                stackId="a"
                fill="var(--color-tx)"
                radius={[3, 3, 0, 0]}
              />
            </BarChart>
          </ChartContainer>
        )}
      </CardContent>
    </Card>
  );
}

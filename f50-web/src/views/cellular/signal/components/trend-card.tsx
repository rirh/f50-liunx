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
import { t } from "@/i18n";
import { useSeries } from "@/lib/series";
import { format } from "date-fns";
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceArea,
  XAxis,
  YAxis,
} from "recharts";

const config = {
  rsrp: { label: "RSRP", color: "var(--primary)" },
  sinr: { label: "SINR", color: "var(--chart-2)" },
} satisfies ChartConfig;

const UNIT: Record<keyof typeof config, string> = { rsrp: "dBm", sinr: "dB" };

/**
 * RSRP (left axis, dBm) and SINR (right axis, dB) over the last minutes. The shaded band is RSRP below -105 dBm,
 * where a connection usually struggles.
 */
export function TrendCard() {
  const points = useSeries("signal");
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.signal.trend.title")}</CardTitle>
        <CardDescription>
          {t("cellular.signal.trend.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {points.length > 1 ? (
          <ChartContainer config={config} className="aspect-auto h-52 w-full">
            <LineChart
              data={points}
              margin={{ left: 0, right: 0, top: 8, bottom: 0 }}
            >
              <CartesianGrid vertical={false} />
              <XAxis dataKey="t" hide />
              <YAxis
                yAxisId="rsrp"
                domain={[-130, -60]}
                width={44}
                tickLine={false}
                axisLine={false}
                tickCount={5}
              />
              <YAxis
                yAxisId="sinr"
                orientation="right"
                domain={[-10, 35]}
                width={32}
                tickLine={false}
                axisLine={false}
                tickCount={5}
              />
              <ReferenceArea
                yAxisId="rsrp"
                y1={-130}
                y2={-105}
                fill="var(--destructive)"
                fillOpacity={0.06}
                ifOverflow="hidden"
              />
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    labelFormatter={(_, p) => {
                      const t = p?.[0]?.payload?.t as number | undefined;
                      return t ? format(t, "HH:mm:ss") : "";
                    }}
                    formatter={(v, name) => {
                      const k = name as keyof typeof config;
                      return (
                        <span className="flex w-full justify-between gap-4">
                          <span className="text-muted-foreground">
                            {config[k]?.label}
                          </span>
                          <span className="tabular-nums">
                            {typeof v === "number"
                              ? `${v.toFixed(1)} ${UNIT[k]}`
                              : "—"}
                          </span>
                        </span>
                      );
                    }}
                  />
                }
              />
              <ChartLegend content={<ChartLegendContent />} />
              <Line
                yAxisId="rsrp"
                dataKey="rsrp"
                type="monotone"
                stroke="var(--color-rsrp)"
                strokeWidth={2}
                dot={false}
                connectNulls
                isAnimationActive={false}
              />
              <Line
                yAxisId="sinr"
                dataKey="sinr"
                type="monotone"
                stroke="var(--color-sinr)"
                strokeWidth={1.5}
                dot={false}
                connectNulls
                isAnimationActive={false}
              />
            </LineChart>
          </ChartContainer>
        ) : (
          <div className="flex h-52 items-center justify-center rounded-lg border border-dashed text-sm text-muted-foreground">
            {t("cellular.signal.trend.waiting")}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

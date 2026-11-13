import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { ChevronDownIcon } from "lucide-react";
import { LiveChart } from "./live-chart";
import type { LiveStatus } from "../hooks/use-live-status";
import { formatCelsius, zoneLabel } from "../utils";

const LEGEND = [
  { key: "cpu", label: () => "CPU", swatch: "bg-primary" },
  {
    key: "mem",
    label: () => t("system.overview.chart.mem"),
    swatch: "bg-[var(--chart-2)]",
  },
  {
    key: "temp",
    label: () => t("system.overview.chart.temp"),
    swatch: "bg-destructive",
  },
] as const;

/** The CPU, memory and temperature history, and every thermal zone folded under the four that matter. */
export function TrendCard({ live }: { live: LiveStatus | undefined }) {
  const [open, setOpen] = useState(false);
  const temps = live?.status.temperatures;
  const key = temps
    ? [
        { name: "SoC", value: temps.soc },
        { name: "CPU", value: temps.cpu },
        { name: t("system.overview.zone.modem"), value: temps.modem },
        { name: t("system.overview.zone.board"), value: temps.board },
      ]
    : [];
  const zones = temps?.zones ?? [];
  return (
    <Card className="gap-4">
      <CardHeader className="flex flex-wrap items-center justify-between gap-2">
        <CardTitle>{t("system.overview.live.trend")}</CardTitle>
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          {LEGEND.map((l) => (
            <span key={l.key} className="flex items-center gap-1.5">
              <span className={cn("size-2 rounded-full", l.swatch)} />
              {l.label()}
            </span>
          ))}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <LiveChart samples={live?.samples ?? []} />
        {key.length > 0 && (
          <div className="grid grid-cols-2 gap-px overflow-hidden rounded-xl bg-border/60 sm:grid-cols-4">
            {key.map((k) => (
              <div
                key={k.name}
                className="flex flex-col gap-0.5 bg-muted/40 px-3.5 py-2.5"
              >
                <span className="text-xs text-muted-foreground">{k.name}</span>
                <span className="text-sm font-medium tabular-nums">
                  {formatCelsius(k.value)}
                </span>
              </div>
            ))}
          </div>
        )}
        {zones.length > 0 && (
          <Collapsible open={open} onOpenChange={setOpen}>
            <CollapsibleTrigger className="flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground">
              {t("system.overview.live.sensors", { n: zones.length })}
              <ChevronDownIcon
                aria-hidden
                className={cn(
                  "size-3.5 transition-transform",
                  open && "rotate-180",
                )}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="mt-3 grid grid-cols-2 gap-x-6 gap-y-1.5 text-xs sm:grid-cols-3">
                {zones.map((z) => (
                  <div
                    key={z.name}
                    className="flex items-baseline justify-between gap-2"
                  >
                    <span className="truncate text-muted-foreground">
                      {zoneLabel(z.name)}
                    </span>
                    <span className="shrink-0 tabular-nums">
                      {formatCelsius(z.millicelsius / 1000)}
                    </span>
                  </div>
                ))}
              </div>
            </CollapsibleContent>
          </Collapsible>
        )}
      </CardContent>
    </Card>
  );
}

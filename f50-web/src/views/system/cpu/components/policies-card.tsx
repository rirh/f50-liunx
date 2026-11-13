import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Empty, EmptyDescription, EmptyHeader } from "@/components/ui/empty";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import type { CpuPolicy } from "../types";

/** 614 -> "614 MHz", 2002 -> "2.00 GHz": the unit a person reads at a glance. */
function freq(mhz: number | null): string {
  if (mhz == null) return "—";
  return mhz >= 1000 ? `${(mhz / 1000).toFixed(2)} GHz` : `${mhz} MHz`;
}

/** "0-3" -> 4. */
function coreCount(cpus: string): number {
  const [a, b] = cpus.split("-").map(Number);
  return Number.isFinite(a) && Number.isFinite(b) ? b - a + 1 : 1;
}

/**
 * A cluster's plain name from where its top speed ranks: the slowest group is the efficiency cores, the fastest of
 * three or more the prime core, the rest performance cores. One cluster is just "CPU".
 */
function clusterName(p: CpuPolicy, all: CpuPolicy[]): string {
  if (all.length === 1) return t("system.cpu.clusters.all");
  const ranked = [...all].sort((a, b) => (a.hwmax ?? 0) - (b.hwmax ?? 0));
  const i = ranked.indexOf(p);
  if (i === 0) return t("system.cpu.clusters.little");
  if (i === ranked.length - 1 && all.length >= 3)
    return t("system.cpu.clusters.prime");
  return t("system.cpu.clusters.big");
}

/** One cluster: its name and cores, the speed now in large figures, and a bar of it against the hardware range. */
function Cluster({ p, all }: { p: CpuPolicy; all: CpuPolicy[] }) {
  const name = clusterName(p, all);
  const lo = p.hwmin ?? 0;
  const hi = p.hwmax ?? 0;
  const span = hi - lo;
  const at = (v: number | null) =>
    v == null || span <= 0
      ? null
      : Math.min(100, Math.max(0, ((v - lo) / span) * 100));
  const cur = at(p.cur);
  const cap =
    p.max != null && p.hwmax != null && p.max < p.hwmax ? at(p.max) : null;
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-foreground/5 bg-muted/25 p-3">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-sm font-medium">{name}</span>
        <span className="text-xs text-muted-foreground tabular-nums">
          {t("system.cpu.clusters.cores", {
            count: coreCount(p.cpus),
            cpus: p.cpus,
          })}
        </span>
      </div>
      <span className="text-2xl leading-none font-semibold tabular-nums">
        {freq(p.cur)}
      </span>
      <div
        role="meter"
        aria-label={t("system.cpu.clusters.curLabel", { name })}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(cur ?? 0)}
        className="relative h-1.5 overflow-hidden rounded-full bg-foreground/8"
      >
        {cur != null && (
          <span
            className="absolute inset-y-0 left-0 rounded-full bg-primary transition-[width] duration-500"
            style={{ width: `${Math.max(cur, 2)}%` }}
          />
        )}
        {cap != null && (
          <span
            aria-hidden
            className="absolute inset-y-0 right-0 bg-[repeating-linear-gradient(135deg,var(--color-muted-foreground)_0_2px,transparent_2px_5px)] opacity-30"
            style={{ left: `${cap}%` }}
          />
        )}
      </div>
      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-0.5 text-xs text-muted-foreground tabular-nums">
        <span
          className={cn(cap != null && "text-amber-600 dark:text-amber-400")}
        >
          {cap != null
            ? t("system.cpu.clusters.capped", { max: freq(p.max) })
            : t("system.cpu.clusters.range", {
                min: freq(p.hwmin),
                max: freq(p.hwmax),
              })}
        </span>
        {p.governor && (
          <span>{t("system.cpu.clusters.governor", { name: p.governor })}</span>
        )}
      </div>
    </div>
  );
}

/** Each group of cores and the speed it runs at now, slowest group first. */
export function PoliciesCard({ policies }: { policies: CpuPolicy[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.cpu.clusters.title")}</CardTitle>
        <CardDescription>
          {t("system.cpu.clusters.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {policies.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyDescription>
                {t("system.cpu.policies.empty")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className="grid gap-2">
            {policies.map((p) => (
              <Cluster key={p.name} p={p} all={policies} />
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

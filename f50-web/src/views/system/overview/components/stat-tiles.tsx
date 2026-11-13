import type { ReactNode } from "react";
import { AnimatedNumber } from "@/components/f50/animated-number";
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  CpuIcon,
  HardDriveIcon,
  type LucideIcon,
  MemoryStickIcon,
  ThermometerIcon,
} from "lucide-react";
import type { LiveStatus } from "../hooks/use-live-status";
import type { SystemInfo } from "../types";
import {
  diskPercent,
  formatCelsius,
  formatMhz,
  formatPercent,
  kb,
  memPercent,
} from "../utils";

function Tile({
  icon: Icon,
  label,
  value,
  percent,
  hint,
}: {
  icon: LucideIcon;
  label: string;
  value: ReactNode;
  percent: number | null;
  hint?: string;
}) {
  const p =
    percent == null ? null : Math.min(100, Math.max(0, Math.round(percent)));
  const critical = p != null && p >= 90;
  const warning = p != null && p >= 70;
  return (
    <div className="relative min-w-0 overflow-hidden rounded-lg border border-foreground/5 bg-muted/25 px-3 py-2">
      {p != null && p > 0 && (
        <span
          aria-hidden
          className={cn(
            "absolute inset-y-0 left-0 transition-[width] duration-500",
            critical
              ? "bg-destructive/15"
              : warning
                ? "bg-amber-500/18"
                : "bg-emerald-500/14",
          )}
          style={{ width: `${p}%` }}
        />
      )}
      <div className="relative flex items-baseline justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5 text-[13px] text-muted-foreground">
          <Icon aria-hidden className="size-3.5 shrink-0" />
          <span className="truncate">{label}</span>
        </span>
        <span
          className={cn(
            "shrink-0 font-mono text-sm font-semibold tabular-nums",
            critical
              ? "text-destructive"
              : warning
                ? "text-amber-600 dark:text-amber-400"
                : "text-foreground",
          )}
        >
          {value}
        </span>
      </div>
      <span
        className="relative block truncate font-mono text-[11px] leading-tight text-muted-foreground"
        title={hint}
      >
        {hint ?? "—"}
      </span>
    </div>
  );
}

/** Temperatures from 30 °C (empty) to 90 °C (full), for the bar. */
const tempShare = (c: number | null) =>
  c == null ? null : ((c - 30) / 60) * 100;

/** The four figures that matter most: CPU, memory, storage and temperature. */
export function StatTiles({
  live,
  info,
}: {
  live: LiveStatus | undefined;
  info: SystemInfo | undefined;
}) {
  const s = live?.status;
  const memory = s?.memory ?? info?.memory;
  const used =
    memory?.total_kb != null && memory.avail_kb != null
      ? memory.total_kb - memory.avail_kb
      : null;
  const mem = memPercent(memory);
  const disk = info?.storage.disk ?? info?.storage.root ?? null;
  const temps = s?.temperatures;
  const hottest = temps?.hottest ?? null;
  const peak = s?.load?.freqs.reduce<number | null>(
    (m, f) => (f.cur != null && (m == null || f.cur > m) ? f.cur : m),
    null,
  );
  return (
    <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
      <Tile
        icon={CpuIcon}
        label="CPU"
        value={
          <AnimatedNumber
            value={live?.cpu}
            format={(n) => formatPercent(n, 1)}
          />
        }
        percent={live?.cpu ?? null}
        hint={
          s?.load
            ? t("system.overview.live.cpuHint", {
                n: s.load.n,
                freqs: formatMhz(peak),
              })
            : undefined
        }
      />
      <Tile
        icon={MemoryStickIcon}
        label={t("system.overview.live.mem")}
        value={
          <AnimatedNumber value={mem} format={(n) => formatPercent(n, 0)} />
        }
        percent={mem}
        hint={`${formatBytes(kb(used))} / ${formatBytes(kb(memory?.total_kb))}`}
      />
      <Tile
        icon={HardDriveIcon}
        label={t("system.overview.live.storage")}
        value={formatPercent(diskPercent(disk), 0)}
        percent={diskPercent(disk)}
        hint={
          disk
            ? `${formatBytes(kb(disk.used_kb))} / ${formatBytes(kb(disk.size_kb))}`
            : t("system.overview.info.notMounted")
        }
      />
      <Tile
        icon={ThermometerIcon}
        label={t("system.overview.live.hottest")}
        value={<AnimatedNumber value={hottest} format={formatCelsius} />}
        percent={tempShare(hottest)}
        hint={
          temps
            ? `SoC ${formatCelsius(temps.soc)} · ${t("system.overview.zone.modem")} ${formatCelsius(temps.modem)}`
            : undefined
        }
      />
    </div>
  );
}

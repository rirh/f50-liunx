import { Link } from "react-router-dom";
import { AnimatedNumber } from "@/components/f50/animated-number";
import { FillMeter } from "@/components/f50/fill-meter";
import { t } from "@/i18n";
import { formatBytes, formatDuration } from "@/lib/format";
import { useSeries } from "@/lib/series";
import { cn, tempShare } from "@/lib/utils";
import { getInfo, systemKeys } from "@/views/system/overview/api";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronRightIcon,
  CpuIcon,
  HardDriveIcon,
  MemoryStickIcon,
  ThermometerIcon,
} from "lucide-react";
import type { DashboardStatus } from "../types";

const ok = <T,>(p: { ok: true; data: T } | { ok: false } | undefined) =>
  p?.ok ? p.data : null;

const share = (
  used: number | null | undefined,
  total: number | null | undefined,
) => (used != null && total ? (used / total) * 100 : null);

const kb = (n: number | null | undefined) =>
  n == null ? "—" : formatBytes(n * 1024);

const pct = (digits: number) => (n: number | null) =>
  n == null ? "—" : `${n.toFixed(digits)}%`;

const celsius = (n: number | null | undefined) =>
  n == null ? "—" : `${n.toFixed(1)} °C`;

/**
 * The device itself, beside the network card: its name, model and uptime, then CPU, memory, storage and the hottest
 * zone as background-filled figures (the same tiles as the system page).
 */
export function SystemCard({ s }: { s: DashboardStatus }) {
  const info = useQuery({
    queryKey: systemKeys.info,
    queryFn: getInfo,
    staleTime: 60_000,
  });
  const i = info.data;
  const system = ok(s.system);
  const cpu = useSeries("system").at(-1)?.cpu ?? null;

  const mem = system?.memory ?? i?.memory;
  const memUsed =
    mem?.total_kb != null && mem.avail_kb != null
      ? mem.total_kb - mem.avail_kb
      : null;
  const memShare = share(memUsed, mem?.total_kb);
  const disk = i?.storage?.disk ?? i?.storage?.root ?? null;
  const diskShare = share(disk?.used_kb, disk?.size_kb);
  const temps = system?.temperatures;
  const hottest = temps ? (temps.hottest ?? temps.cpu) : null;
  const hot = !!system?.heat_alarm;
  const uptime = system?.uptime ?? i?.uptime ?? null;

  return (
    <article className="flex flex-col gap-3 rounded-2xl border bg-card p-4 text-card-foreground">
      <Link
        to="/system/overview"
        className="group flex min-w-0 items-center gap-2"
      >
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex min-w-0 items-baseline gap-1.5">
            <span className="truncate text-sm font-semibold group-hover:text-primary">
              {i?.hostname ?? "ZTE F50"}
            </span>
            {i?.model && (
              <span className="truncate text-xs text-muted-foreground">
                {i.model}
              </span>
            )}
          </span>
          <span className="truncate text-xs text-muted-foreground">
            {uptime == null
              ? "—"
              : t("dashboard.overview.uptime", {
                  time: formatDuration(uptime),
                })}
            {hot && (
              <span className="ms-1.5 font-medium text-destructive">
                · {t("dashboard.hero.hot")}
              </span>
            )}
          </span>
        </span>
        <ChevronRightIcon
          aria-hidden
          className="size-4 shrink-0 text-muted-foreground/60 transition-transform group-hover:translate-x-0.5"
        />
      </Link>
      <div className="grid grid-cols-2 gap-2">
        <FillMeter
          icon={CpuIcon}
          label={t("dashboard.hero.cpu")}
          value={<AnimatedNumber value={cpu} format={pct(1)} />}
          percent={cpu}
          hint={
            system?.load
              ? t("dashboard.device.cores", { n: system.load.n })
              : undefined
          }
        />
        <FillMeter
          icon={MemoryStickIcon}
          label={t("dashboard.hero.memory")}
          value={<AnimatedNumber value={memShare} format={pct(0)} />}
          percent={memShare}
          hint={`${kb(memUsed)} / ${kb(mem?.total_kb)}`}
        />
        <FillMeter
          icon={HardDriveIcon}
          label={t("dashboard.device.storage")}
          value={pct(0)(diskShare)}
          percent={diskShare}
          hint={disk ? `${kb(disk.used_kb)} / ${kb(disk.size_kb)}` : undefined}
        />
        <FillMeter
          icon={ThermometerIcon}
          label={t("system.overview.live.hottest")}
          value={
            <span className={cn(hot && "text-destructive")}>
              <AnimatedNumber value={hottest} format={celsius} />
            </span>
          }
          percent={hot ? 100 : tempShare(hottest)}
          hint={temps ? `SoC ${celsius(temps.soc)}` : undefined}
        />
      </div>
    </article>
  );
}

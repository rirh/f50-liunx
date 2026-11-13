import { AnimatedNumber } from "@/components/f50/animated-number";
import { FillMeter } from "@/components/f50/fill-meter";
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { tempShare } from "@/lib/utils";
import {
  CpuIcon,
  HardDriveIcon,
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
      <FillMeter
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
      <FillMeter
        icon={MemoryStickIcon}
        label={t("system.overview.live.mem")}
        value={
          <AnimatedNumber value={mem} format={(n) => formatPercent(n, 0)} />
        }
        percent={mem}
        hint={`${formatBytes(kb(used))} / ${formatBytes(kb(memory?.total_kb))}`}
      />
      <FillMeter
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
      <FillMeter
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

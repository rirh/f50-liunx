import { useState } from "react";
import { Link } from "react-router-dom";
import { AnimatedNumber } from "@/components/f50/animated-number";
import { FillMeter } from "@/components/f50/fill-meter";
import { t } from "@/i18n";
import { copyText } from "@/lib/clipboard";
import { formatBytes, formatDuration } from "@/lib/format";
import { useSeries } from "@/lib/series";
import { cn, tempShare, tempTone } from "@/lib/utils";
import { getIdentity } from "@/views/cellular/signal/api";
import { getInfo, systemKeys } from "@/views/system/overview/api";
import { useQuery } from "@tanstack/react-query";
import {
  CheckIcon,
  ChevronRightIcon,
  CopyIcon,
  CpuIcon,
  HardDriveIcon,
  MemoryStickIcon,
  ThermometerIcon,
} from "lucide-react";
import { toast } from "sonner";
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
 * The modem's IMEI (f50d keeps it read; a write-access method, which the signed-in root has), set quietly after the
 * uptime as a tag: the tag is the copy button, its small icon a tick once copied.
 */
function Imei() {
  const q = useQuery({
    queryKey: ["modem", "identity"],
    queryFn: ({ signal }) => getIdentity(signal),
    staleTime: 5 * 60_000,
    retry: false,
  });
  const [copied, setCopied] = useState(false);
  const imei = q.data?.ident?.imei;
  if (!imei) return null;
  const copy = () =>
    copyText(imei).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => toast.error(t("dashboard.device.copyFailed")),
    );
  const Icon = copied ? CheckIcon : CopyIcon;
  return (
    <button
      type="button"
      className="group/imei inline-flex h-5 min-w-0 cursor-pointer items-center gap-1 overflow-hidden rounded-[5px] bg-foreground/[0.06] ps-1.5 pe-1 text-2xs leading-none tabular-nums transition-colors outline-none hover:bg-foreground/[0.1] focus-visible:ring-2 focus-visible:ring-ring/50"
      title={copied ? t("common.action.copied") : t("common.action.copy")}
      aria-label={`IMEI ${imei}, ${t("common.action.copy")}`}
      onClick={() => void copy()}
    >
      <span className="shrink-0 font-semibold text-muted-foreground/80">
        IMEI
      </span>
      <span className="truncate font-medium text-foreground/80">{imei}</span>
      <Icon
        aria-hidden
        className={cn(
          "size-3 shrink-0",
          copied
            ? "text-[#248A3D] dark:text-[#30D158]"
            : "text-muted-foreground/70 group-hover/imei:text-foreground",
        )}
      />
    </button>
  );
}

/**
 * The device itself, beside the network card: its name, model and uptime, then CPU, memory, storage and the hottest
 * zone as background-filled figures; the IMEI is a tag after the uptime (the same tiles as the system page).
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
    <article className="flex flex-col gap-3 rounded-2xl bg-card p-4 text-card-foreground">
      <div className="flex min-w-0 items-center gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <Link
            to="/system/overview"
            className="group flex min-w-0 items-baseline gap-1.5"
          >
            <span className="truncate text-sm font-semibold group-hover:text-primary">
              {i?.hostname ?? "ZTE F50"}
            </span>
            {i?.model && (
              <span className="truncate text-xs text-muted-foreground">
                {i.model}
              </span>
            )}
          </Link>
          <span className="flex min-w-0 items-center gap-1.5 text-xs whitespace-nowrap text-muted-foreground">
            <span className="shrink-0">
              {uptime == null
                ? "—"
                : t("dashboard.overview.uptime", {
                    time: formatDuration(uptime),
                  })}
            </span>
            {hot && (
              <span className="shrink-0 font-medium text-destructive">
                · {t("dashboard.hero.hot")}
              </span>
            )}
            <Imei />
          </span>
        </div>
        <Link
          to="/system/overview"
          aria-label={t("dashboard.hero.system")}
          className="group -me-1 p-1"
        >
          <ChevronRightIcon
            aria-hidden
            className="size-4 shrink-0 text-muted-foreground/60 transition-transform group-hover:translate-x-0.5"
          />
        </Link>
      </div>
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
          value={<AnimatedNumber value={hottest} format={celsius} />}
          percent={hot ? 100 : tempShare(hottest)}
          tone={hot ? "danger" : tempTone(hottest)}
          hint={temps ? `SoC ${celsius(temps.soc)}` : undefined}
        />
      </div>
    </article>
  );
}

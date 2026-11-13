import { Link } from "react-router-dom";
import { AnimatedNumber } from "@/components/f50/animated-number";
import { Button } from "@/components/ui/button";
import { type ChartConfig, ChartContainer } from "@/components/ui/chart";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { t } from "@/i18n";
import { formatBytes, formatDuration, formatRate } from "@/lib/format";
import { type RatePoint, useSeries } from "@/lib/series";
import { cn } from "@/lib/utils";
import { ArrowDownIcon, ArrowUpIcon, EllipsisIcon } from "lucide-react";
import { Area, AreaChart, YAxis } from "recharts";
import { CarrierLogo } from "./carrier-logo";
import { SignalGrid } from "./device-meters";
import { SignalBars } from "./signal-bars";
import { CARRIER_NAME, carrierOf } from "../carrier";
import { DATA_PHASE_LABEL, RAT_LABEL, label } from "../labels";
import type { DashboardStatus } from "../types";

const ok = <T,>(p: { ok: true; data: T } | { ok: false } | undefined) =>
  p?.ok ? p.data : null;

const share = (
  used: number | null | undefined,
  total: number | null | undefined,
) => (used != null && total ? (used / total) * 100 : null);

/** "23.3 KB/s" -> ["23.3", "KB/s"], so the figure can be large and the unit small (as Apple's widgets set them). */
const split = (s: string): [string, string] => {
  const i = s.lastIndexOf(" ");
  return i < 0 ? [s, ""] : [s.slice(0, i), s.slice(i + 1)];
};

const rateChart = {
  rx: { label: "rx", color: "#0A84FF" },
  tx: { label: "tx", color: "#34C759" },
} satisfies ChartConfig;

/** One direction of the live speed: arrow and label, the figure large, the unit small. */
function Speed({ dir, value }: { dir: "down" | "up"; value: number | null }) {
  const unit = split(formatRate(value))[1];
  const Icon = dir === "down" ? ArrowDownIcon : ArrowUpIcon;
  return (
    <div className="flex min-w-0 flex-col">
      <span
        className={cn(
          "flex items-center gap-1 text-xs font-medium",
          dir === "down"
            ? "text-[#0A84FF]"
            : "text-[#248A3D] dark:text-[#30D158]",
        )}
      >
        <Icon aria-hidden className="size-3.5" strokeWidth={2.5} />
        {t(dir === "down" ? "dashboard.hero.down" : "dashboard.hero.up")}
      </span>
      <span className="truncate">
        <AnimatedNumber
          value={value}
          format={(v) => split(formatRate(v))[0]}
          className="tabular-nums text-xl leading-tight font-semibold"
        />
        <span className="ms-1 text-sm font-medium text-muted-foreground">
          {unit}
        </span>
      </span>
    </div>
  );
}

const DAY_MS = 86_400_000;

/** The share of the billing cycle gone by: its days from `start` to `last`, less those left. */
const cycleElapsed = (start: string, last: string, left: number) => {
  const days = Math.round((Date.parse(last) - Date.parse(start)) / DAY_MS) + 1;
  return days > 0 ? ((days - left) / days) * 100 : null;
};

/** One direction's recent curve, on its own scale, filled with a gradient of its colour down to nothing. */
function SpeedChart({ k, points }: { k: "rx" | "tx"; points: RatePoint[] }) {
  if (points.length < 2)
    return (
      <div className="flex h-10 items-center text-xs text-muted-foreground">
        {t("dashboard.hero.collecting")}
      </div>
    );
  return (
    <ChartContainer
      config={rateChart}
      className="aspect-auto h-10 w-full"
      aria-hidden
    >
      <AreaChart
        data={points}
        margin={{ left: 0, right: 0, top: 6, bottom: 0 }}
      >
        <defs>
          <linearGradient id={`dev-${k}`} x1="0" y1="0" x2="0" y2="1">
            <stop
              offset="0%"
              stopColor={`var(--color-${k})`}
              stopOpacity={0.45}
            />
            <stop
              offset="70%"
              stopColor={`var(--color-${k})`}
              stopOpacity={0.12}
            />
            <stop
              offset="100%"
              stopColor={`var(--color-${k})`}
              stopOpacity={0}
            />
          </linearGradient>
        </defs>
        <YAxis hide domain={[0, (max: number) => Math.max(max * 1.15, 1)]} />
        <Area
          dataKey={k}
          type="monotone"
          stroke={`var(--color-${k})`}
          fill={`url(#dev-${k})`}
          strokeWidth={1.75}
          isAnimationActive={false}
        />
      </AreaChart>
    </ChartContainer>
  );
}

/**
 * The network at a glance, most-looked-at first: the carrier's emblem, the generation and the link (state, SIM,
 * address); the live speed in large figures over its curve; the data used this cycle; the signal's recent readings;
 * the device's own state is the system card's (system-card.tsx). Sections are separated by hairlines, never
 * boxed. Data, the hotspot, USB, messages and the version are the service list's, so this card does not repeat them.
 */
export function DeviceCard({ s }: { s: DashboardStatus }) {
  const modem = ok(s.modem);
  const data = ok(s.data);
  const traffic = ok(s.traffic);

  const rates = useSeries("rate");
  const rate = rates.at(-1);
  const signal = useSeries("signal");

  const carrier = carrierOf(modem?.operator);
  const carrierName =
    (carrier && CARRIER_NAME[carrier]) ??
    modem?.operator ??
    (modem?.registered
      ? t("dashboard.hero.registered")
      : t("dashboard.hero.noNetwork"));

  const online = data?.state === "connected";
  const dot = online
    ? "bg-[#34C759]"
    : data?.state === "failed" || !data
      ? "bg-[#FF3B30]"
      : data.state === "down"
        ? "bg-muted-foreground/50"
        : "bg-[#FF9500]";
  const stateText = data
    ? online && data.since_s != null
      ? t("dashboard.data.connectedFor", { time: formatDuration(data.since_s) })
      : label(DATA_PHASE_LABEL, data.state)
    : t("common.state.readFailed");

  const capped = !!traffic && traffic.cap.bytes > 0;
  const used = traffic ? share(traffic.cycle.used, traffic.cap.bytes) : null;
  // the background fills to the share of the cap used; with no cap, to how far the cycle has gone
  const elapsed = traffic
    ? cycleElapsed(
        traffic.cycle.start,
        traffic.cycle.last,
        traffic.cycle.days_left,
      )
    : null;

  const fill = Math.min(100, Math.max(0, (capped ? used : elapsed) ?? 0));
  const fillTone =
    traffic?.cap.level === "over"
      ? "bg-destructive/15"
      : traffic?.cap.level === "warn"
        ? "bg-amber-500/18"
        : capped
          ? "bg-emerald-500/14"
          : "bg-[#0A84FF]/10";

  return (
    <article
      className="flex h-full flex-col overflow-hidden rounded-2xl border bg-card text-card-foreground"
      data-state={data?.state}
    >
      {/* 1 · the network: carrier, generation, state, bars */}
      <header className="flex items-center gap-3 px-4 pt-3.5 pb-2">
        <CarrierLogo carrier={carrier} />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="flex min-w-0 items-center gap-1.5">
            <Link
              to="/cellular/signal"
              className="truncate text-base leading-tight font-semibold hover:text-primary"
            >
              {carrierName}
            </Link>
            {modem?.registered && modem.rat && (
              <span className="shrink-0 rounded-[5px] bg-foreground px-1.5 py-px text-2xs leading-4 font-semibold text-background">
                {label(RAT_LABEL, modem.rat)}
              </span>
            )}
            {modem?.roaming && (
              <span className="shrink-0 rounded-[5px] bg-[#FF9500]/15 px-1.5 py-px text-2xs leading-4 font-semibold text-[#C93400] dark:text-[#FF9F0A]">
                {t("dashboard.hero.roaming")}
              </span>
            )}
          </div>
          <span className="flex min-w-0 items-center gap-1.5 text-sm text-muted-foreground">
            <span className="relative flex size-2 shrink-0">
              {online && (
                <span
                  className={cn(
                    "absolute size-full animate-ping rounded-full opacity-40",
                    dot,
                  )}
                />
              )}
              <span className={cn("relative size-2 rounded-full", dot)} />
            </span>
            <span className="truncate">{stateText}</span>
            {data && (
              <span className="shrink-0 text-muted-foreground/70">
                · SIM{data.slot}
              </span>
            )}
            {data?.addresses[0] && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="truncate tabular-nums text-xs text-muted-foreground/70">
                    · {data.addresses[0]}
                  </span>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="flex flex-col gap-0.5">
                  {data.addresses.map((a) => (
                    <span key={a} className="tabular-nums">
                      {a}
                    </span>
                  ))}
                </TooltipContent>
              </Tooltip>
            )}
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          {modem && <SignalBars bars={modem.bars} />}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                className="text-muted-foreground"
                aria-label={t("dashboard.device.more")}
              >
                <EllipsisIcon />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem asChild>
                <Link to="/cellular/signal">{t("dashboard.hero.mobile")}</Link>
              </DropdownMenuItem>
              <DropdownMenuItem asChild>
                <Link to="/cellular/data">{t("dashboard.data.title")}</Link>
              </DropdownMenuItem>
              <DropdownMenuItem asChild>
                <Link to="/cellular/traffic">{t("dashboard.hero.usage")}</Link>
              </DropdownMenuItem>
              <DropdownMenuItem asChild>
                <Link to="/system/overview">{t("dashboard.hero.system")}</Link>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>

      {/* 2 · the live speed: download and upload side by side, each over its own curve */}
      <section className="grid grid-cols-2 divide-x pb-2.5">
        <div className="flex min-w-0 flex-col gap-0.5 ps-4 pe-3">
          <Speed dir="down" value={rate?.rx ?? null} />
          <SpeedChart k="rx" points={rates} />
        </div>
        <div className="flex min-w-0 flex-col gap-0.5 ps-3 pe-4">
          <Speed dir="up" value={rate?.tx ?? null} />
          <SpeedChart k="tx" points={rates} />
        </div>
      </section>

      {/* 3 · data used this cycle: one tile, today under the total, a thin bar for how far the cycle has gone */}
      {traffic && (
        <section className="border-t px-4 py-2.5">
          <Link
            to="/cellular/traffic"
            className="relative block overflow-hidden rounded-lg border border-foreground/5 bg-muted/25 px-3 py-2 transition-colors hover:bg-muted/40"
          >
            {fill != null && fill > 0 && (
              <span
                aria-hidden
                className={cn(
                  "absolute inset-y-0 left-0 transition-[width] duration-500",
                  fillTone,
                )}
                style={{ width: `${fill}%` }}
              />
            )}
            <span className="relative flex items-baseline justify-between gap-3">
              <span className="flex min-w-0 items-baseline gap-2 truncate">
                <span className="shrink-0 text-xs font-medium text-muted-foreground">
                  {t("dashboard.hero.usage")}
                </span>
                <AnimatedNumber
                  value={traffic.cycle.used}
                  format={(v) => split(formatBytes(v))[0]}
                  className="tabular-nums text-base leading-tight font-semibold"
                />
                <span className="-ms-1 text-xs font-medium text-muted-foreground">
                  {split(formatBytes(traffic.cycle.used))[1]}
                </span>
                <span className="truncate text-xs text-muted-foreground">
                  /{" "}
                  {capped ? (
                    <span className="tabular-nums">
                      {formatBytes(traffic.cap.bytes)} · {Math.round(used ?? 0)}
                      %
                    </span>
                  ) : (
                    t("dashboard.device.unlimited")
                  )}
                </span>
              </span>
              <span className="flex shrink-0 items-baseline gap-2 text-xs text-muted-foreground">
                <span>
                  {t("dashboard.hero.today")}{" "}
                  <AnimatedNumber
                    value={traffic.today.rx + traffic.today.tx}
                    format={(v) => formatBytes(v)}
                    className="tabular-nums font-semibold text-foreground"
                  />
                </span>
                <span className="tabular-nums">
                  {t("dashboard.hero.daysLeft", {
                    n: traffic.cycle.days_left,
                  })}
                </span>
              </span>
            </span>
          </Link>
        </section>
      )}

      {/* 4 · the signal's recent readings */}
      <section className="border-t px-4 pt-2.5 pb-3">
        <SignalGrid points={signal} />
      </section>
    </article>
  );
}

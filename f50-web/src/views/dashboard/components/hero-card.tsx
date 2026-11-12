import type { ReactNode } from "react";
import { AnimatedNumber } from "@/components/f50/animated-number";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { type ChartConfig, ChartContainer } from "@/components/ui/chart";
import { Progress } from "@/components/ui/progress";
import { t } from "@/i18n";
import { formatBytes, formatDuration, formatRate } from "@/lib/format";
import { useSeries } from "@/lib/series";
import { cn } from "@/lib/utils";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  CpuIcon,
  GaugeIcon,
  type LucideIcon,
  PieChartIcon,
  SignalIcon,
} from "lucide-react";
import { Area, AreaChart, YAxis } from "recharts";
import { SignalBars } from "./signal-bars";
import { TextLink } from "./text-link";
import { RAT_LABEL, label } from "../labels";
import type {
  DashboardStatus,
  ModemStatus,
  SystemStatusInfo,
  TrafficView,
} from "../types";

/** A reading older than this is shown as stale (the collector may be paused by a restart). */
const STALE_MS = 30_000;

const rateChart = {
  rx: { label: "rx", color: "var(--chart-1)" },
  tx: { label: "tx", color: "var(--chart-2)" },
} satisfies ChartConfig;

const pct = (n: number | null) => (n == null ? "—" : `${n.toFixed(0)}%`);
const celsius = (n: number | null) => (n == null ? "—" : `${n.toFixed(1)}°C`);
const db = (u: string) => (n: number | null) =>
  n == null ? "—" : `${n.toFixed(1)} ${u}`;

/** One quarter of the strip: an icon and a linked heading over its figures. */
function Section({
  icon: Icon,
  title,
  to,
  aside,
  children,
}: {
  icon: LucideIcon;
  title: string;
  to: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex min-w-0 flex-col gap-2 p-4">
      <header className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        <Icon aria-hidden className="size-3.5 text-primary" />
        <TextLink to={to}>{title}</TextLink>
        {aside && <span className="ms-auto">{aside}</span>}
      </header>
      {children}
    </section>
  );
}

function Mini({
  label: name,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col">
      <span className="text-[0.7rem] text-muted-foreground">{name}</span>
      <span className="truncate text-sm font-medium tabular-nums">
        {children}
      </span>
    </div>
  );
}

function Mobile({ m }: { m: ModemStatus }) {
  const stale = m.age_ms != null && m.age_ms > STALE_MS;
  return (
    <>
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-col">
          <span className="truncate text-xl leading-tight font-semibold">
            {m.operator ??
              (m.registered
                ? t("dashboard.hero.registered")
                : t("dashboard.hero.unregistered"))}
          </span>
          <span className="text-xs text-muted-foreground">
            {m.rat ? label(RAT_LABEL, m.rat) : t("dashboard.hero.noNetwork")}
          </span>
        </div>
        <SignalBars bars={m.bars} />
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Mini label="RSRP">
          <AnimatedNumber value={m.signal?.rsrp} format={db("dBm")} />
        </Mini>
        <Mini label="SINR">
          <AnimatedNumber value={m.signal?.sinr} format={db("dB")} />
        </Mini>
      </div>
      {(!m.registered ||
        m.roaming ||
        !m.radio_allowed ||
        m.restarting ||
        stale) && (
        <div className="flex flex-wrap gap-1">
          {!m.registered && (
            <Badge variant="destructive">
              {t("dashboard.hero.unregistered")}
            </Badge>
          )}
          {m.roaming && (
            <Badge variant="outline">{t("dashboard.hero.roaming")}</Badge>
          )}
          {!m.radio_allowed && (
            <Badge variant="destructive">{t("dashboard.hero.radioOff")}</Badge>
          )}
          {m.restarting && (
            <Badge variant="outline">{t("dashboard.hero.restarting")}</Badge>
          )}
          {stale && (
            <Badge variant="outline">{t("dashboard.hero.stale")}</Badge>
          )}
        </div>
      )}
    </>
  );
}

function Rate() {
  const points = useSeries("rate");
  const last = points.at(-1);
  const peak = points.reduce((m, p) => Math.max(m, p.rx, p.tx), 0);
  return (
    <>
      <div className="grid grid-cols-2 gap-2">
        <div className="flex min-w-0 flex-col">
          <span className="flex items-center gap-1 text-[0.7rem] text-muted-foreground">
            <ArrowDownIcon
              aria-hidden
              className="size-3 text-[var(--chart-1)]"
            />
            {t("dashboard.hero.down")}
          </span>
          <AnimatedNumber
            value={last?.rx ?? null}
            format={formatRate}
            className="truncate text-lg leading-tight font-semibold"
          />
        </div>
        <div className="flex min-w-0 flex-col">
          <span className="flex items-center gap-1 text-[0.7rem] text-muted-foreground">
            <ArrowUpIcon aria-hidden className="size-3 text-[var(--chart-2)]" />
            {t("dashboard.hero.up")}
          </span>
          <AnimatedNumber
            value={last?.tx ?? null}
            format={formatRate}
            className="truncate text-lg leading-tight font-semibold"
          />
        </div>
      </div>
      {points.length > 1 ? (
        <ChartContainer
          config={rateChart}
          className="aspect-auto h-12 w-full"
          aria-hidden
        >
          <AreaChart
            data={points}
            margin={{ left: 0, right: 0, top: 2, bottom: 0 }}
          >
            <defs>
              {(["rx", "tx"] as const).map((k) => (
                <linearGradient
                  key={k}
                  id={`hero-${k}`}
                  x1="0"
                  y1="0"
                  x2="0"
                  y2="1"
                >
                  <stop
                    offset="0%"
                    stopColor={`var(--color-${k})`}
                    stopOpacity={0.4}
                  />
                  <stop
                    offset="100%"
                    stopColor={`var(--color-${k})`}
                    stopOpacity={0.02}
                  />
                </linearGradient>
              ))}
            </defs>
            <YAxis hide domain={[0, "dataMax"]} />
            <Area
              dataKey="tx"
              type="monotone"
              stroke="var(--color-tx)"
              fill="url(#hero-tx)"
              strokeWidth={1.25}
              isAnimationActive={false}
            />
            <Area
              dataKey="rx"
              type="monotone"
              stroke="var(--color-rx)"
              fill="url(#hero-rx)"
              strokeWidth={1.75}
              isAnimationActive={false}
            />
          </AreaChart>
        </ChartContainer>
      ) : (
        <div className="flex h-12 items-center text-xs text-muted-foreground">
          {t("dashboard.hero.collecting")}
        </div>
      )}
      {peak > 0 && (
        <span className="sr-only">
          {t("dashboard.hero.peak", { rate: formatRate(peak) })}
        </span>
      )}
    </>
  );
}

const LEVEL_VARIANT = {
  ok: "secondary",
  warn: "outline",
  over: "destructive",
} as const;

function Usage({ tr }: { tr: TrafficView }) {
  const capped = tr.cap.bytes > 0;
  const share = capped
    ? Math.min(100, (tr.cycle.used / tr.cap.bytes) * 100)
    : 0;
  const level = tr.cap.level as keyof typeof LEVEL_VARIANT;
  return (
    <>
      <div className="grid grid-cols-2 gap-2">
        <div className="flex min-w-0 flex-col">
          <span className="text-[0.7rem] text-muted-foreground">
            {t("dashboard.hero.today")}
          </span>
          <AnimatedNumber
            value={tr.today.rx + tr.today.tx}
            format={(n) => formatBytes(n)}
            className="truncate text-lg leading-tight font-semibold"
          />
        </div>
        <div className="flex min-w-0 flex-col">
          <span className="text-[0.7rem] text-muted-foreground">
            {t("dashboard.hero.cycle")}
          </span>
          <AnimatedNumber
            value={tr.cycle.used}
            format={(n) => formatBytes(n)}
            className="truncate text-lg leading-tight font-semibold"
          />
        </div>
      </div>
      {capped ? (
        <div className="flex flex-col gap-1">
          <Progress value={share} aria-label={t("dashboard.hero.cycle")} />
          <span className="flex items-center justify-between gap-2 text-[0.7rem] text-muted-foreground tabular-nums">
            {t("dashboard.hero.capOf", {
              cap: formatBytes(tr.cap.bytes),
              pct: share.toFixed(0),
            })}
            {LEVEL_VARIANT[level] && level !== "ok" && (
              <Badge variant={LEVEL_VARIANT[level]}>
                {t(`dashboard.hero.level.${level}`)}
              </Badge>
            )}
          </span>
        </div>
      ) : (
        <span className="text-[0.7rem] text-muted-foreground">
          {t("dashboard.hero.noCap")}
        </span>
      )}
      {(tr.cap.cut_active || !tr.clock) && (
        <div className="flex flex-wrap gap-1">
          {tr.cap.cut_active && (
            <Badge variant="destructive">{t("dashboard.hero.cutActive")}</Badge>
          )}
          {!tr.clock && (
            <Badge variant="outline">{t("dashboard.hero.noClock")}</Badge>
          )}
        </div>
      )}
    </>
  );
}

function System({ s }: { s: SystemStatusInfo }) {
  const points = useSeries("system");
  const cpu = points.at(-1)?.cpu ?? null;
  const { total_kb, avail_kb } = s.memory;
  const mem =
    total_kb && avail_kb != null
      ? ((total_kb - avail_kb) / total_kb) * 100
      : null;
  const temp = s.temperatures.hottest ?? s.temperatures.cpu;
  return (
    <>
      <div className="grid grid-cols-2 gap-x-2 gap-y-1.5">
        <Mini label={t("dashboard.hero.cpu")}>
          <AnimatedNumber value={cpu} format={pct} />
        </Mini>
        <Mini label={t("dashboard.hero.memory")}>
          <AnimatedNumber value={mem} format={pct} />
        </Mini>
        <Mini label={t("dashboard.hero.temp")}>
          <span className={cn(s.heat_alarm && "text-destructive")}>
            <AnimatedNumber value={temp} format={celsius} />
          </span>
        </Mini>
        <Mini label={t("dashboard.hero.uptime")}>
          {formatDuration(s.uptime)}
        </Mini>
      </div>
      <div className="flex flex-col gap-1">
        <Progress
          value={cpu ?? 0}
          aria-label={t("dashboard.hero.cpu")}
          className="h-1"
        />
        <Progress
          value={mem ?? 0}
          aria-label={t("dashboard.hero.memory")}
          className="h-1"
        />
      </div>
    </>
  );
}

/** The four figures people look at first, in one strip: the network, the speed, the usage, the device. */
export function HeroCard({ s }: { s: DashboardStatus }) {
  const ok = <T,>(p: { ok: true; data: T } | { ok: false } | undefined) =>
    p?.ok ? p.data : null;
  const modem = ok(s.modem);
  const traffic = ok(s.traffic);
  const system = ok(s.system);
  const dash = (
    <p className="text-xs text-muted-foreground">
      {t("common.state.readFailed")}
    </p>
  );
  return (
    <Card className="grid gap-0 divide-y py-0 sm:grid-cols-2 sm:divide-y-0 xl:grid-cols-4 xl:divide-x [&>section]:border-border sm:[&>section:nth-child(n+3)]:border-t xl:[&>section:nth-child(n+3)]:border-t-0 sm:[&>section:nth-child(even)]:border-s xl:[&>section:nth-child(even)]:border-s-0">
      <Section
        icon={SignalIcon}
        title={t("dashboard.hero.mobile")}
        to="/cellular/signal"
      >
        {modem ? <Mobile m={modem} /> : dash}
      </Section>
      <Section
        icon={GaugeIcon}
        title={t("dashboard.hero.rate")}
        to="/cellular/data"
      >
        <Rate />
      </Section>
      <Section
        icon={PieChartIcon}
        title={t("dashboard.hero.usage")}
        to="/cellular/traffic"
        aside={
          traffic && (
            <span className="tabular-nums">
              {t("dashboard.hero.daysLeft", { n: traffic.cycle.days_left })}
            </span>
          )
        }
      >
        {traffic ? <Usage tr={traffic} /> : dash}
      </Section>
      <Section
        icon={CpuIcon}
        title={t("dashboard.hero.system")}
        to="/system/overview"
        aside={
          system?.heat_alarm && (
            <Badge variant="destructive">{t("dashboard.hero.hot")}</Badge>
          )
        }
      >
        {system ? <System s={system} /> : dash}
      </Section>
    </Card>
  );
}

import { useState } from "react";
import { Link } from "react-router-dom";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { AnimatedSegmentedTabs } from "@/components/ui/animated-segmented-tabs";
import { Card } from "@/components/ui/card";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { ArrowDownIcon, ArrowUpIcon, ChevronRightIcon } from "lucide-react";
import { NodeFlag } from "./node-flag";
import { probeNode } from "../api";
import { routingHint, routingLabel, ROUTINGS } from "../lib/configs";
import { plainName } from "../lib/country";
import {
  allNodes,
  clockDuration,
  latencyTone,
  rateText,
  TONE,
} from "../lib/format";
import { type Rate, useRates } from "../lib/rates";
import { useProxyStatus, useSetProxy } from "../queries";
import type { Profile, Routing } from "../types";

type Probe =
  | { state: "pending" }
  | { state: "ok"; ms: number }
  | { state: "fail"; error: string }
  | null;

/** The last minute of download as a faint area behind the status: the traffic, without a chart of its own. */
function Sparkline({ rates }: { rates: Rate[] }) {
  if (rates.length < 2) return null;
  const max = Math.max(1, ...rates.map((r) => Math.max(r.down, r.up)));
  const w = 60;
  const x = (i: number) => w - (rates.length - 1 - i);
  const y = (v: number) => 20 - (v / max) * 18;
  const line = (k: "down" | "up") =>
    rates.map((r, i) => `${i ? "L" : "M"}${x(i)},${y(r[k])}`).join("");
  return (
    <svg
      aria-hidden
      viewBox={`0 0 ${w} 20`}
      preserveAspectRatio="none"
      className="pointer-events-none absolute inset-x-0 bottom-0 h-3/5 w-full"
    >
      <path
        d={`${line("down")}L${x(rates.length - 1)},20L${x(0)},20Z`}
        className="fill-primary/[0.07]"
      />
      <path
        d={line("down")}
        className="fill-none stroke-primary/40"
        strokeWidth={1}
        vectorEffect="non-scaling-stroke"
      />
      <path
        d={line("up")}
        className="fill-none stroke-chart-3/40"
        strokeWidth={1}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

/**
 * While running: how long, the delay through the node (the engine's own check, or a click's real request), and the
 * speeds.
 */
function LiveLine({
  now,
  probe,
  onTest,
}: {
  now: number;
  probe: Probe;
  onTest: () => void;
}) {
  const { data: s } = useProxyStatus();
  const rate = useRates().at(-1) ?? { up: 0, down: 0 };
  if (!s) return null;
  const h = s.health;
  const problem = h
    ? !h.online
      ? t("proxy.hero.offline")
      : !h.node_ok
        ? t("proxy.hero.nodeDown")
        : null
    : null;
  const delay =
    probe?.state === "pending" ? (
      <SweepShine>{t("proxy.node.testing")}</SweepShine>
    ) : probe?.state === "ok" ? (
      <span className={latencyTone(probe.ms)}>{probe.ms} ms</span>
    ) : probe?.state === "fail" ? (
      <span className="text-destructive">{t("proxy.node.timeout")}</span>
    ) : problem ? (
      <span className={h?.online ? TONE.warn : "text-destructive"}>
        {problem}
      </span>
    ) : h?.delay_ms ? (
      `${h.delay_ms} ms`
    ) : (
      t("proxy.hero.testRun")
    );
  return (
    <span className="flex flex-wrap items-center gap-x-3 gap-y-0.5">
      {s.since && <span>{clockDuration(now - s.since * 1000)}</span>}
      <button
        type="button"
        onClick={onTest}
        disabled={probe?.state === "pending"}
        title={
          probe?.state === "fail"
            ? probe.error
            : `${t("proxy.hero.test")} · ${t("proxy.hero.healthHint")}`
        }
        className="-mx-1 flex items-center gap-1 rounded px-1 outline-none hover:bg-foreground/[0.06] focus-visible:ring-2 focus-visible:ring-ring/50"
      >
        <span
          className={cn(
            "size-1.5 rounded-full",
            problem || probe?.state === "fail" ? "bg-destructive" : TONE.goodBg,
          )}
        />
        {delay}
      </button>
      <span className="flex items-center gap-0.5">
        <ArrowDownIcon className="size-3 text-primary" />
        {rateText(rate.down)}
      </span>
      <span className="flex items-center gap-0.5">
        <ArrowUpIcon className="size-3 text-chart-3" />
        {rateText(rate.up)}
      </span>
    </span>
  );
}

/**
 * The page's top strip, two cells: the switch on the node in use (its live line carries the speeds, the delay — a
 * click sends one real request through the node — and the last minute's traffic behind it); then the 分流 mode with
 * the config it uses.
 */
export function ConnectHero({ profile }: { profile: Profile }) {
  const status = useProxyStatus();
  const s = status.data;
  const set = useSetProxy();
  const rates = useRates();
  const [probe, setProbe] = useState<Probe>(null);

  const selected = allNodes(profile).find((n) => n.id === profile.selected);
  const enabled = s?.enabled ?? profile.enabled;
  const state = s?.state ?? (profile.enabled ? "starting" : "off");
  const routing = s?.routing ?? profile.routing;
  const config =
    profile.configs.find(
      (c) => c.id === (s?.config ?? profile.active_config),
    ) ?? profile.configs[0];
  const busy = set.isPending;
  const running = state === "running";

  const toggle = (on: boolean) => set.mutate({ enabled: on });
  const pickRouting = (r: Routing) =>
    r !== routing && set.mutate({ routing: r });
  const test = async () => {
    if (!selected || probe?.state === "pending") return;
    setProbe({ state: "pending" });
    try {
      const { ms } = await probeNode(selected.id);
      setProbe({ state: "ok", ms });
    } catch (e) {
      setProbe({
        state: "fail",
        error: e instanceof Error ? e.message : String(e),
      });
    }
  };

  let caption: React.ReactNode;
  if (busy)
    caption = (
      <SweepShine>
        {enabled ? t("proxy.hero.stopping") : t("proxy.hero.starting")}
      </SweepShine>
    );
  else if (state === "starting")
    caption = <SweepShine>{t("proxy.hero.starting")}</SweepShine>;
  else if (running)
    caption = (
      <LiveLine
        now={status.dataUpdatedAt}
        probe={probe}
        onTest={() => void test()}
      />
    );
  else if (state === "error")
    caption = (
      <span className="text-destructive">{t("proxy.state.error")}</span>
    );
  else caption = selected ? t("proxy.state.off") : t("proxy.hero.pickNode");

  const nodeName = (running && s?.node?.name) || selected?.name;

  return (
    <div className="flex flex-col gap-3">
      <Card className="grid gap-px overflow-hidden bg-border py-0 md:grid-cols-[minmax(0,1fr)_auto]">
        <div className="relative flex min-h-16 items-center gap-3 bg-card px-4 py-3">
          {running && <Sparkline rates={rates} />}
          {nodeName ? <NodeFlag name={nodeName} className="relative" /> : null}
          <div className="relative flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="truncate text-sm font-medium">
              {nodeName ? plainName(nodeName) : t("proxy.hero.noNode")}
            </span>
            <span
              aria-live="polite"
              className="text-xs text-muted-foreground tabular-nums"
            >
              {caption}
            </span>
          </div>
          <Switch
            className="relative"
            aria-label={
              enabled ? t("proxy.hero.turnOff") : t("proxy.hero.turnOn")
            }
            checked={busy ? !enabled : enabled}
            disabled={busy || (!enabled && !selected)}
            onCheckedChange={toggle}
          />
        </div>
        <div className="flex flex-col justify-center gap-1.5 bg-card px-4 py-3">
          <AnimatedSegmentedTabs
            label={t("proxy.hero.routing")}
            value={routing}
            onValueChange={pickRouting}
            options={ROUTINGS.map((r) => ({
              value: r,
              label: routingLabel(r),
              disabled: busy,
            }))}
            triggerClassName="px-2.5"
          />
          {routing === "rule" ? (
            <Link
              to="/proxy/rules"
              title={t("proxy.hero.config")}
              className="flex items-center gap-0.5 self-start rounded px-1 text-xs text-muted-foreground outline-none hover:text-primary focus-visible:ring-2 focus-visible:ring-ring/50"
            >
              {config?.name}
              <ChevronRightIcon className="size-3" />
            </Link>
          ) : (
            <span className="px-1 text-xs text-muted-foreground">
              {routingHint(routing)}
            </span>
          )}
        </div>
      </Card>
      {state === "error" && s?.error && (
        <Alert variant="destructive">
          <AlertTitle>{t("proxy.hero.errorTitle")}</AlertTitle>
          <AlertDescription>{s.error}</AlertDescription>
        </Alert>
      )}
    </div>
  );
}

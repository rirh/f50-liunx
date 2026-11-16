import { Link } from "react-router-dom";
import { FillMeter } from "@/components/f50/fill-meter";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { routingLabel, ROUTING_ICON } from "@/views/proxy/lib/configs";
import {
  allNodes,
  clockDuration,
  latencyTone,
  rateText,
} from "@/views/proxy/lib/format";
import { useRates } from "@/views/proxy/lib/rates";
import { useProfile, useProxyStatus, useSetProxy } from "@/views/proxy/queries";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  ChevronRightIcon,
  GaugeIcon,
  PlugZapIcon,
  SigmaIcon,
} from "lucide-react";

/**
 * The proxy at a glance, under the device card: the node and state with the on/off switch, then the speeds, the
 * health check's delay and the traffic of this run as tiles. Absent where the device has no `f50.proxy`.
 */
export function ProxyCard() {
  const status = useProxyStatus();
  const profile = useProfile();
  const set = useSetProxy();
  const rate = useRates().at(-1) ?? null;
  const s = status.data;
  const p = profile.data;
  if (!s && !p) return null;

  const selected = p ? allNodes(p).find((n) => n.id === p.selected) : undefined;
  const enabled = s?.enabled ?? p?.enabled ?? false;
  const state = s?.state ?? (enabled ? "starting" : "off");
  const routing = s?.routing ?? p?.routing ?? "rule";
  const running = state === "running";
  const busy = set.isPending;
  const nodeName = (running && s?.node?.name) || selected?.name;
  const RoutingIcon = ROUTING_ICON[routing];

  const h = running ? s?.health : null;
  const problem = h
    ? !h.online
      ? t("proxy.hero.offline")
      : !h.node_ok
        ? t("proxy.hero.nodeDown")
        : null
    : null;

  let caption: React.ReactNode;
  if (busy)
    caption = (
      <SweepShine>
        {enabled ? t("proxy.hero.stopping") : t("proxy.hero.starting")}
      </SweepShine>
    );
  else if (state === "starting")
    caption = <SweepShine>{t("proxy.hero.starting")}</SweepShine>;
  else if (state === "error")
    caption = (
      <span className="text-destructive" title={s?.error ?? undefined}>
        {t("proxy.state.error")}
      </span>
    );
  else if (running)
    caption = s?.since
      ? t("dashboard.proxy.runningFor", {
          time: clockDuration(status.dataUpdatedAt - s.since * 1000),
        })
      : t("proxy.state.running");
  else caption = t("proxy.state.off");

  const dash = "—";
  return (
    <article className="flex flex-col gap-3 rounded-2xl border bg-card p-4 text-card-foreground">
      <div className="flex min-w-0 items-center gap-2">
        <Link
          to="/proxy"
          className="group flex min-w-0 flex-1 items-center gap-2"
        >
          <PlugZapIcon
            aria-hidden
            className={cn(
              "size-5 shrink-0",
              running ? "text-primary" : "text-muted-foreground",
            )}
          />
          <span className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="flex min-w-0 items-baseline gap-1.5">
              <span className="truncate text-sm font-semibold group-hover:text-primary">
                {nodeName ?? t("proxy.hero.noNode")}
              </span>
              <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                <RoutingIcon aria-hidden className="size-3" />
                {routingLabel(routing)}
              </span>
            </span>
            <span
              aria-live="polite"
              className="truncate text-xs text-muted-foreground tabular-nums"
            >
              {caption}
            </span>
          </span>
          <ChevronRightIcon
            aria-hidden
            className="size-4 shrink-0 text-muted-foreground/60 transition-transform group-hover:translate-x-0.5"
          />
        </Link>
        <Switch
          aria-label={
            enabled ? t("proxy.hero.turnOff") : t("proxy.hero.turnOn")
          }
          checked={busy ? !enabled : enabled}
          disabled={busy || (!enabled && !selected)}
          onCheckedChange={(on) => set.mutate({ enabled: on })}
        />
      </div>
      <div className="grid grid-cols-2 gap-2">
        <FillMeter
          icon={ArrowDownIcon}
          label={t("dashboard.proxy.down")}
          value={running && rate ? rateText(rate.down) : dash}
          percent={null}
          hint={
            running && s
              ? t("dashboard.proxy.total", { size: formatBytes(s.down) })
              : undefined
          }
        />
        <FillMeter
          icon={ArrowUpIcon}
          label={t("dashboard.proxy.up")}
          value={running && rate ? rateText(rate.up) : dash}
          percent={null}
          hint={
            running && s
              ? t("dashboard.proxy.total", { size: formatBytes(s.up) })
              : undefined
          }
        />
        <FillMeter
          icon={GaugeIcon}
          label={t("dashboard.proxy.delay")}
          value={
            problem ? (
              <span className="text-destructive">{problem}</span>
            ) : h?.delay_ms ? (
              <span className={latencyTone(h.delay_ms)}>{h.delay_ms} ms</span>
            ) : h ? (
              t("common.state.ok")
            ) : (
              dash
            )
          }
          percent={null}
          hint={h ? t("proxy.hero.healthHint") : undefined}
        />
        <FillMeter
          icon={SigmaIcon}
          label={t("dashboard.proxy.traffic")}
          value={running && s ? formatBytes(s.up + s.down) : dash}
          percent={null}
          hint={
            running && s?.groups.length
              ? t("dashboard.proxy.groups", { n: s.groups.length })
              : undefined
          }
        />
      </div>
    </article>
  );
}

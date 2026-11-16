import { Group, Row } from "@/components/f50/grouped-list";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { ChevronRightIcon } from "lucide-react";
import { TrafficChart } from "./traffic-chart";
import { routingLabel } from "../../lib/configs";
import {
  allNodes,
  byteParts,
  clockDuration,
  subscriptionTitle,
  TONE,
  usageParts,
} from "../../lib/format";
import { useRates } from "../../lib/rates";
import { useConnections, useProfile, useProxyStatus } from "../../queries";

function Stat({
  label,
  dot,
  rate,
  total,
}: {
  label: string;
  dot: string;
  rate: number;
  total: number;
}) {
  const r = byteParts(rate);
  const tt = byteParts(total);
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <span className={cn("size-1.5 rounded-full", dot)} />
        {label}
      </span>
      <span className="flex items-baseline gap-1">
        <span className="text-2xl font-semibold tabular-nums">{r.value}</span>
        <span className="text-sm text-muted-foreground">{r.unit}/s</span>
      </span>
      <span className="text-xs text-muted-foreground tabular-nums">
        {t("proxy.data.traffic.total", { value: `${tt.value} ${tt.unit}` })}
      </span>
    </div>
  );
}

/** 流量: the speeds and totals since the start with the last minute's chart, the run, and subscription usage. */
export function TrafficView({ onConnections }: { onConnections: () => void }) {
  const { data: s, dataUpdatedAt } = useProxyStatus({ fast: true });
  const { data: profile } = useProfile();
  const rates = useRates();
  const running = s?.state === "running";
  const { data: conns } = useConnections({ paused: !running });
  const rate = running
    ? (rates.at(-1) ?? { up: 0, down: 0 })
    : { up: 0, down: 0 };
  const config = profile?.configs.find((c) => c.id === s?.config);
  const usages = (profile?.subscriptions ?? []).filter((x) => x.usage);
  const node =
    profile && s?.node
      ? allNodes(profile).find((n) => n.id === s.node?.id)
      : undefined;

  return (
    <div className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,22rem)]">
      <Card>
        <CardHeader>
          <CardTitle>{t("proxy.data.traffic.title")}</CardTitle>
          <CardDescription>{t("proxy.data.traffic.hint")}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="grid grid-cols-2 gap-4">
            <Stat
              label={t("proxy.data.traffic.down")}
              dot="bg-primary"
              rate={rate.down}
              total={running ? s.down : 0}
            />
            <Stat
              label={t("proxy.data.traffic.up")}
              dot="bg-chart-3"
              rate={rate.up}
              total={running ? s.up : 0}
            />
          </div>
          <TrafficChart rates={running ? rates : []} now={dataUpdatedAt} />
          <p className="text-xs text-muted-foreground">
            {running ? t("proxy.data.traffic.window") : t("proxy.state.off")}
          </p>
        </CardContent>
      </Card>
      <div className="flex flex-col gap-6">
        <Group title={t("proxy.data.run.title")}>
          <Row label={t("proxy.data.run.state")}>
            <span className="inline-flex items-center gap-1.5">
              <span
                className={cn(
                  "size-1.5 rounded-full",
                  running ? TONE.goodBg : "bg-muted-foreground/40",
                )}
              />
              {s ? t(`proxy.state.${s.state}`) : "—"}
            </span>
          </Row>
          {running && (
            <>
              <Row label={t("proxy.data.run.node")}>{s.node?.name ?? "—"}</Row>
              {node && (
                <Row label={t("proxy.data.run.protocol")}>{node.summary}</Row>
              )}
              <Row label={t("proxy.data.run.routing")}>
                {routingLabel(s.routing)}
              </Row>
              {s.routing === "rule" && (
                <Row label={t("proxy.data.run.config")}>
                  {config?.name ?? s.config}
                </Row>
              )}
              <Row label={t("proxy.data.run.uptime")}>
                {s.since ? clockDuration(dataUpdatedAt - s.since * 1000) : "—"}
              </Row>
              <Row label={t("proxy.data.run.router")}>
                {s.intercept_router
                  ? t("common.state.enabled")
                  : t("common.state.disabled")}
              </Row>
              <Button
                variant="ghost"
                className="h-auto min-h-9 w-full justify-between rounded-none px-3 py-1.5 font-normal"
                onClick={onConnections}
              >
                <span className="text-muted-foreground">
                  {t("proxy.data.run.active")}
                </span>
                <span className="flex items-center gap-1 tabular-nums">
                  {conns?.active.length ?? 0}
                  <ChevronRightIcon className="text-muted-foreground" />
                </span>
              </Button>
            </>
          )}
        </Group>
        <Group title={t("proxy.data.engine.title")}>
          <Row label={t("proxy.data.engine.mem")}>
            {s?.mem ? formatBytes(s.mem) : "—"}
          </Row>
          <Row label="Xray">{s?.xray || "—"}</Row>
          <Row label={t("proxy.data.engine.version")}>{s?.engine || "—"}</Row>
        </Group>
        {usages.map((sub) => {
          const u = usageParts(sub.usage!);
          return (
            <Group
              key={sub.id}
              title={t("proxy.data.usage.title", {
                name: subscriptionTitle(sub),
              })}
            >
              <Row label={t("proxy.data.usage.used")}>{u.used}</Row>
              <Row label={t("proxy.data.usage.total")}>{u.total}</Row>
              <Row label={t("proxy.data.usage.expire")}>
                {u.expire ?? t("proxy.sub.noExpiry")}
              </Row>
            </Group>
          );
        })}
      </div>
    </div>
  );
}

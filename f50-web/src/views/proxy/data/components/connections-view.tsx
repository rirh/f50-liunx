import { useMemo, useState } from "react";
import { ConfirmButton } from "@/components/f50/confirm-button";
import { DataTable } from "@/components/f50/data-table";
import { AnimatedSegmentedTabs } from "@/components/ui/animated-segmented-tabs";
import { Button } from "@/components/ui/button";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { columnHelper, type TableColumn } from "@/lib/table";
import { cn } from "@/lib/utils";
import { PauseIcon, PlayIcon, SearchIcon, XIcon } from "lucide-react";
import { formatAge, formatClock, rateText } from "../../lib/format";
import {
  type ConnectionsSnapshot,
  useCloseConnections,
  useConnections,
} from "../../queries";
import type { Conn } from "../../types";

type Scope = "active" | "closed";
type Route = "proxy" | "direct" | "block";
const ROUTES: Route[] = ["proxy", "direct", "block"];
const ROUTE_BAR: Record<Route, string> = {
  proxy: "bg-primary",
  direct: "bg-muted-foreground/40",
  block: "bg-destructive",
};
const ROUTE_TEXT: Record<Route, string> = {
  proxy: "text-primary",
  direct: "text-muted-foreground",
  block: "text-destructive",
};
const ROWS_SHOWN = 300;
const PROTOCOLS: Record<string, string> = {
  tls: "TLS",
  http1: "HTTP",
  quic: "QUIC",
  bittorrent: "BT",
};

/** proxy | direct | block; `node:<name>` (a rule's node or a 策略组's pick) counts as proxy. */
const routeOf = (c: Conn): Route =>
  c.outbound === "direct" || c.outbound === "block" ? c.outbound : "proxy";

/** Where it went, for people: 直连, 拦截, the node or group by name, else 代理. */
function routeText(c: Conn) {
  if (c.outbound === "direct") return t("proxy.outbound.direct");
  if (c.outbound === "block") return t("proxy.outbound.block");
  const node = c.outbound.startsWith("node:") ? c.outbound.slice(5) : null;
  if (c.group) return node ? `${c.group} · ${node}` : c.group;
  return node ?? t("proxy.data.conns.proxied");
}

/** Share of connections per route as one bar; each legend entry filters the list. */
function RouteBreakdown({
  conns,
  route,
  onRoute,
}: {
  conns: Conn[];
  route: Route | null;
  onRoute: (r: Route | null) => void;
}) {
  const counts = ROUTES.map((key) => ({
    key,
    n: conns.filter((c) => routeOf(c) === key).length,
  })).filter((r) => r.n > 0);
  const total = conns.length || 1;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex h-1.5 gap-px overflow-hidden rounded-full bg-muted">
        {counts.map(({ key, n }) => (
          <span
            key={key}
            style={{ width: `${(n / total) * 100}%` }}
            className={cn(
              "h-full transition-[width,opacity] duration-500 ease-out",
              ROUTE_BAR[key],
              route && route !== key && "opacity-30",
            )}
          />
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-1">
        {counts.map(({ key, n }) => (
          <button
            key={key}
            type="button"
            aria-pressed={route === key}
            onClick={() => onRoute(route === key ? null : key)}
            className={cn(
              "flex h-6 items-center gap-1.5 rounded-full px-2.5 text-xs text-muted-foreground outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50",
              route === key && "bg-muted text-foreground",
            )}
          >
            <span className={cn("size-1.5 rounded-full", ROUTE_BAR[key])} />
            {t(`proxy.data.conns.route.${key}`)}
            <span className="tabular-nums">{n}</span>
          </button>
        ))}
        {route && (
          <Button
            variant="link"
            size="xs"
            className="ms-auto"
            onClick={() => onRoute(null)}
          >
            {t("proxy.data.conns.showAll")}
          </Button>
        )}
      </div>
    </div>
  );
}

const col = columnHelper<Conn>();

function columns(
  snap: ConnectionsSnapshot | undefined,
  onClose: ((id: number) => void) | null,
): TableColumn<Conn>[] {
  const now = snap?.at ?? 0;
  const out: TableColumn<Conn>[] = [
    col.accessor((c) => c.host, {
      id: "host",
      header: () => t("proxy.data.conns.col.host"),
      cell: ({ row: { original: c } }) => {
        const meta = [
          c.network.toUpperCase(),
          c.protocol && (PROTOCOLS[c.protocol] ?? c.protocol.toUpperCase()),
          c.ip && c.ip !== c.host ? c.ip : null,
        ].filter(Boolean);
        return (
          <div className="flex max-w-[22rem] min-w-40 flex-col gap-0.5">
            <span className="flex min-w-0 items-baseline">
              <span className="truncate font-medium select-text">
                {c.host || "—"}
              </span>
              <span className="shrink-0 text-muted-foreground tabular-nums">
                :{c.port}
              </span>
            </span>
            <span className="truncate text-xs text-muted-foreground tabular-nums">
              {meta.join(" · ")}
            </span>
          </div>
        );
      },
    }),
    col.accessor((c) => c.source, {
      id: "source",
      header: () => t("proxy.data.conns.col.source"),
      cell: (c) => (
        <span className="text-xs tabular-nums">{c.getValue() || "—"}</span>
      ),
    }),
    col.accessor((c) => routeText(c), {
      id: "route",
      header: () => t("proxy.data.conns.col.route"),
      cell: ({ row: { original: c } }) => (
        <span
          className={cn("text-xs whitespace-nowrap", ROUTE_TEXT[routeOf(c)])}
        >
          {routeText(c)}
        </span>
      ),
    }),
    col.accessor((c) => c.up + c.down, {
      id: "traffic",
      header: () => t("proxy.data.conns.col.traffic"),
      sortDescFirst: true,
      cell: ({ row: { original: c } }) => {
        const speed = snap?.rates.get(c.id);
        const moving = c.end === 0 && speed && speed.down + speed.up >= 1;
        return (
          <div className="flex flex-col gap-0.5 text-xs whitespace-nowrap tabular-nums">
            <span>
              ↓ {formatBytes(c.down)} ↑ {formatBytes(c.up)}
            </span>
            {moving && (
              <span className="text-primary">
                {speed.down >= speed.up
                  ? `↓ ${rateText(speed.down)}`
                  : `↑ ${rateText(speed.up)}`}
              </span>
            )}
          </div>
        );
      },
    }),
    col.accessor((c) => c.start, {
      id: "time",
      header: () => t("proxy.data.conns.col.time"),
      sortDescFirst: true,
      cell: ({ row: { original: c } }) => (
        <span className="text-xs whitespace-nowrap text-muted-foreground tabular-nums">
          {c.end === 0
            ? formatAge(now - c.start)
            : `${formatClock(c.end)} · ${formatAge(c.end - c.start)}`}
        </span>
      ),
    }),
  ];
  if (onClose)
    out.push(
      col.display({
        id: "close",
        header: () => (
          <span className="sr-only">{t("proxy.data.conns.close")}</span>
        ),
        cell: ({ row: { original: c } }) => (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={t("proxy.data.conns.closeOne", { host: c.host })}
            onClick={() => onClose(c.id)}
            className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
          >
            <XIcon />
          </Button>
        ),
      }),
    );
  return out;
}

/** 连接: open and recently closed connections with the client, the route, search, sort, close one or all. */
export function ConnectionsView() {
  const [scope, setScope] = useState<Scope>("active");
  const [query, setQuery] = useState("");
  const [route, setRoute] = useState<Route | null>(null);
  const [paused, setPaused] = useState(false);
  const { data, isPending } = useConnections({ paused });
  const close = useCloseConnections();

  const pool = useMemo(
    () => (scope === "active" ? (data?.active ?? []) : (data?.closed ?? [])),
    [data, scope],
  );
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    const out = pool.filter(
      (c) =>
        (!route || routeOf(c) === route) &&
        (!q ||
          c.host.toLowerCase().includes(q) ||
          c.ip?.includes(q) ||
          c.source?.includes(q) ||
          String(c.port) === q),
    );
    // the engine lists open ones oldest first; closed ones are newest first already
    if (scope === "active") out.reverse();
    return out.slice(0, ROWS_SHOWN);
  }, [pool, query, route, scope]);
  const { mutate } = close;
  const cols = useMemo(
    () => columns(data, scope === "active" ? (id) => mutate(id) : null),
    [data, scope, mutate],
  );
  const activeCount = data?.active.length ?? 0;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <AnimatedSegmentedTabs
          label={t("proxy.data.conns.scope")}
          value={scope}
          onValueChange={(s) => {
            setScope(s);
            setRoute(null);
          }}
          triggerClassName="px-2.5"
          options={[
            {
              value: "active",
              label: (
                <>
                  {t("proxy.data.conns.active")}{" "}
                  <span className="tabular-nums opacity-60">{activeCount}</span>
                </>
              ),
            },
            {
              value: "closed",
              label: (
                <>
                  {t("proxy.data.conns.closed")}{" "}
                  <span className="tabular-nums opacity-60">
                    {data?.closed.length ?? 0}
                  </span>
                </>
              ),
            },
          ]}
        />
        <InputGroup className="w-full min-w-40 flex-1 sm:w-auto">
          <InputGroupAddon>
            <SearchIcon />
          </InputGroupAddon>
          <InputGroupInput
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("proxy.data.conns.search")}
            aria-label={t("common.action.search")}
          />
        </InputGroup>
        <div className="flex items-center gap-1">
          <Button
            variant="outline"
            size="sm"
            aria-pressed={paused}
            onClick={() => setPaused(!paused)}
          >
            {paused ? (
              <PlayIcon data-icon="inline-start" />
            ) : (
              <PauseIcon data-icon="inline-start" />
            )}
            {paused ? t("proxy.data.resume") : t("proxy.data.pause")}
          </Button>
          <ConfirmButton
            size="sm"
            title={t("proxy.data.conns.closeAllTitle")}
            description={t("proxy.data.conns.closeAllBody", {
              count: activeCount,
            })}
            confirmText={t("proxy.data.conns.closeAll")}
            disabled={!activeCount}
            pending={close.isPending && close.variables === 0}
            onConfirm={() => mutate(0)}
          >
            {t("proxy.data.conns.closeAll")}
          </ConfirmButton>
        </div>
      </div>
      {pool.length > 0 && (
        <RouteBreakdown conns={pool} route={route} onRoute={setRoute} />
      )}
      <DataTable
        key={`${scope}-${data?.epoch ?? ""}`}
        data={rows}
        columns={cols}
        getRowId={(c) => String(c.id)}
        empty={
          isPending
            ? t("app.loading")
            : query || route
              ? t("proxy.data.conns.noMatch")
              : scope === "active"
                ? t("proxy.data.conns.none")
                : t("proxy.data.conns.noneClosed")
        }
      />
      <p className="text-xs text-muted-foreground">
        {paused
          ? t("proxy.data.paused")
          : scope === "closed"
            ? t("proxy.data.conns.closedKept")
            : pool.length > ROWS_SHOWN
              ? t("proxy.data.conns.capped", { n: ROWS_SHOWN })
              : t("proxy.data.conns.live")}
      </p>
    </div>
  );
}

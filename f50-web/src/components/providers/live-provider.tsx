/**
 * The one `f50.live` stream of the signed-in app, and the chart histories.
 *
 * - Each event goes into the query cache under the key the page that shows it reads, so a page renders pushed and
 *   polled figures alike; while the stream is live, those pages stop polling (`usePoll`).
 * - A new unread count means a message came or was read: the conversation list is fetched again at once.
 * - The stream reconnects with a growing pause (1 s to 30 s). A refused subscription (a login older than the
 *   `:subscribe` grant) is tried again only every 5 minutes; the pages poll meanwhile.
 * - A stream alone never touches the rpcd session, which then expires after 5 idle minutes: one cheap read a minute
 *   keeps it alive.
 * - The histories (`lib/series`) are recorded from the cache, whichever way a reading arrived.
 */
import { type PropsWithChildren, useEffect } from "react";
import { setLiveState } from "@/hooks/use-live";
import { useSession } from "@/hooks/use-session";
import { type LiveEvent, readLive } from "@/lib/live";
import { append, resetSeries } from "@/lib/series";
import { f50 } from "@/lib/ubus";
import { signalKeys } from "@/views/cellular/signal/api";
import { STATUS_KEY } from "@/views/dashboard/api";
import type { DashboardStatus } from "@/views/dashboard/types";
import { smsKeys } from "@/views/sms/api";
import type { UnreadCounts } from "@/views/sms/types";
import { systemKeys } from "@/views/system/overview/api";
import type { CpuTimes, SystemStatus } from "@/views/system/overview/types";
import { cpuPercent, memPercent } from "@/views/system/overview/utils";
import { type QueryClient, useQueryClient } from "@tanstack/react-query";

const TOPIC_KEYS: Record<string, readonly unknown[]> = {
  status: STATUS_KEY,
  system: systemKeys.status,
  modem: signalKeys.status,
  cells: signalKeys.cells,
  sms: smsKeys.unread,
};

const BACKOFF_MIN = 1000;
const BACKOFF_MAX = 30_000;
const DENIED_RETRY = 5 * 60_000;
const KEEPALIVE = 60_000;

interface ModemLike {
  signal: {
    rsrp: number | null;
    rsrq: number | null;
    sinr: number | null;
  } | null;
}

interface DataLike {
  bytes?: { rx: number; tx: number } | null;
}

/** Records the chart points from the cache's successful updates. */
function recordSeries(qc: QueryClient) {
  let prevLoad: CpuTimes | null = null;
  let prevBytes: { rx: number; tx: number; t: number } | null = null;

  const system = (s: SystemStatus) => {
    const t = (s.ts ?? Math.floor(Date.now() / 1000)) * 1000;
    const cpu = cpuPercent(prevLoad, s.load);
    if (s.load) prevLoad = s.load;
    append("system", {
      t,
      cpu,
      mem: memPercent(s.memory),
      temp: s.temperatures?.hottest ?? null,
    });
  };
  const signal = (m: ModemLike) => {
    append("signal", {
      t: Date.now(),
      rsrp: m.signal?.rsrp ?? null,
      rsrq: m.signal?.rsrq ?? null,
      sinr: m.signal?.sinr ?? null,
    });
  };
  const rate = (d: DataLike) => {
    const b = d.bytes;
    if (!b) return;
    const now = Date.now();
    const prev = prevBytes;
    prevBytes = { ...b, t: now };
    // a counter that went back is a new connection: start again from here
    if (!prev || b.rx < prev.rx || b.tx < prev.tx) return;
    const dt = (now - prev.t) / 1000;
    if (dt < 0.5) return;
    append("rate", {
      t: now,
      rx: (b.rx - prev.rx) / dt,
      tx: (b.tx - prev.tx) / dt,
    });
  };

  return qc.getQueryCache().subscribe((e) => {
    if (e.type !== "updated" || e.action.type !== "success") return;
    const key = e.query.queryKey;
    const data = e.query.state.data;
    if (!data) return;
    if (same(key, systemKeys.status)) system(data as SystemStatus);
    else if (same(key, signalKeys.status)) signal(data as ModemLike);
    else if (same(key, STATUS_KEY)) {
      const s = data as DashboardStatus;
      if (s.data?.ok) rate(s.data.data as DataLike);
      if (s.modem?.ok) signal(s.modem.data as ModemLike);
      if (s.system?.ok) system(s.system.data as unknown as SystemStatus);
    }
  });
}

function same(a: readonly unknown[], b: readonly unknown[]) {
  return a.length === b.length && a.every((x, i) => x === b[i]);
}

function onEvent(qc: QueryClient, e: LiveEvent) {
  const key = TOPIC_KEYS[e.topic];
  const env = e.payload as { ok?: boolean; data?: unknown } | null;
  if (!key || !env?.ok) return;
  if (e.topic === "sms") {
    const before = qc.getQueryData<UnreadCounts>(smsKeys.unread);
    const now = env.data as UnreadCounts;
    if (before && (before.total !== now.total || before.unread !== now.unread))
      void qc.invalidateQueries({
        queryKey: smsKeys.all,
        refetchType: "active",
      });
  }
  qc.setQueryData(key, env.data);
}

/** Opens and keeps the stream while signed in. */
export function LiveProvider({ children }: PropsWithChildren) {
  const qc = useQueryClient();
  const session = useSession();
  const sid = session?.id;

  useEffect(() => recordSeries(qc), [qc]);

  useEffect(() => {
    if (!sid) {
      setLiveState("off");
      resetSeries();
      return;
    }
    const stop = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let keepalive: ReturnType<typeof setInterval> | undefined;
    const wait = (ms: number) =>
      new Promise<void>((resolve) => {
        timer = setTimeout(resolve, ms);
      });

    const loop = async () => {
      let backoff = BACKOFF_MIN;
      while (!stop.signal.aborted) {
        setLiveState("connecting");
        const attempt = new AbortController();
        const abort = () => attempt.abort();
        stop.signal.addEventListener("abort", abort);
        const end = await readLive(
          sid,
          attempt.signal,
          () => {
            setLiveState("live");
            backoff = BACKOFF_MIN;
          },
          (e) => onEvent(qc, e),
        );
        stop.signal.removeEventListener("abort", abort);
        if (stop.signal.aborted) return;
        if (end.kind === "denied") {
          setLiveState("denied");
          await wait(DENIED_RETRY);
        } else {
          setLiveState("retrying");
          await wait(backoff);
          backoff = Math.min(backoff * 2, BACKOFF_MAX);
        }
      }
    };
    void loop();
    keepalive = setInterval(() => {
      // any authorised call touches the session; this one is cheap and in every read grant
      void f50("f50.job", "list").catch(() => undefined);
    }, KEEPALIVE);

    return () => {
      stop.abort();
      clearTimeout(timer);
      clearInterval(keepalive);
      setLiveState("off");
    };
  }, [qc, sid]);

  return children;
}

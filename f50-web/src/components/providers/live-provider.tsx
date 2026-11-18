/**
 * The signed-in app's live stream and chart histories.
 *
 * - The topics come from the mounted pages (`usePoll`/`useLiveTopic`): they are leased with `f50.live.topics want`
 *   before the stream opens, whenever the set changes, and every 30 s (f50d holds a lease 2 minutes), and released when
 *   the provider goes. f50d then reads only those topics, once per period for every tab.
 * - Each event goes into the query cache under the key the page that shows it reads, so a page renders pushed and
 *   polled figures alike. A topic counts as pushed (its pages stop polling) while the stream is live, f50d granted it
 *   and a reading arrived over the stream within 30 s. Quiet and failed topics are polled while the rest stay
 *   pushed. Older firmware and sessions without the lease grant use the full `f50.live` stream.
 * - A new unread count means a message came or was read: the conversation list is fetched again at once.
 * - A new log fingerprint means the logs gained a line: the open log page reads them again at once.
 * - The stream reconnects with a growing pause (1 s to 30 s). A refused subscription (a login older than the
 *   `:subscribe` grant) is tried again only every 5 minutes; the pages poll meanwhile.
 * - A stream alone never touches the rpcd session, which then expires after 5 idle minutes: the lease renewal (or,
 *   for an f50d without leases, one cheap read a minute) keeps it alive.
 * - The histories (`lib/series`) are recorded from the cache, whichever way a reading arrived.
 */
import { type PropsWithChildren, useEffect } from "react";
import {
  onWantedChange,
  setLiveState,
  setPushedTopics,
  wantedTopics,
} from "@/hooks/use-live";
import { useSession } from "@/hooks/use-session";
import {
  LIVE_TOPICS_OBJECT,
  type LiveEvent,
  type LiveTopic,
  readLive,
} from "@/lib/live";
import { append, resetSeries } from "@/lib/series";
import { ApiError, f50, ubusCall } from "@/lib/ubus";
import { signalKeys } from "@/views/cellular/signal/api";
import { STATUS_KEY } from "@/views/dashboard/api";
import type { DashboardStatus } from "@/views/dashboard/types";
import { proxyKeys } from "@/views/proxy/api";
import { smsKeys } from "@/views/sms/api";
import type { UnreadCounts } from "@/views/sms/types";
import { logsAll, logsMarkKey } from "@/views/system/logs/api";
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
  logs: logsMarkKey,
  proxy: proxyKeys.status,
};

const BACKOFF_MIN = 1000;
const BACKOFF_MAX = 30_000;
const DENIED_RETRY = 5 * 60_000;
const KEEPALIVE = 60_000;
/** Lease renewal; f50d holds one for 2 minutes (a hidden tab's timers may fire only once a minute). */
const RENEW = 30_000;
/** A granted topic not heard from for this long is polled again (topics send at least every 15 s). */
const STALE = 30_000;
/** The pause before a changed topic set is leased (pages mount and unmount together on a route change). */
const WANT_DEBOUNCE = 50;
const WANT_TIMEOUT = 10_000;

interface WantReply {
  topics: LiveTopic[];
  ttl: number;
  data?: Record<string, unknown>;
}

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
  if (!Object.hasOwn(TOPIC_KEYS, e.topic)) return false;
  const key = TOPIC_KEYS[e.topic];
  const env = e.payload as { ok?: boolean; data?: unknown } | null;
  if (
    env?.ok !== true || !env.data || typeof env.data !== "object" ||
    Array.isArray(env.data)
  ) return false;
  if (e.topic === "sms") {
    const before = qc.getQueryData<UnreadCounts>(smsKeys.unread);
    const now = env.data as UnreadCounts;
    if (before && (before.total !== now.total || before.unread !== now.unread))
      void qc.invalidateQueries({
        queryKey: smsKeys.all,
        refetchType: "active",
      });
  }
  if (e.topic === "logs") {
    const before = qc.getQueryData<{ mark: string }>(logsMarkKey);
    const now = env.data as { mark: string };
    // the logs gained a line: the log page (if open) reads them again; nothing else is fetched
    if (before && before.mark !== now.mark)
      void qc.invalidateQueries({ queryKey: logsAll, refetchType: "active" });
  }
  qc.setQueryData(key, env.data);
  return true;
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
    let stopped = false;
    // Each provider owns a lease, even if React remounts it before the previous one has released its topics.
    const client = `tab-${Array.from({ length: 16 }, () => Math.floor(Math.random() * 36).toString(36)).join("")}`;
    let attempt: AbortController | undefined;
    let wake: (() => void) | undefined;

    // `null` means the legacy stream (all topics); otherwise only granted topics can pause polling.
    let granted: Set<LiveTopic> | null = new Set();
    const heard = new Map<LiveTopic, number>();
    const refresh = () => {
      const now = performance.now();
      const fed = wantedTopics().filter(
        (t) =>
          (granted === null || granted.has(t)) &&
          now - (heard.get(t) ?? -Infinity) < STALE,
      );
      setPushedTopics(fed);
    };
    const lease = async (topics: LiveTopic[]) => {
      const request = new AbortController();
      const deadline = setTimeout(() => request.abort(), WANT_TIMEOUT);
      try {
        const r = await f50<WantReply>(
          LIVE_TOPICS_OBJECT,
          "want",
          { client, topics: topics.join(",") },
          { session: sid, signal: request.signal },
        );
        if (stopped) return;
        granted = new Set(r.topics);
        for (const topic of heard.keys()) {
          if (!granted.has(topic)) heard.delete(topic);
        }
        // Seed an empty cache without replacing a more recent poll or push. A lease reply cannot prove SSE is healthy.
        for (const [topic, payload] of Object.entries(r.data ?? {})) {
          if (
            Object.hasOwn(TOPIC_KEYS, topic) &&
            granted.has(topic as LiveTopic) &&
            qc.getQueryData(TOPIC_KEYS[topic]) === undefined
          ) {
            onEvent(qc, { topic, payload });
          }
        }
      } catch (error) {
        if (stopped) return;
        if (
          error instanceof ApiError &&
          ["forbidden", "not-found", "unknown-method", "unsupported"].includes(error.code)
        ) {
          granted = null;
          heard.clear();
          attempt?.abort();
          wake?.();
        }
      } finally {
        clearTimeout(deadline);
      }
      refresh();
    };
    // Serialize updates for the same client and coalesce further changes while an update is in flight.
    let pending: Promise<void> | undefined;
    let syncAgain = false;
    const sync = () => {
      if (stopped || granted === null) return Promise.resolve();
      syncAgain = true;
      pending ??= (async () => {
        while (syncAgain && !stopped && granted !== null) {
          syncAgain = false;
          await lease(wantedTopics());
        }
      })().finally(() => {
        pending = undefined;
      });
      return pending;
    };
    let debounce: ReturnType<typeof setTimeout> | undefined;
    const offWanted = onWantedChange(() => {
      clearTimeout(debounce);
      debounce = setTimeout(() => void sync(), WANT_DEBOUNCE);
    });
    const renew = setInterval(() => void sync(), RENEW);
    const stale = setInterval(refresh, STALE / 3);
    const onVisible = () => {
      if (document.visibilityState === "visible") void sync();
    };
    document.addEventListener("visibilitychange", onVisible);
    const wait = (ms: number) =>
      new Promise<void>((resolve) => {
        const timer = setTimeout(() => wake?.(), ms);
        wake = () => {
          clearTimeout(timer);
          wake = undefined;
          resolve();
        };
      });

    const loop = async () => {
      let backoff = BACKOFF_MIN;
      while (!stopped) {
        setLiveState("connecting");
        // the lease first, so f50d starts only these topics when the subscription arrives
        await sync();
        if (stopped) return;
        const leasedStream = granted !== null;
        const stream = new AbortController();
        attempt = stream;
        const end = await readLive(
          sid,
          stream.signal,
          () => {
            if (stream.signal.aborted) return;
            setLiveState("live");
            backoff = BACKOFF_MIN;
          },
          (e) => {
            if (stream.signal.aborted) return;
            if (onEvent(qc, e)) heard.set(e.topic as LiveTopic, performance.now());
            else heard.delete(e.topic as LiveTopic);
            refresh();
          },
          leasedStream ? LIVE_TOPICS_OBJECT : undefined,
        );
        attempt = undefined;
        if (stopped) return;
        heard.clear();
        refresh();
        if (leasedStream && granted === null) continue;
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
    const keepalive = setInterval(() => {
      // the lease renewal touches the session too; this is for an f50d without `want`
      if (granted === null) {
        void f50("f50.job", "list", {}, { session: sid }).catch(() => undefined);
      }
    }, KEEPALIVE);

    return () => {
      stopped = true;
      attempt?.abort();
      wake?.();
      clearTimeout(debounce);
      clearInterval(keepalive);
      clearInterval(renew);
      clearInterval(stale);
      offWanted();
      document.removeEventListener("visibilitychange", onVisible);
      // Release this provider's lease after the last update, using the same session without changing the current login.
      if (granted !== null) {
        void Promise.resolve(pending)
          .then(() => ubusCall(
            LIVE_TOPICS_OBJECT, "want", { client, topics: "" }, sid, { probe: true },
          ))
          .catch(() => undefined);
      }
      setPushedTopics([]);
      setLiveState("off");
      resetSeries();
    };
  }, [qc, sid]);

  return children;
}

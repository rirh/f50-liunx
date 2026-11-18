/**
 * Which live topics the open pages want, which of them the stream feeds, and the polling interval a page
 * uses otherwise.
 *
 * A page that shows a live figure asks {@link usePoll} (or {@link useLiveTopic}) with its topic: while mounted that
 * registers the topic, and the provider (`components/providers/live-provider.tsx`) asks f50d for exactly the topics
 * registered (`f50.live.topics want`). The answer is per topic: `false` (no
 * polling) only while the stream is live, f50d granted that topic and it has been heard from recently; its own
 * interval otherwise, so a topic this f50d lacks, or one that went quiet, is polled while the others stay pushed.
 */
import { useEffect, useSyncExternalStore } from "react";
import type { LiveTopic } from "@/lib/live";

export type LiveState =
  /** Not signed in, or the provider is not mounted. */
  | "off"
  | "connecting"
  | "live"
  /** The login has no `:subscribe` grant (it predates it: signing in again fixes it); pages poll. */
  | "denied"
  /** The stream failed; pages poll until it is back. */
  | "retrying";

let state: LiveState = "off";
/** The topics the stream feeds now (set by the provider). */
let pushed: ReadonlySet<LiveTopic> = new Set();
const listeners = new Set<() => void>();

/** How many mounted users each topic has. */
const wanted = new Map<LiveTopic, number>();
const wantListeners = new Set<() => void>();

function emit(set: Set<() => void>) {
  for (const l of set) l();
}

export function setLiveState(next: LiveState) {
  if (next === state) return;
  state = next;
  emit(listeners);
}

/** The topics the stream feeds now; pages whose topic is not in it poll. */
export function setPushedTopics(next: Iterable<LiveTopic>) {
  const set = new Set(next);
  if (set.size === pushed.size && [...set].every((t) => pushed.has(t))) return;
  pushed = set;
  emit(listeners);
}

/** The topics some mounted page wants, sorted (a stable lease request). */
export function wantedTopics(): LiveTopic[] {
  return [...wanted.keys()].sort();
}

/** Calls `l` whenever the wanted topics change; returns the unsubscribe. */
export function onWantedChange(l: () => void): () => void {
  wantListeners.add(l);
  return () => {
    wantListeners.delete(l);
  };
}

function retain(topic: LiveTopic): () => void {
  const n = wanted.get(topic) ?? 0;
  wanted.set(topic, n + 1);
  if (n === 0) emit(wantListeners);
  return () => {
    const left = (wanted.get(topic) ?? 1) - 1;
    if (left > 0) {
      wanted.set(topic, left);
      return;
    }
    wanted.delete(topic);
    emit(wantListeners);
  };
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useLiveState(): LiveState {
  return useSyncExternalStore(
    subscribe,
    () => state,
    () => state,
  );
}

/** Wants `topic` while mounted; `true` while the stream feeds it (the page need not poll it). */
export function useLiveTopic(topic: LiveTopic): boolean {
  useEffect(() => retain(topic), [topic]);
  const fed = () => state === "live" && pushed.has(topic);
  return useSyncExternalStore(subscribe, fed, fed);
}

/** `refetchInterval` for a query that the live `topic` also fills: `false` while that topic is pushed, `ms` otherwise. */
export function usePoll(ms: number, topic: LiveTopic): number | false {
  return useLiveTopic(topic) ? false : ms;
}

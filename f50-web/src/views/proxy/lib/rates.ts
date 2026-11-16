/**
 * The proxy's speed, from the deltas of `status` (`up` / `down` are bytes since the start): the last 61 samples,
 * kept while the console is open so every proxy page starts with what was already seen. Recorded by
 * `useProxyStatus` whichever way a status arrived (pushed on the `proxy` topic or polled).
 */
import { useSyncExternalStore } from "react";
import type { ProxyStatus } from "../types";

export type Rate = { t: number; up: number; down: number };

const KEEP = 61;
let rates: Rate[] = [];
let prev: {
  up: number;
  down: number;
  at: number;
  since: number | null;
} | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

/** One status reading, taken at `at` (ms). The same reading seen twice counts once. */
export function recordStatus(s: ProxyStatus, at: number) {
  if (s.state !== "running") {
    if (prev || rates.length) {
      prev = null;
      rates = [];
      emit();
    }
    return;
  }
  const last = prev;
  if (last && last.at === at) return;
  prev = { up: s.up, down: s.down, at, since: s.since };
  // another run of the engine, or counters that went back: start again from here
  if (!last || last.since !== s.since || s.up < last.up || s.down < last.down)
    return;
  const dt = (at - last.at) / 1000;
  if (dt < 0.5) return;
  rates = [
    ...rates,
    { t: at, up: (s.up - last.up) / dt, down: (s.down - last.down) / dt },
  ].slice(-KEEP);
  emit();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useRates(): Rate[] {
  return useSyncExternalStore(
    subscribe,
    () => rates,
    () => rates,
  );
}

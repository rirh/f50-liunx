/**
 * Short histories of live figures, for the charts: CPU and memory share and temperature, the signal, the data rate.
 *
 * They are recorded from the query cache (see `components/providers/live-provider.tsx`), so a reading counts the same
 * whether it was pushed over `f50.live` or polled, and every page that shows a chart starts with what was already
 * seen. Each series keeps its last {@link KEEP} points; components read them with {@link useSeries}.
 */
import { useSyncExternalStore } from "react";

/** About four minutes of 2-3 s readings. */
export const KEEP = 120;

export interface SystemPoint {
  t: number;
  /** Busy share since the previous reading, %. */
  cpu: number | null;
  mem: number | null;
  temp: number | null;
}

export interface SignalPoint {
  t: number;
  rsrp: number | null;
  rsrq: number | null;
  sinr: number | null;
}

export interface RatePoint {
  t: number;
  /** Bytes per second since the previous reading. */
  rx: number;
  tx: number;
}

interface Series {
  system: SystemPoint[];
  signal: SignalPoint[];
  rate: RatePoint[];
}

type Name = keyof Series;

let state: Series = { system: [], signal: [], rate: [] };
const listeners = new Set<() => void>();

export function append<N extends Name>(name: N, point: Series[N][number]) {
  const list = state[name] as Series[N][number][];
  const last = list.at(-1);
  // the same reading seen twice (a push and a poll that crossed) is kept once
  if (last && last.t === point.t) return;
  state = { ...state, [name]: [...list, point].slice(-KEEP) };
  for (const l of listeners) l();
}

/** Forgets everything (a logout, another device). */
export function resetSeries() {
  state = { system: [], signal: [], rate: [] };
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useSeries<N extends Name>(name: N): Series[N] {
  return useSyncExternalStore(
    subscribe,
    () => state[name],
    () => state[name],
  );
}

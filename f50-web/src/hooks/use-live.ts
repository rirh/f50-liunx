/**
 * Whether the pages are fed by the `f50.live` stream, and the polling interval a page uses otherwise.
 *
 * The provider (`components/providers/live-provider.tsx`) owns the one stream and sets the state here; a page that
 * shows a live figure asks {@link usePoll} for its `refetchInterval`: `false` while the topic is pushed, its own
 * interval when the stream is down, refused or not yet open, so nothing stops updating when push is unavailable.
 */
import { useSyncExternalStore } from "react";
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
const listeners = new Set<() => void>();

export function setLiveState(next: LiveState) {
  if (next === state) return;
  state = next;
  for (const l of listeners) l();
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

/**
 * `refetchInterval` for a query that a live topic also fills: `false` while the stream is live, `ms` otherwise.
 * `topic` documents which event replaces the poll.
 */
export function usePoll(ms: number, _topic?: LiveTopic): number | false {
  return useLiveState() === "live" ? false : ms;
}

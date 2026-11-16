/** The proxy pages' own UI state: delay results and which node groups are folded. */
import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";

export type Latency = number | "pending" | "fail";

/** Delay test results by node id; cleared when a subscription is refreshed. */
export const latencyAtom = atom<Record<string, Latency>>({});

/** Folded node groups by key (`sub:<id>`, `local`, `group:<name>`); remembered in this browser. */
export const foldedAtom = atomWithStorage<Record<string, boolean>>(
  "f50.proxy.folded",
  {},
  undefined,
  { getOnInit: true },
);

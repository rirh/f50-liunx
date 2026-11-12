import type { Key } from "@/i18n";
import type { Rat, SimState } from "./types";

export const RAT_LABEL: Record<Rat, string> = {
  gsm: "2G",
  umts: "3G",
  lte: "4G LTE",
  nsa: "5G NSA",
  sa: "5G SA",
};

export const SIM_STATE_LABEL: Record<SimState, Key> = {
  ready: "cellular.signal.simState.ready",
  absent: "cellular.signal.simState.absent",
  locked: "cellular.signal.simState.locked",
};

/** 27.007 registration `<stat>`. */
export const REG_STAT_LABEL: Partial<Record<number, Key>> = {
  0: "cellular.signal.regStat.unregistered",
  1: "cellular.signal.regStat.registered",
  2: "cellular.signal.regStat.searching",
  3: "cellular.signal.regStat.denied",
  4: "cellular.signal.regStat.unknown",
  5: "cellular.signal.regStat.roaming",
  10: "cellular.signal.regStat.emergency",
};

export const CFUN_LABEL: Partial<Record<number, Key>> = {
  0: "cellular.signal.cfun.off",
  1: "cellular.signal.cfun.on",
  2: "cellular.signal.cfun.off",
  4: "cellular.shared.airplane",
};

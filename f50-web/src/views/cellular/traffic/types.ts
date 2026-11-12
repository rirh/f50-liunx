/** `f50.traffic` replies (f50-data/src/traffic/mod.rs `view`); byte counts, dates as local `YYYY-MM-DD`. */

export interface RxTx {
  rx: number;
  tx: number;
}

export interface DayUsage extends RxTx {
  date: string;
}

export interface CycleUsage extends RxTx {
  start: string;
  /** The cycle's last day (inclusive). */
  last: string;
  used: number;
}

export type CapLevel = "ok" | "warn" | "over" | "none";

/** `f50.traffic get`. */
export interface TrafficStatus {
  /** The counted cellular device and whether it was there at the last round. */
  device: string;
  available: boolean;
  /** The clock is trusted: bytes are put on days. */
  clock: boolean;
  /** Unix seconds when counting started. */
  since: number | null;
  now: number;
  reset_day: number;
  plan: string;
  today: DayUsage;
  /** Counted while the clock was not trusted, added to the current cycle. */
  pending: RxTx;
  cycle: CycleUsage & { days_left: number };
  cap: {
    /** 0: no cap. */
    bytes: number;
    /** Percent of the cap that warns. */
    warn: number;
    /** Data is cut at the cap. */
    cut: boolean;
    level: CapLevel;
    cut_active: boolean;
  };
  /** Up to 62 days, newest first. */
  days: DayUsage[];
  /** Up to 12 cycles, current first. */
  cycles: CycleUsage[];
}

export type TrafficKey =
  "reset_day" | "cap" | "warn" | "cut" | "plan" | "clear";

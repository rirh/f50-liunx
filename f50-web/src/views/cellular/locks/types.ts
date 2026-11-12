/** `f50.lock` replies (f50-modem/src/objects/lock.rs, lock/{mod,read,apply,reset}.rs). */

/** Network modes (`codec::mode_label`): auto152 is automatic without 2G. */
export type ModeLabel = "auto" | "auto152" | "4g" | "sa" | "nsa";

export interface ModeReading {
  label: ModeLabel;
  /** `+SPTESTMODE` work mode of slot 0, of slot 1, and the primary field. */
  work: number | null;
  gran: number | null;
  m1: number | null;
  prim: number | null;
}

export interface BandsReading {
  /** Locked bands, sorted; empty is automatic. */
  locked: number[];
  raw: string | null;
}

/** What the modem holds now, as last read. */
export interface LockReading {
  ts: number;
  mode: ModeReading;
  /** EN-DC allowed; null when the read failed. */
  endc: boolean | null;
  lte: BandsReading;
  nr: BandsReading;
  /** Every locked cell, NR first: `nr:627264,393`, `lte:1650,211`. */
  cells: string[];
  /** The supported bands: NR from the module, LTE the board's table. */
  caps: { nr: number[]; lte: number[] };
}

/** The last reset's account. */
export interface ResetResult {
  state: "running" | "ok" | "failed";
  ts: number;
  end: number;
  error?: string;
  step?: string | null;
}

/** `f50.lock get`. */
export interface LockStatus {
  /** The boot replay of saved locks. */
  auto_apply: boolean;
  reset: ResetResult | null;
  /** Null until the first background read finishes. */
  lock: LockReading | null;
  refreshing: boolean;
  /** The id of the running radio job, if any. */
  applying: string | null;
}

export type LockKind = "mode" | "endc" | "lte" | "nr" | "cell" | "auto_apply";

/** `f50.lock set`: `auto_apply` answers at once, every other kind starts a job. */
export type LockSetReply =
  { auto_apply: boolean } | { job: string; queued: boolean };

/** The result of a `f50.lock set` job. */
export interface LockApplyResult {
  kind: Exclude<LockKind, "auto_apply">;
  value: string;
  restarted: boolean;
  lock: LockReading | null;
}

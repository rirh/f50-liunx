/** `f50.boot get` (f50-hw/src/bootctl/state.rs `BootInfo`, report.rs `StageReport`, misc.rs `SlotInfo`). */

export type Slot = "a" | "b";

export type Stage = "early" | "vendor" | "hardware" | "post";

export type Outcome = "done" | "skipped" | "failed";

export interface Step {
  name: string;
  outcome: Outcome;
  detail: string;
}

export interface StageReport {
  stage: Stage;
  /** Unix seconds the stage started (0 before the clock is set). */
  started: number;
  /** Kernel uptime at the start, seconds. */
  uptime: number | null;
  duration_ms: number;
  /** False when a step the stage exists for failed. */
  ok: boolean;
  steps: Step[];
}

/** One slot as the boot control block describes it. */
export interface SlotInfo {
  priority: number;
  tries: number;
  successful: boolean;
}

export interface MiscState {
  hex: string;
  magic: boolean;
  crc_ok: boolean;
  slot_a: SlotInfo;
  slot_b: SlotInfo;
}

export interface BootInfo {
  /** The slot Linux runs from. */
  slot: Slot;
  /** "linux" keeps Linux armed; "android" (also when unset) leaves every Linux boot a one-shot trial. */
  default_boot: string;
  /** Failed boots in a row before Android. */
  attempts: number;
  /** Linux stays whatever happens. */
  locked: boolean;
  /** Boots since the last good one, as the initramfs counted them. */
  boots_since_good: number | null;
  /** This slot's pair of blocks is there, matches it, and misc can be written. */
  writable: boolean;
  /** null when misc cannot be read. */
  misc: MiscState | null;
  /** The last report of every stage that left one, in boot order. */
  stages: StageReport[];
}

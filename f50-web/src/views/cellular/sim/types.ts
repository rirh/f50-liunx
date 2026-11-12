/** `f50.sim` replies (f50-modem/src/objects/sim.rs, sim/mod.rs, sim/state.rs). */

export type SimState = "absent" | "ready" | "locked";

/** `f50.sim get`. */
export interface SimStatus {
  /** The slot this boot uses. */
  active: 1 | 2;
  /** The slot the next boot will use. */
  default: 1 | 2;
  slot2_supported: boolean;
  /** False on the F50: a selection applies at the next boot. */
  live_switch: boolean;
  reboot_required: boolean;
  /** The card in use's own account (`AT+CPIN?`), null before the first check. */
  state: SimState | null;
  /** Each card's radio setting, SIM1 then SIM2; both off is the persistent airplane mode. */
  radio: { enabled: [boolean, boolean]; all_off: boolean };
  /** A radio sequence is running. */
  busy: boolean;
}

/** The result of the `f50.sim select` job. */
export interface SelectResult {
  active: 1 | 2;
  default: 1 | 2;
  reboot_required: boolean;
}

export type CardSlot = "1" | "2" | "all";

/** `f50.sim card`: a job when the card in use has to follow now. */
export interface CardReply {
  saved: true;
  job?: string;
  applies?: string;
}

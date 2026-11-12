/** `f50.data` replies (f50-data/src/objects.rs, state.rs, settings.rs). */

/** Where the connection is (`Phase`). */
export type DataPhase = "down" | "connecting" | "connected" | "held" | "failed";
/** Why data is held back (`Hold`). */
export type DataHold = "roaming" | "sim-missing" | "radio-off";
export type Pdp = "ipv4" | "ipv6" | "ipv4v6";
/** As stored; `both` (PAP or CHAP, the network picks) and `invalid` can be read but not written. */
export type AuthRead = "none" | "pap" | "chap" | "both" | "invalid";
export type AuthWrite = "none" | "pap" | "chap";

/** `f50.data get`. Never holds the password. */
export interface DataStatus {
  state: DataPhase;
  hold: DataHold | null;
  /** The last failed dial's reason. */
  error: string | null;
  /** Seconds in the current phase. */
  since_s: number | null;
  interface: string;
  device: string;
  slot: 1 | 2;
  /** `address/prefix` of the IPv4 and IPv6 interfaces. */
  addresses: string[];
  dns: string[] | null;
  relay: boolean;
  apn: string;
  pdp: Pdp;
  auth: AuthRead;
  username: string;
  password_set: boolean;
  iccid_bound: boolean;
  roaming: boolean;
  roaming_held: boolean;
}

/** `f50.data set` arguments: only what changes is sent; an absent password keeps the saved one. */
export interface DataSettings {
  apn?: string;
  auth?: AuthWrite;
  username?: string;
  password?: string;
  pdp?: Pdp;
  roaming?: boolean;
}

export interface DataSetReply {
  slot: 1 | 2;
  /** The context changed: netifd dials again. */
  redial: boolean;
}

export interface DataPowerReply {
  state: "on" | "off";
}

/** `f50.modem` replies (f50-modem/src/objects/modem.rs, collector/{round,parse,engmd,operator}.rs). */

export type Rat = "gsm" | "umts" | "lte" | "nsa" | "sa";
export type SimState = "absent" | "ready" | "locked";

/** The serving cell's signal from the preferred source (`Signal`); `rssi` is never filled today. */
export interface Signal {
  rsrp: number | null;
  rsrq: number | null;
  sinr: number | null;
  rssi: number | null;
  bars: number;
}

/** `f50.modem status`. */
export interface ModemStatus {
  registered: boolean;
  roaming: boolean;
  rat: Rat | null;
  operator: string | null;
  signal: Signal | null;
  bars: number;
  cfun: number | null;
  /** Set when the modem did not answer the last round. */
  error: string | null;
  age_ms: number | null;
  slot: 1 | 2;
  sim_state: SimState | null;
  radio_allowed: boolean;
  restarting: boolean;
}

/** `+CEREG` / `+C5GREG` decoded. */
export interface Registration {
  stat: number | null;
  tac: string | null;
  cid: string | null;
  act: number | null;
}

export interface OperatorInfo {
  name: string | null;
  plmn: string | null;
  act: number | null;
}

export interface LteServing {
  band: number;
  earfcn: number;
  pci: number;
  rsrp: number | null;
  rsrq: number | null;
  /** The carriers' bandwidths joined with "+" ("20M+10M"); empty when unknown. */
  bw: string;
  enb: number | null;
  cid: number | null;
  /** Hex, empty when unknown. */
  tac: string;
  /** "DL_CA" while downlink carrier aggregation is active. */
  ca: "DL_CA" | "";
  sinr: number | null;
  dl_mcs: number | null;
  ul_mcs: number | null;
  dl_bler: number | null;
  ul_bler: number | null;
}

export interface NrServing {
  band: number;
  arfcn: number;
  pci: number;
  rsrp: number | null;
  rsrq: number | null;
  sinr: number | null;
  bw_mhz: number | null;
  gnb: string | null;
  cid: string | null;
  ul_mcs: number | null;
  dl_mcs: number | null;
  ul_bler: number | null;
  dl_bler: number | null;
}

/** The bearer's QoS (`+CGEQOSRDP`); rates in Mbit/s. */
export interface Qos {
  qci: number;
  dl: number | null;
  ul: number | null;
}

export interface Neighbour {
  rat: "lte" | "nr";
  band: number;
  arfcn: number;
  pci: number;
  rsrp: number | null;
  rsrq: number | null;
  /** NR neighbours only. */
  sinr?: number | null;
}

/** The collector's detail: a full reading, or `{ts, error}` when the modem did not answer. */
export interface CellDetail {
  ts?: number;
  error?: string;
  cfun?: number | null;
  reg?: Registration | null;
  reg5g?: Registration | null;
  operator?: OperatorInfo | null;
  sig_src?: "nr" | "lte" | "cesq";
  sig?: {
    rsrp?: number | null;
    rsrq?: number | null;
    sinr?: number | null;
  } | null;
  lte?: LteServing | null;
  nr?: NrServing | null;
  qos?: Qos | null;
  neigh?: Neighbour[] | null;
  full_ts?: number;
}

/** `f50.modem cells`; `detail` is null before the first reading. */
export interface ModemCells {
  age_ms: number | null;
  detail: CellDetail | null;
}

export interface Identity {
  ts: number;
  slot: number;
  mfg: string | null;
  model: string | null;
  fw: string | null;
  imei: string | null;
  imsi: string | null;
  iccid: string | null;
}

/** `f50.modem identity` (write access: private). */
export interface ModemIdentity {
  slot: 1 | 2;
  ident: Identity | null;
  msisdn: string | null;
  msisdn_src: "sim" | "ims" | null;
}

/** `f50.modem radio` / `f50.sim card`: a job when the card in use must follow now. */
export interface RadioReply {
  saved: true;
  job?: string;
  applies?: string;
}

/** `f50.status get`: each part is the envelope of the read method it comes from (see f50-api/src/status.rs). */
import type { ApiErrorCode } from "@/lib/ubus";

export type Part<T> =
  | { ok: true; data: T }
  | { ok: false; code: ApiErrorCode; error: string; field?: string };

/** `f50.modem status` (f50-modem/src/objects/modem.rs). */
export type Rat = "gsm" | "umts" | "lte" | "nsa" | "sa";
export type SimState = "absent" | "ready" | "locked";

export interface ModemStatus {
  registered: boolean;
  roaming: boolean;
  rat: Rat | null;
  operator: string | null;
  signal: {
    rsrp: number | null;
    rsrq: number | null;
    sinr: number | null;
    rssi: number | null;
    bars: number;
  } | null;
  bars: number;
  cfun?: unknown;
  error?: unknown;
  age_ms: number | null;
  slot: number;
  sim_state: SimState | null;
  radio_allowed: boolean;
  restarting: boolean;
}

/** `f50.sim get` (f50-modem/src/sim/mod.rs). */
export interface SimInfo {
  active: number;
  default: number;
  reboot_required: boolean;
  state: SimState | null;
  radio: { enabled: [boolean, boolean]; all_off: boolean };
  busy: boolean;
}

/** `f50.data get` (f50-data/src/objects.rs). */
export type DataPhase = "down" | "connecting" | "connected" | "held" | "failed";
export type DataHold = "roaming" | "sim-missing" | "radio-off";

export interface DataInfo {
  state: DataPhase;
  hold: DataHold | null;
  error: string | null;
  since_s: number | null;
  interface: string;
  device: string;
  slot: number;
  addresses: string[];
  dns: string[] | null;
  /** The device's byte counters since it came up (diffed for the rate). */
  bytes?: { rx: number; tx: number } | null;
  apn: string;
  roaming: boolean;
  roaming_held: boolean;
}

/** `f50.traffic get` (f50-data/src/traffic/mod.rs `view`). */
export interface TrafficView {
  available: boolean;
  clock: boolean;
  reset_day: number;
  plan: string;
  today: { date: string; rx: number; tx: number };
  cycle: {
    start: string;
    last: string;
    days_left: number;
    rx: number;
    tx: number;
    used: number;
  };
  cap: {
    bytes: number;
    warn: number;
    cut: boolean;
    level: "ok" | "warn" | "over" | "none";
    cut_active: boolean;
  };
}

/** `f50.wifi get` (f50-net/src/wifi/ap.rs). */
export interface WifiInfo {
  enabled: boolean;
  client_mode: boolean;
  ssid: string | null;
  band: "2.4" | "5";
  channel: string;
  channel_live: number | null;
  security: "wpa2" | "wpa2-wpa3" | "wpa3" | "open" | "other";
  hidden: boolean;
  status: string | null;
  clients: { mac: string; ip: string | null; name: string | null }[];
}

/** `f50.wificlient status` (f50-net/src/client/join.rs). */
export interface WifiClientInfo {
  mode: "off" | "client" | "hotspot";
  connected: boolean;
  ssid: string | null;
  signal: number | null;
  address: string | null;
  active: boolean;
  firewall: "none" | "closed" | "shared" | "unknown";
  firewall_ok: boolean;
  saved: { ssid: string; at_boot: boolean } | null;
}

/** `f50.usb get` (f50-net/src/usb.rs). */
export interface UsbInfo {
  role: "host" | "device" | "unavailable";
  net_mode: string;
  net_current: string;
  link: {
    udc: string;
    bound: boolean;
    state: string | null;
    speed: string | null;
  } | null;
  devices: {
    port: string;
    manufacturer: string | null;
    product: string | null;
    id: string;
    speed: string | null;
  }[];
  adb: { on: boolean; state: unknown; running: boolean };
}

/** `f50.system status` (f50-system/src/info.rs, dash.rs, facts.rs, thermal.rs). */
export interface SystemStatusInfo {
  ts: number;
  uptime: number | null;
  load: {
    n: number;
    total: number;
    idle: number;
    freqs: { cur: number | null; max: number | null }[];
  } | null;
  memory: {
    total_kb: number | null;
    avail_kb: number | null;
    swap_total_kb: number | null;
    swap_free_kb: number | null;
  };
  temperatures: {
    soc: number | null;
    cpu: number | null;
    modem: number | null;
    board: number | null;
    hottest: number | null;
  };
  heat_alarm: boolean;
  wifi_clients: number;
  lan_leases: number;
  conns: number | null;
  usb_link: {
    role: "device" | "host";
    speed: string | null;
    mbps: number | null;
  } | null;
  update: string | null;
}

/** `f50.sms unread` (f50-sms/src/api.rs). */
export interface SmsUnread {
  total: number;
  unread: number;
}

/** The part of `f50.update status` the dashboard shows (f50-update/src/status.rs). */
export interface UpdateBrief {
  installed: string | null;
  restart_pending: boolean;
  job: string | null;
  notice: { tag: string; installed: string | null; at: number } | null;
}

export interface DashboardStatus {
  modem?: Part<ModemStatus>;
  sim?: Part<SimInfo>;
  data?: Part<DataInfo>;
  traffic?: Part<TrafficView>;
  wifi?: Part<WifiInfo>;
  wificlient?: Part<WifiClientInfo>;
  usb?: Part<UsbInfo>;
  system?: Part<SystemStatusInfo>;
  sms?: Part<SmsUnread>;
  update?: Part<UpdateBrief>;
}

export type OnOff = "on" | "off";

/** `f50.system` replies (f50-system/src/{info,facts,thermal,dash,zram}.rs). */

/** facts::CpuTimes: cumulative clock ticks since boot; diff two readings for the busy share. */
export interface CpuTimes {
  /** Number of CPUs. */
  n: number;
  total: number;
  /** idle + iowait. */
  idle: number;
  /** One per cpufreq policy (cluster), kHz. */
  freqs: { cur: number | null; max: number | null }[];
}

/** facts::Memory, kB. */
export interface Memory {
  total_kb: number | null;
  avail_kb: number | null;
  swap_total_kb: number | null;
  swap_free_kb: number | null;
}

/** facts::Disk, kB. */
export interface Disk {
  size_kb: number;
  used_kb: number;
  avail_kb: number;
}

/** thermal::Temps, °C (zones in m°C). */
export interface Temps {
  soc: number | null;
  cpu: number | null;
  modem: number | null;
  board: number | null;
  hottest: number | null;
  zones: { name: string; millicelsius: number }[];
}

/** zram::status. */
export interface Zram {
  on: boolean;
  enabled: boolean;
  size_bytes: number | null;
  algorithm: string | null;
  stored_bytes: number | null;
  ram_used_bytes: number | null;
}

/** `f50.system info`. */
export interface SystemInfo {
  device: string;
  system: string;
  model: string | null;
  release: string | null;
  kernel: string | null;
  hostname: string;
  timezone: string | null;
  user: string;
  /** Unix seconds. */
  time: number;
  uptime: number | null;
  load: CpuTimes | null;
  memory: Memory;
  storage: { root: Disk | null; disk: Disk | null };
  temperatures: Temps;
  zram: Zram;
}

/** dash::UsbLink. */
export interface UsbLink {
  role: "device" | "host";
  speed: string | null;
  mbps: number | null;
  max: string | null;
  max_mbps: number | null;
}

/** `f50.system status`. */
export interface SystemStatus {
  ts: number;
  uptime: number | null;
  load: CpuTimes | null;
  memory: Memory;
  temperatures: Temps;
  heat_alarm: boolean;
  wifi_clients: number;
  lan_leases: number;
  conns: number | null;
  usb_link: UsbLink | null;
  /** The tag of a newer release, when the update service saw one. */
  update: string | null;
}

/** dash::Station. */
export interface Station {
  mac: string;
  host: string | null;
  ip: string | null;
  /** dBm. */
  signal: number | null;
  tx: string | null;
  rx: string | null;
  conn_min: number | null;
}

/** dash::Lease. */
export interface Lease {
  host: string | null;
  ip: string;
  mac: string;
  /** Seconds left. */
  left: number;
}

/** `f50.system clients`. */
export interface Clients {
  wifi: Station[];
  leases: Lease[];
}

/** One poll of the live figures, derived for the charts. */
export interface Sample {
  /** ms, for the x axis. */
  t: number;
  cpu: number | null;
  mem: number | null;
  temp: number | null;
}

/** The part of `f50.modem identity` (write access: private) the overview shows. */
export interface OwnNumber {
  slot: 1 | 2;
  msisdn: string | null;
  msisdn_src: "sim" | "ims" | null;
}

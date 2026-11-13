/** `f50.wifi` replies (`f50d/crates/f50-net/src/wifi/{ap,settings}.rs`). */

export type WifiBand = "2.4" | "5";

/** The access point's `encryption` in one word (`ap.rs security()`). */
export type WifiSecurity = "wpa2" | "wpa2-wpa3" | "wpa3" | "open" | "other";

/** `f50.wifi credentials`: administrator-only, read on demand for sharing. */
export interface WifiCredentials {
  ssid: string;
  password: string | null;
  security: WifiSecurity;
  hidden: boolean;
}

/** A station of the access point, with its DHCP lease's address and name where there is one. */
export interface WifiStation {
  mac: string;
  ip: string | null;
  name: string | null;
}

/** `f50.wifi get`: never the password, only whether one is set. */
export interface WifiInfo {
  /** Neither the radio nor the access point is disabled in UCI. */
  enabled: boolean;
  /** The radio is the Wi-Fi client's: settings wait until the hotspot comes back. */
  client_mode: boolean;
  ssid: string | null;
  band: WifiBand;
  /** The configured channel: "auto" or a number as text. */
  channel: string;
  /** The channel hostapd (or `iw`) reports now. */
  channel_live: number | null;
  /** UCI `htmode` (`HT20`, `VHT80`, `HE40`, ...). */
  htmode: string | null;
  country: string;
  security: WifiSecurity;
  hidden: boolean;
  password_set: boolean;
  /** The channels the hotspot may use in this country (5 GHz: without DFS). */
  channels: { "2.4": number[]; "5": number[] };
  /** hostapd's `get_status` status ("ENABLED", ...), absent when hostapd is not running. */
  status: string | null;
  clients: WifiStation[];
}

/** `f50.wifi set`: only the fields to change; channel and width as text ("auto" or a number). */
export interface WifiSetArgs {
  ssid?: string;
  password?: string;
  band?: WifiBand;
  channel?: string;
  width?: string;
  hidden?: boolean;
  country?: string;
}

/** `f50.wifi set`: written; the Wi-Fi reloads two seconds after the reply. */
export interface WifiSetResult {
  restart: true;
}

/** `f50.wifi power`: `pending` (netifd reloads), `unchanged`, `saved` (the reload failed: a reboot applies it). */
export interface WifiPowerResult {
  result: "pending" | "unchanged" | "saved";
}

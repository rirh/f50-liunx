/** `f50.wificlient` replies (`f50d/crates/f50-net/src/client.rs`, `client/{join,scan}.rs`). */
import type { JobStarted } from "@/lib/jobs";

/** Who has the radio: nobody, the client (`wpa_supplicant` holds wlan0) or the hotspot. */
export type ClientMode = "off" | "client" | "hotspot";

/** The nft tables for wlan0: none, closed (nothing forwarded), shared with the LAN, or not readable. */
export type FirewallState = "none" | "closed" | "shared" | "unknown";

/** `f50.wificlient status`: read live from the supplicant, the lease, nft and the saved file. */
export interface ClientStatus {
  mode: ClientMode;
  /** The supplicant reached COMPLETED. */
  connected: boolean;
  ssid: string | null;
  /** RSSI in dBm. */
  signal: number | null;
  /** wlan0's IPv4 as `A.B.C.D/N`. */
  address: string | null;
  firewall: FirewallState;
  /** The client holds the radio (this daemon's join or a previous one's marker). */
  active: boolean;
  /** The tables match what the client state needs. */
  firewall_ok: boolean;
  /** The saved network; `at_boot`: it is joined at boot. */
  saved: { ssid: string; at_boot: boolean } | null;
}

/** As `scan.rs` labels a network's security. */
export type NetworkSecurity =
  "WPA2/3" | "WPA3" | "WPA2" | "WPA" | "OWE" | "EAP" | "WEP" | "open";

/** One network in range: one entry per name, strongest first, no hidden networks. No channel is reported. */
export interface ScanNetwork {
  ssid: string;
  /** dBm. */
  signal: number;
  security: NetworkSecurity;
}

export interface ScanResult {
  networks: ScanNetwork[];
}

/** `f50.wificlient scan`: at once when the supplicant holds wlan0, else a job whose result is a `ScanResult`. */
export type ScanReply = ScanResult | JobStarted;

export interface JoinArgs {
  ssid: string;
  password?: string;
  open?: boolean;
}

/** The result of a join job. */
export interface JoinResult {
  joined: true;
  ssid: string;
  address: string | null;
}

/** `forget`: delete the saved network; `keep`: still join it at boot; neither: saved but not joined at boot. */
export interface LeaveArgs {
  forget?: boolean;
  keep?: boolean;
}

/** The result of a leave job. */
export interface LeaveResult {
  left: true;
  forgotten: boolean;
}

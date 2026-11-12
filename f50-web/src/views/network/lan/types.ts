/** `f50.lan` replies (`f50d/crates/f50-net/src/lan.rs`). */

/** `f50.lan get`. */
export interface LanInfo {
  /** The configured router address. */
  address: string;
  netmask: string;
  prefix: number;
  /** br-lan's address now as `A.B.C.D/N` (differs from the configuration until a change is applied). */
  live: string | null;
  default_address: string;
  /** The address a computer on the USB network gets. */
  usb_host_address: string;
  dhcp: {
    enabled: boolean;
    /** Offset from the network address. */
    start: number | null;
    limit: number | null;
    /** `12h`, `30m`, `infinite`, ... */
    leasetime: string | null;
  };
}

/** `f50.lan set`: only the fields to change. */
export interface LanSetArgs {
  address?: string;
  netmask?: string;
  dhcp?: boolean;
  dhcp_start?: number;
  dhcp_limit?: number;
  leasetime?: string;
}

/** `f50.lan set`: saved; netifd and dnsmasq reload two seconds after the reply. */
export interface LanSetResult {
  restart: true;
}

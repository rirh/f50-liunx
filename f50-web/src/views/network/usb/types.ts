/** `f50.usb` replies (`f50d/crates/f50-net/src/{usb.rs,usb/**,adbd.rs}`). */

/** The role switch now; `unavailable`: no role switch on this kernel. */
export type UsbRole = "host" | "device" | "none" | "unavailable";
/** The gadget's network mode from the next boot (`default`: NCM). */
export type UsbNetMode = "ncm" | "ecm" | "rndis" | "multi" | "default";
/** The functions the gadget has bound now (`mixed`: an unexpected mix; `none` also in the host role). */
export type UsbNetBound = "ncm" | "ecm" | "rndis" | "multi" | "mixed" | "none";
export type UsbScope = "once" | "permanent";
/** What a Linux computer names the link: `usbN` or `ethN`. */
export type UsbNetName = "usb" | "eth";

/** The device controller (UDC) from sysfs; strings as the kernel writes them. */
export interface UsbLink {
  udc: string;
  /** The gadget is bound to the controller. */
  bound: boolean;
  /** `configured`, `not attached`, `suspended`, ... */
  state: string | null;
  /** `high-speed`, `super-speed`, `UNKNOWN`, ... */
  speed: string | null;
  max_speed: string | null;
}

/** A device on the bus (`/sys/bus/usb/devices`). */
export interface UsbDevice {
  port: string | null;
  manufacturer: string | null;
  product: string | null;
  /** `vendor:product` in hex. */
  id: string;
  /** Mbit/s as text (`480`, `5000`). */
  speed: string | null;
}

/** adb in one word: off, on but unusable here (no adbd or FunctionFS), on and open to all, on for keys only. */
export type AdbState = "off" | "unavailable" | "open" | "keys";

export interface AdbSummary {
  state: AdbState;
  on: boolean;
  /** `/etc/f50/adb` says neither on nor off (adb stays off). */
  damaged: boolean;
  auth: "open" | "keys";
  /** Why the auth mode is `keys` despite the setting (an untrusted directory, ...), in English. */
  auth_reason: string | null;
  keys: number;
  /** adb's function is in the gadget. */
  in_gadget: boolean;
  /** The adbd supervisor is running. */
  running: boolean;
}

/** `f50.usb get`. */
export interface UsbInfo {
  role: UsbRole;
  role_boot: "host" | "device";
  host_supported: boolean;
  net_mode: UsbNetMode;
  net_scope: UsbScope;
  /** What boot/init made this boot (`null`: nothing recorded). */
  net_now: Exclude<UsbNetMode, "default"> | null;
  net_current: UsbNetBound;
  net_name: UsbNetName;
  link: UsbLink | null;
  devices: UsbDevice[];
  adb: AdbSummary;
}

/** `f50.usb set`: at least one of role, net, name; `scope` applies to net (once/permanent) and to the host role. */
export interface UsbSetArgs {
  role?: "host" | "device";
  net?: UsbNetMode;
  scope?: UsbScope;
  name?: UsbNetName;
}

/** `f50.usb set`: `pending`: a role switch comes a moment after the reply. */
export interface UsbSetResult {
  pending: boolean;
  net_mode: UsbNetMode;
  once: boolean;
  net_name: UsbNetName;
}

/** A USB network adapter in the host role. */
export interface UsbAdapter {
  name: string;
  carrier: boolean;
  in_lan: boolean;
  /** Not owned by another network (may join the LAN). */
  eligible: boolean;
  /** The network that owns it, `unknown` when it could not be determined. */
  owner: string | null;
}

/** `f50.usb adapters`: empty with `host_only` outside the host role. */
export interface UsbAdapters {
  devices: UsbAdapter[];
  host_only: boolean;
}

export interface AdapterAddResult {
  added: string;
}

export interface AdbArgs {
  state?: "on" | "off";
  auth?: "open" | "keys";
}

/** `f50.usb adb`: `restart`: adbd restarts (the USB link drops for a moment); `note` is an English exposure line. */
export interface AdbResult {
  state: AdbState;
  auth: "open" | "keys";
  restart: boolean;
  note: string;
}

/** `f50.usb adb_key_add`: `added` false when the key was there already. */
export interface AdbKeyResult {
  added: boolean;
  keys: number;
  auth: "open" | "keys";
}

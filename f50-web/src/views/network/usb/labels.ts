import { t, type Key } from "@/i18n";
import type { AdbState, UsbNetBound, UsbNetMode, UsbRole } from "./types";

const ROLE_LABEL = {
  host: "network.usb.role.host",
  device: "network.usb.role.device",
  none: "network.usb.role.none",
  unavailable: "network.usb.role.unavailable",
} satisfies Record<UsbRole, Key>;

const NET_MODE_LABEL = {
  default: "network.usb.netMode.default",
  ncm: "network.usb.netMode.ncm",
  ecm: "network.usb.netMode.ecm",
  rndis: "network.usb.netMode.rndis",
  multi: "network.usb.netMode.multi",
} satisfies Record<UsbNetMode, Key>;

const NET_MODE_HINT = {
  default: "network.usb.netModeHint.default",
  ncm: "network.usb.netModeHint.ncm",
  ecm: "network.usb.netModeHint.ecm",
  rndis: "network.usb.netModeHint.rndis",
  multi: "network.usb.netModeHint.multi",
} satisfies Record<UsbNetMode, Key>;

const NET_BOUND_LABEL = {
  ncm: "network.usb.netBound.ncm",
  ecm: "network.usb.netBound.ecm",
  rndis: "network.usb.netBound.rndis",
  multi: "network.usb.netBound.multi",
  mixed: "network.usb.netBound.mixed",
  none: "network.usb.netBound.none",
} satisfies Record<UsbNetBound, Key>;

const ADB_STATE_LABEL = {
  off: "network.usb.adbState.off",
  unavailable: "network.usb.adbState.unavailable",
  open: "network.usb.adbState.open",
  keys: "network.usb.adbState.keys",
} satisfies Record<AdbState, Key>;

/** A label from `map`, or the value itself when the device reports one the console does not know. */
function label<T extends string>(map: Record<T, Key>, v: T): string {
  const k = map[v] as Key | undefined;
  return k ? t(k) : v;
}

export const roleLabel = (v: UsbRole) => label(ROLE_LABEL, v);
export const netModeLabel = (v: UsbNetMode) => label(NET_MODE_LABEL, v);
export const netModeHint = (v: UsbNetMode) => label(NET_MODE_HINT, v);
export const netBoundLabel = (v: UsbNetBound) => label(NET_BOUND_LABEL, v);
export const adbStateLabel = (v: AdbState) => label(ADB_STATE_LABEL, v);

/** The dashboard's value labels, as message keys; `label(map, value)` reads one in the current language. */
import { type Key, t } from "@/i18n";
import type {
  DataHold,
  DataPhase,
  Rat,
  SimState,
  WifiClientInfo,
  WifiInfo,
} from "./types";

export const RAT_LABEL: Record<Rat, Key> = {
  gsm: "dashboard.rat.gsm",
  umts: "dashboard.rat.umts",
  lte: "dashboard.rat.lte",
  nsa: "dashboard.rat.nsa",
  sa: "dashboard.rat.sa",
};

export const SIM_STATE_LABEL: Record<SimState, Key> = {
  absent: "dashboard.simState.absent",
  ready: "dashboard.simState.ready",
  locked: "dashboard.simState.locked",
};

export const DATA_PHASE_LABEL: Record<DataPhase, Key> = {
  down: "dashboard.dataPhase.down",
  connecting: "dashboard.dataPhase.connecting",
  connected: "dashboard.dataPhase.connected",
  held: "dashboard.dataPhase.held",
  failed: "dashboard.dataPhase.failed",
};

export const DATA_HOLD_LABEL: Record<DataHold, Key> = {
  roaming: "dashboard.dataHold.roaming",
  "sim-missing": "dashboard.dataHold.sim-missing",
  "radio-off": "dashboard.dataHold.radio-off",
};

export const SECURITY_LABEL: Record<WifiInfo["security"], Key> = {
  wpa2: "dashboard.security.wpa2",
  "wpa2-wpa3": "dashboard.security.wpa2-wpa3",
  wpa3: "dashboard.security.wpa3",
  open: "dashboard.security.open",
  other: "dashboard.security.other",
};

export const UPLINK_MODE_LABEL: Record<WifiClientInfo["mode"], Key> = {
  off: "dashboard.uplinkMode.off",
  client: "dashboard.uplinkMode.client",
  hotspot: "dashboard.uplinkMode.hotspot",
};

export const USB_ROLE_LABEL: Record<string, Key> = {
  host: "dashboard.usbRole.host",
  device: "dashboard.usbRole.device",
  unavailable: "dashboard.usbRole.unavailable",
};

/** The label of `value` in `map`, or the value itself when the device sent one the console does not know. */
export function label<V extends string>(
  map: Record<V, Key>,
  value: V | string | null | undefined,
): string {
  if (value == null) return "—";
  const key = (map as Record<string, Key | undefined>)[value];
  return key ? t(key) : value;
}

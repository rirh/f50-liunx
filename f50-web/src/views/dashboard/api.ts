import { f50 } from "@/lib/ubus";
import type { DashboardStatus, OnOff } from "./types";

export const STATUS_KEY = ["f50.status"] as const;

/** Every dashboard part in one call; a failed part is its own `{ ok: false }`, never the whole call. */
export const getStatus = () => f50<DashboardStatus>("f50.status", "get");

export const setDataPower = (state: OnOff) =>
  f50<{ state: OnOff }>("f50.data", "power", { state });

/** `result`: `pending` (netifd reloads), `unchanged`, or `saved` (applies after a reboot or `wifi reload`). */
export const setWifiPower = (state: OnOff) =>
  f50<{ result: "pending" | "unchanged" | "saved" }>("f50.wifi", "power", {
    state,
  });

/** Reboots in two seconds (`f50.system reboot`). */
export const reboot = () => f50<Record<string, never>>("f50.system", "reboot");

/** The next boot is Android's, then a reboot in two seconds (`f50.boot android`). */
export const switchToAndroid = () =>
  f50<{ slot: "a" | "b"; reboot_in_s: number }>("f50.boot", "android");

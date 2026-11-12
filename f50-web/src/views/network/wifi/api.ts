import { f50 } from "@/lib/ubus";
import type {
  WifiInfo,
  WifiPowerResult,
  WifiSetArgs,
  WifiSetResult,
} from "./types";

export const wifiKeys = { info: ["f50.wifi", "get"] as const };

export const getWifi = () => f50<WifiInfo>("f50.wifi", "get");
export const setWifi = (args: WifiSetArgs) =>
  f50<WifiSetResult>("f50.wifi", "set", { ...args });
export const powerWifi = (on: boolean) =>
  f50<WifiPowerResult>("f50.wifi", "power", { state: on ? "on" : "off" });

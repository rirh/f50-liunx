import { f50 } from "@/lib/ubus";
import type {
  WifiCredentials,
  WifiInfo,
  WifiPowerResult,
  WifiSetArgs,
  WifiSetResult,
} from "./types";

export const wifiKeys = {
  info: ["f50.wifi", "get"] as const,
  credentials: ["f50.wifi", "credentials"] as const,
};

export const getWifi = () => f50<WifiInfo>("f50.wifi", "get");
export const getWifiCredentials = () =>
  f50<WifiCredentials>("f50.wifi", "credentials");
export const setWifi = (args: WifiSetArgs) =>
  f50<WifiSetResult>("f50.wifi", "set", { ...args });
export const powerWifi = (on: boolean) =>
  f50<WifiPowerResult>("f50.wifi", "power", { state: on ? "on" : "off" });

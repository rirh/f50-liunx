import { f50 } from "@/lib/ubus";
import type { TrafficKey, TrafficStatus } from "./types";

export const trafficKeys = { get: ["cellular", "traffic"] as const };

export const getTraffic = () => f50<TrafficStatus>("f50.traffic", "get");
/** Every value is a string on the wire (`Rule::Text`). */
export const setTraffic = (key: TrafficKey, value: string) =>
  f50<Record<string, never>>("f50.traffic", "set", { key, value });
export const clearTraffic = () => setTraffic("clear", "yes");

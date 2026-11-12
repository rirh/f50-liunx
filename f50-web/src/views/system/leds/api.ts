import { f50 } from "@/lib/ubus";
import type { LedKey, LedSettings } from "./types";

export const ledKey = ["f50.led", "get"] as const;

export const getLeds = () => f50<LedSettings>("f50.led", "get");
/**
 * One setting; the reply is the whole new `get`. Values (settings.rs `validate`): timeout "default" or 0-86400 without
 * a leading zero; network/wifi/boot "0"|"1"; *_brightness 1-100; thermal siren|blink|off.
 */
export const setLed = (key: LedKey, value: string) =>
  f50<LedSettings>("f50.led", "set", { key, value });

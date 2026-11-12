/** `f50.led get` / `set` (f50-system/src/led/settings.rs `LedSettings::to_json`). */

export type ThermalLight = "siren" | "blink" | "off";

export type LedKey =
  | "timeout"
  | "network"
  | "wifi"
  | "network_brightness"
  | "wifi_brightness"
  | "boot"
  | "thermal";

export type Lamp = "network" | "wifi";

export interface LedSettings {
  device: string;
  lamps: Lamp[];
  dimmable: Lamp[];
  /** Seconds without a key before the lamps go dark; 0 = always on. */
  timeout: number;
  /** The timeout is the device's default (no LED_TIMEOUT in led.conf). */
  timeout_default: boolean;
  default_timeout: number;
  network: boolean;
  /** 1-100 % of the lamp's usual level. */
  network_brightness: number;
  wifi: boolean;
  wifi_brightness: number;
  boot: boolean;
  thermal: ThermalLight;
}

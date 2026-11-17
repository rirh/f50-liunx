export { cn } from "cn";

/** A temperature as a share, 30 °C empty to 90 °C full (for a filled meter). */
export const tempShare = (c: number | null | undefined) =>
  c == null ? null : ((c - 30) / 60) * 100;

/** How a figure reads: fine, normal, worth watching, act now. */
export type Tone = "success" | "info" | "warning" | "danger";

/**
 * A temperature's tone, by the SoC's operating bands: cool below 50 °C, normal load to 70 °C, warm (the dashboard's
 * "running warm") to 85 °C, and hot from 85 °C, where f50d's thermal guard caps the CPU and raises the heat alarm.
 */
export const tempTone = (c: number | null | undefined): Tone | undefined =>
  c == null
    ? undefined
    : c >= 85
      ? "danger"
      : c >= 70
        ? "warning"
        : c >= 50
          ? "info"
          : "success";

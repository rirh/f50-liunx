export { cn } from "cn";

/** A temperature as a share, 30 °C empty to 90 °C full (for a filled meter). */
export const tempShare = (c: number | null | undefined) =>
  c == null ? null : ((c - 30) / 60) * 100;

/**
 * The value syntax `f50.lock set` accepts (f50-modem/src/lock/codec.rs `validate`), checked before sending. Every
 * value is also a ubus Word: `[a-z0-9:,_-]*`, at most 127 bytes, not starting with "-".
 */
import type { Key } from "@/i18n";
import type { ModeLabel } from "./types";

export const MODES: { value: ModeLabel; label: Key; hint: Key }[] = (
  ["auto", "auto152", "4g", "nsa", "sa"] as const
).map((value) => ({
  value,
  label: `cellular.locks.modes.${value}.label` as const,
  hint: `cellular.locks.modes.${value}.hint` as const,
}));

/** Bands: empty (automatic) or up to 32 numbers of up to 3 digits. */
export function bandsValue(bands: number[]): string {
  return [...new Set(bands)].sort((a, b) => a - b).join(",");
}

export const BAND_LIST_RE = /^(?:[0-9]{1,3}(?:,[0-9]{1,3}){0,31})?$/;

/** One cell lock: `lte:<earfcn>,<pci>` or `nr:<arfcn>,<pci>`, ARFCN up to 7 digits, PCI up to 4. */
export const ARFCN_RE = /^[0-9]{1,7}$/;
export const PCI_RE = /^[0-9]{1,4}$/;

export function cellValue(
  rat: "lte" | "nr",
  arfcn: string,
  pci: string,
): string {
  return `${rat}:${arfcn},${pci}`;
}

export const CELL_VALUE_RE =
  /^(?:auto|off|off-nr|off-lte|(?:nr|lte):[0-9]{1,7},[0-9]{1,4})$/;

export function parseCell(
  c: string,
): { rat: "lte" | "nr"; arfcn: string; pci: string } | null {
  const m = /^(nr|lte):([0-9]+),([0-9]+)$/.exec(c);
  return m ? { rat: m[1] as "lte" | "nr", arfcn: m[2], pci: m[3] } : null;
}

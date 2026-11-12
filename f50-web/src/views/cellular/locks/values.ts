/**
 * The value syntax `f50.lock set` accepts (f50-modem/src/lock/codec.rs `validate`), checked before sending. Every
 * value is also a ubus Word: `[a-z0-9:,_-]*`, at most 127 bytes, not starting with "-".
 */
import type { ModeLabel } from "./types";

export const MODES: { value: ModeLabel; label: string; hint: string }[] = [
  { value: "auto", label: "自动", hint: "2G/3G/4G/5G 自动" },
  { value: "auto152", label: "自动（无 2G）", hint: "自动，但不使用 2G" },
  { value: "4g", label: "仅 4G", hint: "只使用 LTE" },
  { value: "nsa", label: "5G NSA", hint: "LTE 锚点 + NR" },
  { value: "sa", label: "5G SA", hint: "5G 独立组网" },
];

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

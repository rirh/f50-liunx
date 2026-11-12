import { t } from "@/i18n";
import type { CpuTimes, Disk, Memory } from "./types";

/** The busy share between two cumulative readings, 0-100. */
export function cpuPercent(
  prev: CpuTimes | null | undefined,
  cur: CpuTimes | null | undefined,
): number | null {
  if (!prev || !cur) return null;
  const total = cur.total - prev.total;
  const idle = cur.idle - prev.idle;
  if (total <= 0) return null;
  return Math.min(100, Math.max(0, ((total - idle) / total) * 100));
}

/** Used memory (total − available), 0-100. */
export function memPercent(m: Memory | null | undefined): number | null {
  if (!m?.total_kb || m.avail_kb == null) return null;
  return ((m.total_kb - m.avail_kb) / m.total_kb) * 100;
}

export function diskPercent(d: Disk | null | undefined): number | null {
  if (!d || d.size_kb <= 0) return null;
  return (d.used_kb / d.size_kb) * 100;
}

export const kb = (n: number | null | undefined) =>
  n == null ? null : n * 1024;

export function formatPercent(
  n: number | null | undefined,
  digits = 0,
): string {
  return n == null ? "—" : `${n.toFixed(digits)}%`;
}

export function formatCelsius(n: number | null | undefined): string {
  return n == null ? "—" : `${n.toFixed(1)} °C`;
}

export function formatMhz(khz: number | null | undefined): string {
  return khz == null ? "—" : `${Math.round(khz / 1000)} MHz`;
}

/** Thermal zone names as the kernel has them, with a Chinese hint for the known ones. */
const ZONE_LABEL: Record<string, () => string> = {
  "soc-thmzone": () => "SoC",
  "apcpu0-thmzone": () => "CPU",
  "nr0-thmzone": () => t("system.overview.zone.modem"),
  "board-thmzone": () => t("system.overview.zone.board"),
};

export const zoneLabel = (name: string) => ZONE_LABEL[name]?.() ?? name;

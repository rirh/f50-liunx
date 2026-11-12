/** Formatting the cellular pages share. */

/** A reading's age in milliseconds as "3 秒前". */
export function formatAgeMs(ms: number | null | undefined): string {
  if (ms == null) return "—";
  if (ms < 1000) return "刚刚";
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} 秒前`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分钟前`;
  return `${Math.floor(m / 60)} 小时前`;
}

/** A number with its unit, or "—". */
export function unit(v: number | string | null | undefined, u: string): string {
  if (v == null || v === "") return "—";
  return `${v} ${u}`;
}

import { format, isSameDay, isSameYear } from "date-fns";

const RE = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/;

/**
 * A message's `time` (`YYYY-MM-DD HH:MM:SS` in the zone `utc_offset` minutes east of UTC) as a Date; null when
 * the offset is unknown or the text is not a time (it is then shown as it came).
 */
export function smsDate(
  time: string,
  utcOffset: number | null | undefined,
): Date | null {
  const m = RE.exec(time);
  if (!m || utcOffset == null) return null;
  const [y, mo, d, h, mi, s] = m.slice(1).map(Number);
  return new Date(Date.UTC(y, mo - 1, d, h, mi, s) - utcOffset * 60_000);
}

/** Compact: the time today, else the date (with the year when not this year). */
export function smsShortTime(time: string, utcOffset?: number | null): string {
  const d = smsDate(time, utcOffset);
  if (!d) return time.slice(5, 16);
  const now = new Date();
  if (isSameDay(d, now)) return format(d, "HH:mm");
  if (isSameYear(d, now)) return format(d, "MM-dd HH:mm");
  return format(d, "yyyy-MM-dd");
}

/** Full local time for a title. */
export function smsFullTime(time: string, utcOffset?: number | null): string {
  const d = smsDate(time, utcOffset);
  return d ? format(d, "yyyy-MM-dd HH:mm:ss xxx") : time;
}

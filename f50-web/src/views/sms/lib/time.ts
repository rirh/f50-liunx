import { useEffect, useState } from "react";
import { dateLocale, t } from "@/i18n";
import { differenceInCalendarDays, format } from "date-fns";

const RE = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/;

/**
 * A message's `time` (`YYYY-MM-DD HH:MM:SS` in the zone `utc_offset` minutes east of UTC) as a Date. Without an
 * offset the time is taken as this browser's local time (the network's time, which is the user's zone almost
 * always); null when the text is not a time (it is then shown as it came).
 */
export function smsDate(
  time: string,
  utcOffset: number | null | undefined,
): Date | null {
  const m = RE.exec(time);
  if (!m) return null;
  const [y, mo, d, h, mi, s] = m.slice(1).map(Number);
  if (utcOffset == null) return new Date(y, mo - 1, d, h, mi, s);
  return new Date(Date.UTC(y, mo - 1, d, h, mi, s) - utcOffset * 60_000);
}

/**
 * The one form every SMS time is shown in (the list, the thread's separators): 刚刚, N 分钟前, N 小时前, N 天前
 * within a month, then 11月16日, and 2025年11月16日 beyond a year. The full time goes in a title (`smsFullTime`).
 */
export function smsShortTime(
  time: string,
  utcOffset?: number | null,
  now: Date = new Date(),
): string {
  const d = smsDate(time, utcOffset);
  if (!d) return time;
  const s = Math.max(0, Math.floor((now.getTime() - d.getTime()) / 1000));
  if (s < 60) return t("app.format.relative.now");
  if (s < 3600)
    return t("app.format.relative.minutes", { count: Math.floor(s / 60) });
  if (s < 86400)
    return t("app.format.relative.hours", { count: Math.floor(s / 3600) });
  const days = differenceInCalendarDays(now, d);
  if (days < 30)
    return t("app.format.relative.days", { count: Math.max(1, days) });
  if (days < 365)
    return format(d, t("app.format.relative.monthDay"), {
      locale: dateLocale(),
    });
  return format(d, t("app.format.relative.yearMonthDay"), {
    locale: dateLocale(),
  });
}

/** Re-renders every minute, so relative times (刚刚 → 1 分钟前) move on while the page stays open. */
export function useMinuteTick(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = window.setInterval(() => setNow(new Date()), 60_000);
    return () => window.clearInterval(id);
  }, []);
  return now;
}

/** Full local time for a title. */
export function smsFullTime(time: string, utcOffset?: number | null): string {
  const d = smsDate(time, utcOffset);
  return d ? format(d, "yyyy-MM-dd HH:mm:ss xxx") : time;
}

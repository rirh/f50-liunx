/** Display helpers shared by every page. */
import { type Key, dateLocale, t } from "@/i18n";
import { type ApiErrorCode, ApiError } from "@/lib/ubus";
import { format, formatDistanceToNow, fromUnixTime } from "date-fns";

const CODE_TEXT: Record<ApiErrorCode, Key> = {
  "bad-request": "app.error.code.bad-request",
  "unknown-method": "app.error.code.unknown-method",
  "unknown-field": "app.error.code.unknown-field",
  "bad-value": "app.error.code.bad-value",
  missing: "app.error.code.missing",
  forbidden: "app.error.code.forbidden",
  unsupported: "app.error.code.unsupported",
  busy: "app.error.code.busy",
  timeout: "app.error.code.timeout",
  failed: "app.error.code.failed",
  "not-found": "app.error.code.not-found",
  unauthorized: "app.error.code.unauthorized",
  transport: "app.error.code.transport",
};

/**
 * A failure for a toast or an alert. f50d's own messages are English sentences: they come after the name of their
 * code ("设备正忙：another radio job runs"); a message the console made itself (already translated) as it is.
 */
export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const key = CODE_TEXT[e.code];
    const label = key ? t(key) : e.code;
    if (!e.message || e.message === label) return label;
    // f50d's text says what happened in its own words, after the name of its code; the console's own message is
    // already a sentence in the page's language
    return e.fromDevice
      ? t("app.error.withDetail", { label, detail: e.message })
      : e.message;
  }
  if (e instanceof Error) return e.message;
  return t("app.error.failed");
}

export function formatBytes(n: number | null | undefined, digits = 1): string {
  if (n == null || !Number.isFinite(n)) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (Math.abs(v) >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : digits)} ${units[i]}`;
}

export function formatRate(bytesPerSecond: number | null | undefined): string {
  if (bytesPerSecond == null) return "—";
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return "—";
  const s = Math.floor(seconds);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return t("app.format.duration.dh", { d, h });
  if (h > 0) return t("app.format.duration.hm", { h, m });
  if (m > 0) return t("app.format.duration.ms", { m, s: s % 60 });
  return t("app.format.duration.s", { s });
}

/** Unix seconds as an absolute time. */
export function formatTime(unix: number | null | undefined): string {
  if (!unix) return "—";
  return format(fromUnixTime(unix), "yyyy-MM-dd HH:mm:ss");
}

/** Unix seconds as "3 分钟前" / "3 minutes ago" (use with `formatTime` in a title). */
export function formatAgo(unix: number | null | undefined): string {
  if (!unix) return "—";
  return formatDistanceToNow(fromUnixTime(unix), {
    addSuffix: true,
    locale: dateLocale(),
  });
}

export function formatValue(v: unknown, unit = ""): string {
  if (v == null || v === "") return "—";
  return `${String(v)}${unit}`;
}

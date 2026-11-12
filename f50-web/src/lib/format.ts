/** Display helpers shared by every page. */
import { ApiError } from "@/lib/ubus";
import { format, formatDistanceToNow, fromUnixTime } from "date-fns";
import { zhCN } from "date-fns/locale";

const CODE_TEXT: Record<string, string> = {
  "bad-request": "请求无效",
  "unknown-method": "设备不认识这个操作",
  "unknown-field": "请求中有多余的字段",
  "bad-value": "取值无效",
  missing: "缺少必填项",
  forbidden: "不允许",
  unsupported: "设备不支持",
  busy: "设备正忙",
  timeout: "超时",
  failed: "操作失败",
  "not-found": "不存在",
  unauthorized: "登录已过期",
  transport: "无法连接设备",
};

/**
 * A failure for a toast or an alert. f50d's own messages are English sentences: they come after the Chinese name
 * of their code ("设备正忙：another radio job runs"); a message that is already Chinese (the client's own) as it is.
 */
export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const label = CODE_TEXT[e.code] ?? e.code;
    if (!e.message) return label;
    // ASCII only: f50d's text, which says what happened in its own words
    return /^[\x20-\x7e]+$/.test(e.message)
      ? `${label}：${e.message}`
      : e.message;
  }
  if (e instanceof Error) return e.message;
  return "操作失败";
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
  if (d > 0) return `${d} 天 ${h} 小时`;
  if (h > 0) return `${h} 小时 ${m} 分`;
  if (m > 0) return `${m} 分 ${s % 60} 秒`;
  return `${s} 秒`;
}

/** Unix seconds as an absolute time. */
export function formatTime(unix: number | null | undefined): string {
  if (!unix) return "—";
  return format(fromUnixTime(unix), "yyyy-MM-dd HH:mm:ss");
}

/** Unix seconds as "3 分钟前" (use with `formatTime` in a title). */
export function formatAgo(unix: number | null | undefined): string {
  if (!unix) return "—";
  return formatDistanceToNow(fromUnixTime(unix), {
    addSuffix: true,
    locale: zhCN,
  });
}

export function formatValue(v: unknown, unit = ""): string {
  if (v == null || v === "") return "—";
  return `${String(v)}${unit}`;
}

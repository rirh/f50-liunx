/**
 * How the proxy pages show nodes, rates and ages (from one-node's `web/src/app/pc/lib/format.ts`); bytes and dates
 * come from the console's own `lib/format`.
 */
import { t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { format, fromUnixTime } from "date-fns";
import type { ProxyNode, Subscription, Usage } from "../types";

/** Three significant digits ("1.40" MB, "21.9" KB, "313" MB) so a changing value keeps its width. */
export function byteParts(n: number): { value: string; unit: string } {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = Math.max(0, n);
  while (v >= 1000 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const value =
    i === 0 ? String(Math.round(v)) : v.toFixed(v < 10 ? 2 : v < 100 ? 1 : 0);
  return { value, unit: units[i] };
}

/** "1.4 MB/s". */
export const rateText = (bytesPerSecond: number) => {
  const { value, unit } = byteParts(bytesPerSecond);
  return `${Number(value)} ${unit}/s`;
};

/** "01:02:03": how long the proxy has run. */
export function clockDuration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const p = (v: number) => String(v).padStart(2, "0");
  return `${p(Math.floor(s / 3600))}:${p(Math.floor((s % 3600) / 60))}:${p(s % 60)}`;
}

/** "1:05", "12:40", "1:02:03": short connection ages. */
export function formatAge(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const p = (v: number) => String(v).padStart(2, "0");
  const h = Math.floor(s / 3600);
  return h > 0
    ? `${h}:${p(Math.floor((s % 3600) / 60))}:${p(s % 60)}`
    : `${Math.floor(s / 60)}:${p(s % 60)}`;
}

/** Wall-clock time of a Unix-ms stamp, with seconds. */
export const formatClock = (ms: number) => format(ms, "HH:mm:ss");

export const formatDay = (sec: number) =>
  format(fromUnixTime(sec), "yyyy-MM-dd");

export function usageParts(u: Usage) {
  return {
    used: `↑ ${formatBytes(u.upload)}  ↓ ${formatBytes(u.download)}`,
    total: u.total > 0 ? formatBytes(u.total) : t("proxy.usage.unlimited"),
    expire: u.expire > 0 ? formatDay(u.expire) : null,
  };
}

/** Delay tone: green under 800 ms, orange under 1500 ms, red beyond (the console's status colours). */
export const latencyTone = (ms: number) =>
  ms < 800 ? TONE.good : ms < 1500 ? TONE.warn : "text-destructive";

/** The console's green and orange (as the LAN and dashboard pages use them). */
export const TONE = {
  good: "text-[#248A3D] dark:text-[#30D158]",
  warn: "text-[#C93400] dark:text-[#FF9F0A]",
  goodBg: "bg-[#34C759]",
  warnBg: "bg-[#FF9500]",
};

const query = (n: Pick<ProxyNode, "link">) => {
  try {
    return new URL(n.link.replace(/^(\w+):\/\//, "onenode://"));
  } catch {
    return null;
  }
};

const PROTOCOL_LABEL: Record<string, string> = {
  vless: "VLESS",
  vmess: "VMess",
  trojan: "Trojan",
  hysteria2: "Hysteria2",
  shadowsocks: "Shadowsocks",
  socks: "SOCKS5",
  http: "HTTP",
};
const TRANSPORT_LABEL: Record<string, string> = {
  xhttp: "XHTTP",
  ws: "WebSocket",
  httpupgrade: "HTTPUpgrade",
  grpc: "gRPC",
};

const transportOf = (n: Pick<ProxyNode, "kind">, q: URLSearchParams) =>
  TRANSPORT_LABEL[n.kind] ?? TRANSPORT_LABEL[q.get("type") ?? ""];

type NodeLike = Pick<ProxyNode, "kind" | "protocol" | "link">;

/** The node row's caption as small tags: ["VLESS", "Reality", "Vision"], ["Hysteria2", "Port Hop"]. */
export function nodeTags(n: NodeLike): string[] {
  const u = query(n);
  const q = u?.searchParams ?? new URLSearchParams();
  const label = PROTOCOL_LABEL[n.protocol] ?? n.protocol.toUpperCase();
  const keep = (parts: (string | false | null | undefined)[]) =>
    parts.filter((p): p is string => Boolean(p));
  if (n.kind === "hy2")
    return keep(["Hysteria2", q.get("mport") && "Port Hop"]);
  if (n.kind === "ss")
    return keep([label, u && decodeURIComponent(u.username)]);
  if (n.protocol === "http" || n.protocol === "socks")
    return [n.link.startsWith("https://") ? "HTTPS" : label];
  const security = q.get("security");
  return keep([
    n.protocol ? label : "VLESS",
    security === "reality" && "Reality",
    transportOf(n, q),
    security === "tls" && "TLS",
    q.get("flow")?.includes("vision") && "Vision",
    n.protocol === "vless" &&
      (q.get("encryption") || "none") !== "none" &&
      "MLKEM",
  ]);
}

/** A subscription's heading: the user's name, else the provider's title, else its host. */
export function subscriptionTitle(s: Subscription) {
  if (s.name) return s.name;
  if (s.title) return s.title;
  try {
    return new URL(s.url).hostname;
  } catch {
    return s.url;
  }
}

/** Every node of the profile: subscriptions' first, then the local ones. */
export const allNodes = (p: {
  subscriptions: Subscription[];
  local_nodes: ProxyNode[];
}) => [...p.subscriptions.flatMap((s) => s.nodes), ...p.local_nodes];

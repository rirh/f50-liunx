import type { NetworkSecurity } from "./types";

/**
 * Whether the backend can join a network of this security: WPA2 (or WPA) with a passphrase, or open. A WPA3-only
 * network is refused (the driver has no SAE); OWE, EAP and WEP are never configured by the join.
 */
export function joinable(s: NetworkSecurity): boolean {
  return s === "WPA2/3" || s === "WPA2" || s === "WPA" || s === "open";
}

export function whyNotJoinable(s: NetworkSecurity): string | null {
  if (joinable(s)) return null;
  if (s === "WPA3") return "仅 WPA3：此设备的 Wi-Fi 驱动不支持 WPA3，无法加入";
  return `${s} 网络不受支持`;
}

export const SECURITY_LABEL: Record<NetworkSecurity, string> = {
  "WPA2/3": "WPA2/WPA3",
  WPA3: "WPA3",
  WPA2: "WPA2",
  WPA: "WPA",
  OWE: "OWE",
  EAP: "企业级 EAP",
  WEP: "WEP",
  open: "开放",
};

/** dBm as a word. */
export function signalText(dbm: number | null): string {
  if (dbm == null) return "—";
  const q = dbm >= -55 ? "强" : dbm >= -70 ? "中" : "弱";
  return `${dbm} dBm（${q}）`;
}

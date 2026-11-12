import { t, type Key } from "@/i18n";
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
  if (s === "WPA3") return t("network.client.security.wpa3Only");
  return t("network.client.security.unsupported", { security: s });
}

export const SECURITY_LABEL = {
  "WPA2/3": "network.client.security.label.WPA2/3",
  WPA3: "network.client.security.label.WPA3",
  WPA2: "network.client.security.label.WPA2",
  WPA: "network.client.security.label.WPA",
  OWE: "network.client.security.label.OWE",
  EAP: "network.client.security.label.EAP",
  WEP: "network.client.security.label.WEP",
  open: "network.client.security.label.open",
} satisfies Record<NetworkSecurity, Key>;

/** dBm as a word. */
export function signalText(dbm: number | null): string {
  if (dbm == null) return "—";
  const q =
    dbm >= -55
      ? t("network.client.signal.strong")
      : dbm >= -70
        ? t("network.client.signal.fair")
        : t("network.client.signal.weak");
  return t("network.client.signal.text", { dbm, quality: q });
}

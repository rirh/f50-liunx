/**
 * The hotspot's channel table, ported from `f50d/crates/f50-net/src/wifi/channels.rs`: the 5 GHz channels the
 * firmware offers without DFS per country, and the widest width each allows. Keep in step with the Rust side.
 */
import type { WifiBand } from "./types";

export const AP5_ALL = [36, 40, 44, 48, 149, 153, 157, 161, 165];
const LOW = [36, 40, 44, 48];
const HIGH = [149, 153, 157, 161, 165];
const NO_165 = [36, 40, 44, 48, 149, 153, 157, 161];
export const AP2_ALL = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

const NONE = ["00", "AM", "BH", "DZ", "TG", "YE"];
const ONLY_LOW = [
  "AF",
  "AI",
  "AN",
  "AW",
  "AZ",
  "BL",
  "BT",
  "BW",
  "BY",
  "EG",
  "ET",
  "FO",
  "GE",
  "GF",
  "GI",
  "GL",
  "GP",
  "IM",
  "IR",
  "JP",
  "KE",
  "KH",
  "KW",
  "LK",
  "LS",
  "MA",
  "MF",
  "MQ",
  "MR",
  "MW",
  "NA",
  "OM",
  "PF",
  "PM",
  "QA",
  "RE",
  "RS",
  "SA",
  "SM",
  "SR",
  "SX",
  "SY",
  "TD",
  "TN",
  "TR",
  "TZ",
  "UZ",
  "VA",
  "VC",
  "WF",
  "WS",
  "YT",
  "ZW",
];
const ONLY_HIGH = ["BD", "BO", "BZ", "CU", "EC", "NG", "PK"];
const WITHOUT_165 = ["ID", "KN", "KP", "LC"];

/** The 5 GHz channels for country `cc`; empty: none, the hotspot has to use 2.4 GHz. */
export function ap5Channels(cc: string): number[] {
  const c = cc.toUpperCase();
  if (NONE.includes(c)) return [];
  if (ONLY_LOW.includes(c)) return LOW;
  if (ONLY_HIGH.includes(c)) return HIGH;
  if (WITHOUT_165.includes(c)) return NO_165;
  return AP5_ALL;
}

/** The channels `band` offers in `cc`. */
export function channelsFor(band: WifiBand, cc: string): number[] {
  return band === "2.4" ? AP2_ALL : ap5Channels(cc);
}

/** The widest 5 GHz width in MHz (80, 40 or 20) whose channels `cc` all allows; 165 is 20 MHz only. */
export function ap5Width(cc: string, ch: number): number {
  const allowed = ap5Channels(cc);
  const block = [36, 40, 44, 48].includes(ch)
    ? [36, 40, 44, 48]
    : [149, 153, 157, 161].includes(ch)
      ? [149, 153, 157, 161]
      : null;
  if (!block) return 20;
  const pair =
    ch === 36 || ch === 40
      ? [36, 40]
      : ch === 44 || ch === 48
        ? [44, 48]
        : ch === 149 || ch === 153
          ? [149, 153]
          : [157, 161];
  if (block.every((c) => allowed.includes(c))) return 80;
  if (pair.every((c) => allowed.includes(c))) return 40;
  return 20;
}

/**
 * The widths to offer: 2.4 GHz up to 40; 5 GHz up to the channel's widest (any up to 80 on "auto": the radio caps
 * it to the channel it picks).
 */
export function widthsFor(
  band: WifiBand,
  cc: string,
  channel: string,
): string[] {
  if (band === "2.4") return ["auto", "20", "40"];
  const ch = Number(channel);
  const max =
    channel === "auto" || !Number.isFinite(ch) ? 80 : ap5Width(cc, ch);
  return ["auto", ...["20", "40", "80"].filter((w) => Number(w) <= max)];
}

/** The width in an `htmode` (`VHT80` → "80"), "auto" when it has none. */
export function widthOf(htmode: string | null): string {
  const m = htmode?.match(/(20|40|80)$/);
  return m ? m[1] : "auto";
}

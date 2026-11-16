/**
 * 应用分流's catalog (one-node's `web/src/app/pc/lib/app-packs.ts`: the same ids, matches and order). Each app's match
 * is written into the config's apps[] on save, so the engine never needs the ids. Every rule_set tag below exists
 * in the One Node rules library (`<rule_sets_url>/<tag>.json`); where there is no geoip-<x>, ip_cidr is inline.
 * Names are brands; the hints are read in the current language (`proxy.apps.hint.<id>`).
 */
import { t } from "@/i18n";
import {
  AsteriskIcon,
  type LucideIcon,
  MapPinIcon,
  MonitorIcon,
  ShieldBanIcon,
} from "lucide-react";
import apple from "../assets/app-icons/apple.com.ico";
import chatgpt from "../assets/app-icons/chatgpt.com.webp";
import claude from "../assets/app-icons/claude.ai.ico";
import disney from "../assets/app-icons/disneyplus.com.ico";
import gemini from "../assets/app-icons/gemini.google.com.png";
import github from "../assets/app-icons/github.com.ico";
import netflix from "../assets/app-icons/netflix.com.ico";
import spotify from "../assets/app-icons/spotify.com.ico";
import steam from "../assets/app-icons/store.steampowered.com.ico";
import telegram from "../assets/app-icons/telegram.org.ico";
import tiktok from "../assets/app-icons/tiktok.com.png";
import x from "../assets/app-icons/x.com.png";
import youtube from "../assets/app-icons/youtube.com.ico";

/** A rulespec default rule without action or outbound. */
export type AppMatch = {
  rule_set?: string[];
  domain?: string[];
  domain_suffix?: string[];
  ip_cidr?: string[];
};

export type AppPackId =
  | "openai"
  | "anthropic"
  | "gemini"
  | "netflix"
  | "youtube"
  | "disney"
  | "spotify"
  | "telegram"
  | "twitter"
  | "apple"
  | "microsoft"
  | "steam"
  | "github"
  | "tiktok"
  | "ads"
  | "cn";

export type AppPack = {
  id: AppPackId;
  /** A brand, or (ads, cn) a key of the name. */
  name: string;
  /** Bundled brand mark, available offline. */
  logo?: string;
  icon: LucideIcon;
  match: AppMatch;
  /** Where it goes when first switched on (default proxy). */
  outbound?: "direct" | "block";
};

/** Telegram's own networks (AS62041 and friends); there is no geoip-telegram. */
const TELEGRAM_IPS = [
  "91.105.192.0/23",
  "91.108.4.0/22",
  "91.108.8.0/22",
  "91.108.12.0/22",
  "91.108.16.0/22",
  "91.108.20.0/22",
  "91.108.56.0/22",
  "95.161.64.0/20",
  "149.154.160.0/20",
  "185.76.151.0/24",
  "2001:67c:4e8::/48",
  "2001:b28:f23c::/47",
  "2001:b28:f23f::/48",
  "2a0a:f280::/32",
];

export const APP_PACKS: readonly AppPack[] = [
  {
    id: "openai",
    logo: chatgpt,
    name: "ChatGPT",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-openai"] },
  },
  {
    id: "anthropic",
    logo: claude,
    name: "Claude",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-anthropic"] },
  },
  {
    id: "gemini",
    logo: gemini,
    name: "Gemini",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-google-gemini"] },
  },
  {
    id: "netflix",
    logo: netflix,
    name: "Netflix",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-netflix"] },
  },
  {
    id: "youtube",
    logo: youtube,
    name: "YouTube",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-youtube"] },
  },
  {
    id: "disney",
    logo: disney,
    name: "Disney+",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-disney"] },
  },
  {
    id: "spotify",
    logo: spotify,
    name: "Spotify",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-spotify"] },
  },
  {
    id: "telegram",
    logo: telegram,
    name: "Telegram",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-telegram"], ip_cidr: TELEGRAM_IPS },
  },
  {
    id: "twitter",
    logo: x,
    name: "X",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-twitter"] },
  },
  {
    id: "apple",
    logo: apple,
    name: "Apple",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-apple"] },
  },
  {
    id: "microsoft",
    name: "Microsoft",
    icon: MonitorIcon,
    match: { rule_set: ["geosite-microsoft"] },
  },
  {
    id: "steam",
    logo: steam,
    name: "Steam",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-steam"] },
  },
  {
    id: "github",
    logo: github,
    name: "GitHub",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-github"] },
  },
  {
    id: "tiktok",
    logo: tiktok,
    name: "TikTok",
    icon: AsteriskIcon,
    match: { rule_set: ["geosite-tiktok"] },
  },
  {
    id: "ads",
    name: "",
    icon: ShieldBanIcon,
    match: { rule_set: ["geosite-category-ads-all"] },
    outbound: "block",
  },
  {
    id: "cn",
    name: "",
    icon: MapPinIcon,
    match: { rule_set: ["geosite-cn", "geoip-cn"] },
    outbound: "direct",
  },
];

export const appPack = (id: string) => APP_PACKS.find((p) => p.id === id);

/** The pack's name in the current language (brands as they are). */
export const appName = (p: AppPack) =>
  p.id === "ads" || p.id === "cn" ? t(`proxy.apps.name.${p.id}`) : p.name;
export const appHint = (p: AppPack) => t(`proxy.apps.hint.${p.id}`);

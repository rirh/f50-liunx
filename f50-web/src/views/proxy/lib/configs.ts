/** 分流 configs: the built-in default, new ones, names (one-node's `web/src/app/pc/lib/configs.ts`). */
import { t } from "@/i18n";
import {
  CirclePauseIcon,
  EarthIcon,
  type LucideIcon,
  SplitIcon,
} from "lucide-react";
import { appPack } from "./app-packs";
import { formatDoc } from "./route-doc";
import type { Config, Routing } from "../types";

export const BUILTIN_ID = "default";
export const isBuiltin = (c: Pick<Config, "id" | "builtin">) =>
  c.builtin || c.id === BUILTIN_ID;

/** What the default config does, in plain words (keep in step with f50d's built-in text and the engine's tail). */
export const builtinSummary = (): { what: string; proxy: boolean }[] => [
  { what: t("proxy.rules.builtin.ai"), proxy: true },
  { what: t("proxy.rules.builtin.foreign"), proxy: true },
  { what: t("proxy.rules.builtin.cn"), proxy: false },
  { what: t("proxy.rules.builtin.rest"), proxy: true },
];

/** A new config starts as the default in editable form: the AI apps on, the rest via the node. */
export function newConfigText(): string {
  const apps = ["anthropic", "openai", "gemini"].flatMap((id) => {
    const pack = appPack(id);
    return pack ? [{ app: id, outbound: "proxy", match: pack.match }] : [];
  });
  return formatDoc({ apps, final: "proxy" });
}

/** A unique default name: 配置 2, 配置 3, … */
export function nextConfigName(configs: Config[]): string {
  const names = new Set(configs.map((c) => c.name));
  for (let i = configs.length; ; i++) {
    const name = t("proxy.rules.nthConfig", { n: i });
    if (!names.has(name)) return name;
  }
}

/** A config added from a link: named after its file (rules.json → rules), else its host. */
export function nameFromUrl(url: string, configs: Config[]): string {
  let name = "";
  try {
    const u = new URL(url);
    const file = decodeURIComponent(
      u.pathname.split("/").filter(Boolean).at(-1) ?? "",
    );
    name = file.replace(/\.(json|conf|txt)$/i, "") || u.hostname;
  } catch {
    // not a URL: f50d refuses it anyway
  }
  if (!name || configs.some((c) => c.name === name))
    return nextConfigName(configs);
  return name.slice(0, 40);
}

export const ROUTINGS: Routing[] = ["rule", "global", "direct"];

export const ROUTING_ICON: Record<Routing, LucideIcon> = {
  rule: SplitIcon,
  global: EarthIcon,
  direct: CirclePauseIcon,
};

export const routingLabel = (r: Routing) => t(`proxy.routing.${r}`);
export const routingHint = (r: Routing) => t(`proxy.routing.${r}Hint`);

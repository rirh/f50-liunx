/** Where traffic can go in a config: the built-in outbounds, its 策略组 or a node by name. */
import { t } from "@/i18n";

export const BUILTIN_OUTBOUNDS = ["proxy", "direct", "block"] as const;

export const outboundLabel = (o: string) =>
  o === "proxy" || o === "direct" || o === "block"
    ? t(`proxy.outbound.${o}`)
    : o;

/** What an outbound refers to, for the label and the missing hint. */
export function outboundKind(
  value: string,
  groups: string[],
  nodes: string[],
): "builtin" | "group" | "node" | "missing" {
  if ((BUILTIN_OUTBOUNDS as readonly string[]).includes(value))
    return "builtin";
  if (groups.includes(value)) return "group";
  if (nodes.includes(value)) return "node";
  return "missing";
}

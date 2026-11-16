/** The engine's check of a config, in one line (one-node's `rules-check.tsx`). */
import { t } from "@/i18n";
import { outboundLabel } from "./outbound";
import type { RulesInfo } from "../types";

export function checkSummary(
  text: string,
  info: RulesInfo | null | undefined,
  builtin = false,
): string {
  if ((!text.trim() && !builtin) || !info) return t("proxy.rules.check.none");
  return t("proxy.rules.check.summary", {
    count: info.source,
    final: outboundLabel(info.final),
  });
}

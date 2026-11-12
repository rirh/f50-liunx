/** Formatting the cellular pages share. */
import { t } from "@/i18n";

/** A reading's age in milliseconds as "3 秒前". */
export function formatAgeMs(ms: number | null | undefined): string {
  if (ms == null) return "—";
  if (ms < 1000) return t("cellular.shared.age.now");
  const s = Math.round(ms / 1000);
  if (s < 60) return t("cellular.shared.age.seconds", { n: s });
  const m = Math.floor(s / 60);
  if (m < 60) return t("cellular.shared.age.minutes", { n: m });
  return t("cellular.shared.age.hours", { n: Math.floor(m / 60) });
}

/** A number with its unit, or "—". */
export function unit(v: number | string | null | undefined, u: string): string {
  if (v == null || v === "") return "—";
  return `${v} ${u}`;
}

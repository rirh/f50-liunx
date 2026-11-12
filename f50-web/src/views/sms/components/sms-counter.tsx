import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { MAX_CHARS, MAX_PARTS, smsEncoding } from "../lib/encoding";

/** "GSM-7 · 2 条 · 剩 140" under the composer: the coding, the parts and what the last part still holds. */
export function SmsCounter({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  const e = smsEncoding(text);
  const over = e.chars > MAX_CHARS || e.parts > MAX_PARTS;
  return (
    <span
      className={cn(
        "text-xs tabular-nums",
        over ? "text-destructive" : "text-muted-foreground",
        className,
      )}
      aria-live="polite"
      title={t(
        e.coding === "gsm" ? "sms.counter.titleGsm" : "sms.counter.titleUcs2",
        { chars: e.chars, units: e.units, perPart: e.perPart },
      )}
    >
      {t("sms.counter.text", {
        coding: e.coding === "gsm" ? "GSM-7" : "UCS-2",
        parts: e.parts,
        max: MAX_PARTS,
        remaining: e.remaining,
      })}
    </span>
  );
}

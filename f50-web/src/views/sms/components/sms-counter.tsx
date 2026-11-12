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
      title={`${e.chars} 字 · ${e.units} 个${e.coding === "gsm" ? "七位字符" : " UTF-16 单元"} · 每条 ${e.perPart}`}
    >
      {e.coding === "gsm" ? "GSM-7" : "UCS-2"} · {e.parts}/{MAX_PARTS} 条 · 剩{" "}
      {e.remaining}
    </span>
  );
}

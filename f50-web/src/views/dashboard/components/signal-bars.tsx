import { t } from "@/i18n";
import { cn } from "@/lib/utils";

/** 0-5 bars, as the LEDs show them. */
export function SignalBars({ bars }: { bars: number }) {
  const n = Math.max(0, Math.min(5, Math.round(bars)));
  return (
    <span
      role="img"
      aria-label={t("dashboard.signalBars", { n })}
      className="inline-flex h-5 items-end gap-0.5"
    >
      {[1, 2, 3, 4, 5].map((i) => (
        <span
          key={i}
          className={cn("w-1.5 rounded-sm", i <= n ? "bg-primary" : "bg-muted")}
          style={{ height: `${i * 20}%` }}
        />
      ))}
    </span>
  );
}

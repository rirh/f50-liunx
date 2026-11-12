import { t } from "@/i18n";
import { cn } from "@/lib/utils";

/** 0-5 bars, as the LEDs and the dashboard show them. */
export function SignalBars({ bars }: { bars: number }) {
  return (
    <div
      className="flex h-5 items-end gap-0.5"
      role="img"
      aria-label={t("cellular.signal.bars", { n: bars })}
    >
      {[1, 2, 3, 4, 5].map((n) => (
        <span
          key={n}
          className={cn(
            "w-1.5 rounded-sm",
            n <= bars ? "bg-primary" : "bg-muted",
          )}
          style={{ height: `${n * 20}%` }}
        />
      ))}
    </div>
  );
}

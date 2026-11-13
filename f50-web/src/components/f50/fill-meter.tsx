import type { ReactNode } from "react";
import { TruncatedText } from "@/components/f50/truncated-text";
import { cn } from "@/lib/utils";
import type { LucideIcon } from "lucide-react";

/**
 * A figure with its share drawn as the box's background: green, amber from 70 %, red from 90 %. The label (and an
 * icon) on the left, the value on the right, one detail line under them.
 */
export function FillMeter({
  icon: Icon,
  className,
  label,
  value,
  percent,
  hint,
}: {
  icon?: LucideIcon;
  className?: string;
  label: string;
  value: ReactNode;
  percent: number | null;
  hint?: string;
}) {
  const p =
    percent == null ? null : Math.min(100, Math.max(0, Math.round(percent)));
  const critical = p != null && p >= 90;
  const warning = p != null && p >= 70;
  return (
    <div
      className={cn(
        "relative min-w-0 overflow-hidden rounded-lg border border-foreground/5 bg-muted/25 px-3 py-2",
        className,
      )}
    >
      {p != null && p > 0 && (
        <span
          aria-hidden
          className={cn(
            "absolute inset-y-0 left-0 transition-[width] duration-500",
            critical
              ? "bg-destructive/15"
              : warning
                ? "bg-amber-500/18"
                : "bg-emerald-500/14",
          )}
          style={{ width: `${p}%` }}
        />
      )}
      <div className="relative flex items-baseline justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5 text-sm text-muted-foreground">
          {Icon && <Icon aria-hidden className="size-3.5 shrink-0" />}
          <span className="truncate">{label}</span>
        </span>
        <span
          className={cn(
            "shrink-0 text-sm font-semibold tabular-nums",
            critical
              ? "text-destructive"
              : warning
                ? "text-amber-600 dark:text-amber-400"
                : "text-foreground",
          )}
        >
          {value}
        </span>
      </div>
      <TruncatedText className="relative tabular-nums text-2xs leading-tight text-muted-foreground">
        {hint ?? "—"}
      </TruncatedText>
    </div>
  );
}

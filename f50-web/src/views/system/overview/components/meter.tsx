import type { ReactNode } from "react";
import { Progress } from "@/components/ui/progress";

/** One live figure: a label, its value and, when it is a share, a bar. */
export function Meter({
  label,
  value,
  hint,
  percent,
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  percent?: number | null;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5 rounded-lg border p-3">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="truncate text-lg font-semibold tabular-nums">
        {value}
      </span>
      {percent !== undefined && (
        <Progress value={percent ?? 0} aria-label={label} />
      )}
      {hint && (
        <span className="truncate text-xs text-muted-foreground">{hint}</span>
      )}
    </div>
  );
}

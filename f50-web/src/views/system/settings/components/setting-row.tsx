import type { ReactNode } from "react";

/**
 * One setting in the General card: its name, the device's current value (read-only) and an optional line under it,
 * with the button that opens its dialog on the right. The value truncates so the button never wraps off on a phone.
 */
export function SettingRow({
  label,
  value,
  hint,
  action,
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  action: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-3 py-3 first:pt-0 last:pb-0">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-xs text-muted-foreground">{label}</span>
        <span className="truncate text-sm font-medium">{value ?? "—"}</span>
        {hint && (
          <span className="text-xs text-muted-foreground tabular-nums">
            {hint}
          </span>
        )}
      </div>
      <div className="shrink-0">{action}</div>
    </div>
  );
}

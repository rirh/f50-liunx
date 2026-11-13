import type { ReactNode } from "react";
import { TruncatedText } from "@/components/f50/truncated-text";
import { cn } from "@/lib/utils";

/** An inset group of rows under a small heading, as in a settings list. */
export function Group({
  title,
  children,
}: {
  title?: string;
  children: ReactNode;
}) {
  return (
    <section className="flex min-w-0 flex-col gap-1.5">
      {title && (
        <h3 className="px-3 text-xs font-medium text-muted-foreground">
          {title}
        </h3>
      )}
      <div className="divide-y divide-border/60 overflow-hidden rounded-xl bg-muted/40">
        {children}
      </div>
    </section>
  );
}

/**
 * One fact: the label on the left, the value on the right. A long value is cut with "…" (full text in a tooltip), or
 * with `wrap` goes on to more lines, for values that must be read whole (identifiers, QoS).
 */
export function Row({
  label,
  children,
  mono,
  wrap,
}: {
  label: string;
  children: ReactNode;
  mono?: boolean;
  wrap?: boolean;
}) {
  return (
    <div className="flex min-h-9 items-center justify-between gap-3 px-3 py-1.5 text-sm">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      {wrap ? (
        <span
          className={cn(
            "min-w-0 text-right break-words tabular-nums",
            mono && "text-xs",
          )}
        >
          {children ?? "—"}
        </span>
      ) : typeof children === "string" || typeof children === "number" ? (
        <TruncatedText
          className={cn("text-right tabular-nums", mono && "text-xs")}
        >
          {children}
        </TruncatedText>
      ) : (
        <span
          className={cn(
            "min-w-0 truncate text-right tabular-nums",
            mono && "text-xs",
          )}
        >
          {children ?? "—"}
        </span>
      )}
    </div>
  );
}

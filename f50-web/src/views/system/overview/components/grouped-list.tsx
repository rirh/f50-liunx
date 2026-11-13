import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** An inset group of rows under a small heading, as in a settings list. */
export function Group({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="flex min-w-0 flex-col gap-1.5">
      <h3 className="px-3 text-xs font-medium text-muted-foreground">
        {title}
      </h3>
      <div className="divide-y divide-border/60 overflow-hidden rounded-xl bg-muted/40">
        {children}
      </div>
    </section>
  );
}

/** One fact: the label on the left, the value on the right. */
export function Row({
  label,
  children,
  mono,
}: {
  label: string;
  children: ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="flex min-h-9 items-center justify-between gap-3 px-3 py-1.5 text-[13px]">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span
        title={typeof children === "string" ? children : undefined}
        className={cn(
          "min-w-0 truncate text-right tabular-nums",
          mono && "font-mono text-xs",
        )}
      >
        {children ?? "—"}
      </span>
    </div>
  );
}

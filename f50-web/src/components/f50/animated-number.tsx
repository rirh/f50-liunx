import { useRef } from "react";
import { useTweenedText } from "@/hooks/use-motion";
import { cn } from "@/lib/utils";

/**
 * A figure that eases to each new value (GSAP; at once with reduced motion). `format` turns the in-between number
 * into text, so units and digits stay as the page writes them. Screen readers get the final value only.
 */
export function AnimatedNumber({
  value,
  format,
  className,
}: {
  value: number | null | undefined;
  format: (n: number | null) => string;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  useTweenedText(ref, value, format);
  return (
    <span className={cn("tabular-nums", className)}>
      {/* filled by the tween (a layout effect, before paint); React never writes it, so a render cannot jump it */}
      <span ref={ref} aria-hidden />

      <span className="sr-only">{format(value ?? null)}</span>
    </span>
  );
}

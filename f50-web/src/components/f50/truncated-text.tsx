import { type ReactNode, useRef, useState } from "react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

/**
 * One line cut off with "…" when it does not fit; hovering or focusing it then shows the whole text in a tooltip.
 * Text that fits gets no tooltip.
 */
export function TruncatedText({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <Tooltip
      open={open}
      onOpenChange={(next) => {
        const el = ref.current;
        setOpen(next && !!el && el.scrollWidth > el.clientWidth);
      }}
    >
      <TooltipTrigger asChild>
        <span ref={ref} className={cn("block min-w-0 truncate", className)}>
          {children}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-80 break-all">
        {children}
      </TooltipContent>
    </Tooltip>
  );
}

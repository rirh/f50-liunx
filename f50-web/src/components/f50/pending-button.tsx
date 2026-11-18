import type { ComponentProps, ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";

/**
 * A button for a request: disabled and aria-busy while it runs, its label sweeping. The sweep's span lays its
 * content out as the button does (a row with the button's gap), so an icon passed in the children stays on the line.
 */
export function PendingButton({
  pending = false,
  icon,
  children,
  disabled,
  ...props
}: ComponentProps<typeof Button> & { pending?: boolean; icon?: ReactNode }) {
  return (
    <Button disabled={disabled || pending} aria-busy={pending} {...props}>
      {icon}
      {pending ? (
        <SweepShine className="inline-flex items-center gap-[inherit] whitespace-nowrap">
          {children}
        </SweepShine>
      ) : (
        children
      )}
    </Button>
  );
}

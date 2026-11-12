import type { ComponentProps, ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";

/** A button for a request: disabled and aria-busy while it runs, its label sweeping. */
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
      {pending ? <SweepShine>{children}</SweepShine> : children}
    </Button>
  );
}

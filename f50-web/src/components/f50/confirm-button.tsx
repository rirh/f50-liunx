import { type ReactNode, useState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";

/**
 * A button for an action that cannot be taken back (reboot, delete, restart the modem): it asks first, then runs
 * `onConfirm`; while that runs the button keeps its width, is disabled and says so.
 */
export function ConfirmButton({
  children,
  title,
  description,
  confirmText,
  pending = false,
  destructive = true,
  variant,
  size,
  disabled,
  extra,
  className,
  onConfirm,
}: {
  children: ReactNode;
  title: string;
  description: ReactNode;
  confirmText?: string;
  pending?: boolean;
  destructive?: boolean;
  variant?: React.ComponentProps<typeof Button>["variant"];
  size?: React.ComponentProps<typeof Button>["size"];
  disabled?: boolean;
  /** Controls inside the dialog, between the text and the buttons (an "also delete from the SIM" checkbox). */
  extra?: ReactNode;
  className?: string;
  onConfirm: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger asChild>
        <Button
          variant={variant ?? (destructive ? "destructive" : "default")}
          size={size}
          className={className}
          disabled={disabled || pending}
          aria-busy={pending}
        >
          {pending ? <SweepShine>{children}</SweepShine> : children}
        </Button>
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        {extra}
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.action.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            variant={destructive ? "destructive" : "default"}
            onClick={() => {
              setOpen(false);
              onConfirm();
            }}
          >
            {confirmText ?? t("common.action.confirm")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

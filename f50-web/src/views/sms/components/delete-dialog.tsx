import { type ReactNode, useId, useState } from "react";
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
import { Checkbox } from "@/components/ui/checkbox";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { t } from "@/i18n";

/** Asks before deleting, with the choice to delete the modem's (SIM) copies too. */
export function DeleteDialog({
  trigger,
  title,
  description,
  onConfirm,
  open: openProp,
  onOpenChange,
}: {
  /** The button that opens it; omit and pass `open` to open it from elsewhere (a menu item). */
  trigger?: ReactNode;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  title: string;
  description: ReactNode;
  onConfirm: (sim: boolean) => void;
}) {
  const [openState, setOpenState] = useState(false);
  const open = openProp ?? openState;
  const setOpen = (v: boolean) => {
    setOpenState(v);
    onOpenChange?.(v);
  };
  const [sim, setSim] = useState(false);
  const id = useId();
  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      {trigger && <AlertDialogTrigger asChild>{trigger}</AlertDialogTrigger>}
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <Field orientation="horizontal">
          <Checkbox
            id={id}
            checked={sim}
            onCheckedChange={(v) => setSim(v === true)}
          />
          <FieldContent>
            <FieldLabel htmlFor={id}>
              {t("sms.deleteDialog.alsoSim")}
            </FieldLabel>
            <FieldDescription>
              {t("sms.deleteDialog.alsoSimDescription")}
            </FieldDescription>
          </FieldContent>
        </Field>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.action.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={() => {
              setOpen(false);
              onConfirm(sim);
            }}
          >
            {t("common.action.delete")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

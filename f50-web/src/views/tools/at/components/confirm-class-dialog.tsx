import { useEffect, useRef } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { t } from "@/i18n";
import type { PendingConfirm } from "./use-at-send";
import { CLASS_LABEL } from "../messages";

/** f50d asked for `confirm`: says what class of command this is and what it does, then resends or drops it. */
export function ConfirmClassDialog({
  pending,
  onConfirm,
  onCancel,
}: {
  pending: PendingConfirm | null;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const label = pending?.cls ? CLASS_LABEL[pending.cls] : null;
  // closing after "确认发送" is not a cancel
  const accepted = useRef(false);
  useEffect(() => {
    if (pending) accepted.current = false;
  }, [pending]);
  return (
    <AlertDialog
      open={pending !== null}
      onOpenChange={(o) => {
        if (!o && !accepted.current) onCancel();
        accepted.current = false;
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="flex flex-wrap items-center gap-2">
            {t("tools.at.confirm.title")}
            {pending?.cls && (
              <Badge variant="secondary">
                {label ? t(label.title) : pending.cls}
              </Badge>
            )}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {label ? t(label.detail) : pending?.message}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {pending && (
          <code className="rounded-md bg-muted px-2 py-1.5 font-mono text-sm break-all">
            {pending.values.command}
          </code>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.action.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={() => {
              accepted.current = true;
              onConfirm();
            }}
          >
            {t("tools.at.confirm.send")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

import type { ReactNode } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@/components/ui/drawer";
import { useIsMobile } from "@/hooks/use-mobile";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { PencilIcon } from "lucide-react";

const WIDTH = { sm: "sm:max-w-md", md: "sm:max-w-lg", lg: "sm:max-w-2xl" };

/**
 * A panel opened on demand: a centred dialog on a wide screen, a bottom drawer on a phone (thumb reach). The body
 * scrolls; the header and footer stay put, so Save is always in view.
 */
export function ResponsiveDialog({
  open,
  onOpenChange,
  title,
  description,
  footer,
  size = "md",
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  footer?: ReactNode;
  size?: keyof typeof WIDTH;
  children: ReactNode;
}) {
  const mobile = useIsMobile();
  if (mobile)
    return (
      <Drawer open={open} onOpenChange={onOpenChange}>
        <DrawerContent className="max-h-[90dvh]">
          <DrawerHeader className="text-left">
            <DrawerTitle>{title}</DrawerTitle>
            {description && (
              <DrawerDescription>{description}</DrawerDescription>
            )}
          </DrawerHeader>
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-2">
            {children}
          </div>
          {footer && (
            <DrawerFooter className="flex-col-reverse border-t pb-[max(1rem,env(safe-area-inset-bottom))]">
              {footer}
            </DrawerFooter>
          )}
        </DrawerContent>
      </Drawer>
    );
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className={cn(
          "flex max-h-[min(44rem,calc(100dvh-4rem))] flex-col gap-0 p-0",
          WIDTH[size],
        )}
      >
        <DialogHeader className="p-5 pb-3">
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 pt-1 pb-5">
          {children}
        </div>
        {footer && (
          <DialogFooter className="m-0 rounded-b-xl border-t bg-muted/50 px-5 py-3">
            {footer}
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}

/**
 * A settings form in a responsive dialog: the fields as children (a `<form id={formId}>`), and Cancel and Save in the
 * footer. Save submits the form by id and stays disabled until something changed. The caller closes it on success
 * and resets the form when it closes, so reopening shows the device's values again.
 */
export function FormDialog({
  open,
  onOpenChange,
  title,
  description,
  formId,
  pending,
  dirty = true,
  submitText,
  size,
  extra,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  formId: string;
  pending: boolean;
  /** Save is enabled only when the form changed. */
  dirty?: boolean;
  submitText?: ReactNode;
  size?: keyof typeof WIDTH;
  /** A secondary action at the footer's start (e.g. reset to defaults). */
  extra?: ReactNode;
  children: ReactNode;
}) {
  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={(o) => !pending && onOpenChange(o)}
      title={title}
      description={description}
      size={size}
      footer={
        <>
          {extra && <div className="sm:me-auto">{extra}</div>}
          <Button
            type="button"
            variant="outline"
            disabled={pending}
            onClick={() => onOpenChange(false)}
          >
            {t("common.action.cancel")}
          </Button>
          <PendingButton
            type="submit"
            form={formId}
            pending={pending}
            disabled={!dirty}
          >
            {submitText ?? t("common.action.save")}
          </PendingButton>
        </>
      }
    >
      {children}
    </ResponsiveDialog>
  );
}

/** The card header's "Edit" button that opens a settings dialog. */
export function EditButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children?: ReactNode;
}) {
  return (
    <Button variant="outline" size="sm" disabled={disabled} onClick={onClick}>
      <PencilIcon data-icon="inline-start" />
      {children ?? t("common.action.edit")}
    </Button>
  );
}

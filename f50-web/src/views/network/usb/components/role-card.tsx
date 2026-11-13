import { useState } from "react";
import { EditButton, FormDialog } from "@/components/f50/form-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { setUsb, usbKeys } from "../api";
import type { UsbInfo, UsbSetArgs } from "../types";

const schema = z.object({
  role: z.enum(["host", "device"]),
  at_boot: z.boolean(),
});
type RoleForm = z.infer<typeof schema>;

/**
 * The port's role now, and the host role at every boot (device is the boot default). The status card shows both, so
 * they open from its header in a dialog; switching the role asks once more, since USB networking and adb drop.
 */
export function UsbRoleDialog({ info }: { info: UsbInfo }) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const initial: RoleForm = {
    role: info.role === "host" ? "host" : "device",
    at_boot: info.role_boot === "host",
  };
  const form = useForm<RoleForm>({
    resolver: zodResolver(schema),
    defaultValues: initial,
  });
  const [pending, setPending] = useState<UsbSetArgs | null>(null);
  const save = useMutation({
    mutationFn: setUsb,
    onSuccess: (r) => {
      setOpen(false);
      toast.success(
        r.pending
          ? t("network.usb.roleCard.saved")
          : t("network.usb.adb.saved"),
      );
      qc.invalidateQueries({ queryKey: usbKeys.all });
      // the switch runs a moment after the reply
      if (r.pending)
        setTimeout(() => qc.invalidateQueries({ queryKey: usbKeys.all }), 1500);
    },
    onError: (e) =>
      reportFormError(e, form.setError, { role: "role", scope: "at_boot" }),
  });
  const role = useWatch({ control: form.control, name: "role" });
  const supported = info.host_supported && info.role !== "unavailable";

  const onSubmit = (v: RoleForm) => {
    if (
      v.role === initial.role &&
      (v.role === "device" || v.at_boot === initial.at_boot)
    ) {
      toast.info(t("network.shared.noChanges"));
      return;
    }
    const args: UsbSetArgs =
      v.role === "host"
        ? { role: "host", scope: v.at_boot ? "permanent" : "once" }
        : { role: "device" };
    if (v.role !== initial.role) setPending(args);
    else save.mutate(args);
  };

  return (
    <>
      <EditButton
        onClick={() => {
          form.reset(initial);
          setOpen(true);
        }}
      >
        {t("network.usb.roleCard.title")}
      </EditButton>
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("network.usb.roleCard.title")}
        description={t("network.usb.roleCard.description")}
        formId="usb-role"
        pending={save.isPending}
        dirty={supported && form.formState.isDirty}
        submitText={t("common.action.apply")}
      >
        <form
          id="usb-role"
          onSubmit={form.handleSubmit(onSubmit)}
          noValidate
          className="flex flex-col gap-4"
        >
          {!supported && (
            <Alert>
              <AlertTitle>
                {t("network.usb.roleCard.unsupported.title")}
              </AlertTitle>
              <AlertDescription>
                {t("network.usb.roleCard.unsupported.description")}
              </AlertDescription>
            </Alert>
          )}
          <FieldGroup>
            <Controller
              control={form.control}
              name="role"
              render={({ field, fieldState }) => (
                <Field
                  data-invalid={fieldState.invalid}
                  data-disabled={!supported || undefined}
                >
                  <FieldLabel>{t("network.usb.roleCard.role")}</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    disabled={!supported}
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    <ToggleGroupItem value="device">
                      {t("network.usb.roleCard.device")}
                    </ToggleGroupItem>
                    <ToggleGroupItem value="host">
                      {t("network.usb.status.host")}
                    </ToggleGroupItem>
                  </ToggleGroup>
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="at_boot"
              render={({ field, fieldState }) => (
                <Field
                  orientation="horizontal"
                  data-invalid={fieldState.invalid}
                  data-disabled={role !== "host" || undefined}
                >
                  <Switch
                    id="usb-at-boot"
                    checked={role === "host" && field.value}
                    disabled={!supported || role !== "host"}
                    onCheckedChange={field.onChange}
                    aria-invalid={fieldState.invalid}
                  />
                  <FieldLabel htmlFor="usb-at-boot">
                    {t("network.usb.roleCard.atBoot")}
                  </FieldLabel>
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
            <FieldDescription>
              {t("network.usb.roleCard.atBootHint")}
            </FieldDescription>
          </FieldGroup>
        </form>
      </FormDialog>
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(o) => !o && setPending(null)}
        title={
          pending?.role === "host"
            ? t("network.usb.roleCard.confirmHost.title")
            : t("network.usb.roleCard.confirmDevice.title")
        }
        confirmText={t("network.usb.roleCard.switch")}
        destructive
        description={
          pending?.role === "host" ? (
            <span>{t("network.usb.roleCard.confirmHost.description")}</span>
          ) : (
            <span>{t("network.usb.roleCard.confirmDevice.description")}</span>
          )
        }
        onConfirm={() => pending && save.mutate(pending)}
      />
    </>
  );
}

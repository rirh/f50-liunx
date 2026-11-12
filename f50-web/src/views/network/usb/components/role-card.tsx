import { useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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

/** The port's role now, and the host role at every boot (device is the boot default). */
export function UsbRoleCard({ info }: { info: UsbInfo }) {
  const qc = useQueryClient();
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
    <Card>
      <CardHeader>
        <CardTitle>{t("network.usb.roleCard.title")}</CardTitle>
        <CardDescription>
          {t("network.usb.roleCard.description")}
        </CardDescription>
      </CardHeader>
      <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
        <CardContent className="flex flex-col gap-4">
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
        </CardContent>
        <CardFooter className="mt-4 flex justify-end">
          <PendingButton
            type="submit"
            pending={save.isPending}
            disabled={!supported}
          >
            {save.isPending
              ? t("common.action.saving")
              : t("common.action.apply")}
          </PendingButton>
        </CardFooter>
      </form>
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
    </Card>
  );
}

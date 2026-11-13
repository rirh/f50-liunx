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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { setUsb, usbKeys } from "../api";
import { netModeHint, netModeLabel } from "../labels";
import type { UsbInfo, UsbNetMode, UsbSetArgs } from "../types";

const MODES: UsbNetMode[] = ["default", "ncm", "ecm", "rndis", "multi"];
const schema = z.object({
  net: z.enum(["default", "ncm", "ecm", "rndis", "multi"]),
  scope: z.enum(["once", "permanent"]),
  name: z.enum(["usb", "eth"]),
});
type NetForm = z.infer<typeof schema>;
const FIELDS = ["net", "scope", "name"] as const;

/**
 * The gadget's network function from the next boot, and the link's name on a Linux computer. The status card shows
 * both already, so they open from its header in a dialog.
 */
export function UsbNetDialog({ info }: { info: UsbInfo }) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const initial: NetForm = {
    net: info.net_mode,
    scope: info.net_scope,
    name: info.net_name,
  };
  const form = useForm<NetForm>({
    resolver: zodResolver(schema),
    defaultValues: initial,
  });
  const save = useMutation({
    mutationFn: setUsb,
    onSuccess: () => {
      toast.success(t("network.usb.net.saved"));
      setOpen(false);
      qc.invalidateQueries({ queryKey: usbKeys.info });
    },
    onError: (e) => reportFormError(e, form.setError, FIELDS),
  });
  const hostRole = info.role === "host" || info.role_boot === "host";
  const net = useWatch({ control: form.control, name: "net" });

  const onSubmit = (v: NetForm) => {
    const args: UsbSetArgs = {};
    if (
      v.net !== initial.net ||
      (v.net !== "default" && v.scope !== initial.scope)
    ) {
      args.net = v.net;
      args.scope = v.scope;
    }
    if (v.name !== initial.name) args.name = v.name;
    if (Object.keys(args).length === 0) {
      toast.info(t("network.shared.noChanges"));
      return;
    }
    save.mutate(args);
  };

  return (
    <>
      <EditButton
        onClick={() => {
          form.reset(initial);
          setOpen(true);
        }}
      >
        {t("network.usb.net.title")}
      </EditButton>
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("network.usb.net.title")}
        description={t("network.usb.net.description")}
        formId="usb-net"
        pending={save.isPending}
        dirty={form.formState.isDirty}
      >
        <form
          id="usb-net"
          onSubmit={form.handleSubmit(onSubmit)}
          noValidate
          className="flex flex-col gap-4"
        >
          {hostRole && (
            <Alert>
              <AlertTitle>{t("network.usb.net.hostRole.title")}</AlertTitle>
              <AlertDescription>
                {t("network.usb.net.hostRole.description")}
              </AlertDescription>
            </Alert>
          )}
          <FieldGroup>
            <Controller
              control={form.control}
              name="net"
              render={({ field, fieldState }) => (
                <Field
                  data-invalid={fieldState.invalid}
                  data-disabled={hostRole || undefined}
                >
                  <FieldLabel>{t("network.usb.net.mode")}</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    className="flex-wrap"
                    disabled={hostRole}
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    {MODES.map((m) => (
                      <ToggleGroupItem key={m} value={m}>
                        {netModeLabel(m)}
                      </ToggleGroupItem>
                    ))}
                  </ToggleGroup>
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>{netModeHint(net)}</FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="scope"
              render={({ field, fieldState }) => (
                <Field
                  data-invalid={fieldState.invalid}
                  data-disabled={hostRole || net === "default" || undefined}
                >
                  <FieldLabel>{t("network.usb.net.scope")}</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    disabled={hostRole || net === "default"}
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    <ToggleGroupItem value="permanent">
                      {t("network.usb.net.permanent")}
                    </ToggleGroupItem>
                    <ToggleGroupItem value="once">
                      {t("network.usb.net.once")}
                    </ToggleGroupItem>
                  </ToggleGroup>
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      {t("network.usb.net.onceHint")}
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="name"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel>{t("network.usb.net.name")}</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    <ToggleGroupItem value="usb">usbN</ToggleGroupItem>
                    <ToggleGroupItem value="eth">ethN</ToggleGroupItem>
                  </ToggleGroup>
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
          </FieldGroup>
        </form>
      </FormDialog>
    </>
  );
}

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

/** The gadget's network function from the next boot, and the link's name on a Linux computer. */
export function UsbNetCard({ info }: { info: UsbInfo }) {
  const qc = useQueryClient();
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
    <Card>
      <CardHeader>
        <CardTitle>{t("network.usb.net.title")}</CardTitle>
        <CardDescription>{t("network.usb.net.description")}</CardDescription>
      </CardHeader>
      <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
        <CardContent className="flex flex-col gap-4">
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
        </CardContent>
        <CardFooter className="mt-4 flex justify-end">
          <PendingButton type="submit" pending={save.isPending}>
            {save.isPending
              ? t("common.action.saving")
              : t("common.action.save")}
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}

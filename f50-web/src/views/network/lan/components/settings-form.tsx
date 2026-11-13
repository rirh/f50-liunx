import { useMemo, useState } from "react";
import { EditButton, FormDialog } from "@/components/f50/form-dialog";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { lanKeys, setLan } from "../api";
import {
  LAN_FIELDS,
  type LanForm,
  NETMASKS,
  consoleUrlAt,
  dhcpRange,
  lanArgs,
  lanDefaults,
  lanSchema,
} from "../schema";
import type { LanInfo, LanSetArgs } from "../types";

function prefixOf(mask: string): number {
  return mask
    .split(".")
    .reduce((n, o) => n + (Number(o).toString(2).match(/1/g)?.length ?? 0), 0);
}

/**
 * The LAN's address and DHCP pool: changed rarely and shown by the status card already, so they open from that card's
 * header in a dialog. Moving the address (or netmask) asks once more, since the console then lives elsewhere.
 */
export function LanSettingsDialog({
  info,
  onMoved,
}: {
  info: LanInfo;
  onMoved: (url: string) => void;
}) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const initial = useMemo(() => lanDefaults(info), [info]);
  const schema = useMemo(() => lanSchema(), []);
  const form = useForm<LanForm>({
    resolver: zodResolver(schema),
    defaultValues: initial,
    mode: "onTouched",
  });
  const [pending, setPending] = useState<LanSetArgs | null>(null);

  const save = useMutation({
    mutationFn: setLan,
    onSuccess: (_, args) => {
      setOpen(false);
      if (args.address) {
        onMoved(consoleUrlAt(args.address));
        toast.success(t("network.lan.settings.savedMoved"));
      } else {
        toast.success(t("network.lan.settings.saved"));
        qc.invalidateQueries({ queryKey: lanKeys.info });
      }
    },
    onError: (e) => reportFormError(e, form.setError, LAN_FIELDS),
  });

  const [address, netmask, dhcp, start, limit] = useWatch({
    control: form.control,
    name: ["address", "netmask", "dhcp", "dhcp_start", "dhcp_limit"],
  });
  const range = dhcpRange(address, netmask, start, limit);

  const onSubmit = (v: LanForm) => {
    const args = lanArgs(v, initial);
    if (Object.keys(args).length === 0) {
      toast.info(t("network.shared.noChanges"));
      return;
    }
    if (args.address || args.netmask) setPending(args);
    else save.mutate(args);
  };

  return (
    <>
      <EditButton
        onClick={() => {
          form.reset(lanDefaults(info));
          setOpen(true);
        }}
      />
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("network.lan.settings.title")}
        description={t("network.lan.settings.description")}
        formId="lan-settings"
        pending={save.isPending}
        dirty={form.formState.isDirty}
        submitText={t("network.shared.saveApply")}
        size="lg"
      >
        <form
          id="lan-settings"
          onSubmit={form.handleSubmit(onSubmit)}
          noValidate
        >
          <FieldGroup>
            <div className="grid gap-4 md:grid-cols-2">
              <Controller
                control={form.control}
                name="address"
                render={({ field, fieldState }) => (
                  <Field data-invalid={fieldState.invalid}>
                    <FieldLabel htmlFor="lan-address">
                      {t("network.lan.settings.address")}
                    </FieldLabel>
                    <Input
                      id="lan-address"
                      inputMode="decimal"
                      autoComplete="off"
                      className="font-mono"
                      aria-invalid={fieldState.invalid}
                      {...field}
                    />
                    {fieldState.error ? (
                      <FieldError errors={[fieldState.error]} />
                    ) : (
                      <FieldDescription>
                        {t("network.lan.settings.addressHint")}
                      </FieldDescription>
                    )}
                  </Field>
                )}
              />
              <Controller
                control={form.control}
                name="netmask"
                render={({ field, fieldState }) => (
                  <Field data-invalid={fieldState.invalid}>
                    <FieldLabel htmlFor="lan-netmask">
                      {t("network.lan.settings.netmask")}
                    </FieldLabel>
                    <Select value={field.value} onValueChange={field.onChange}>
                      <SelectTrigger
                        id="lan-netmask"
                        className="w-full font-mono"
                        aria-invalid={fieldState.invalid}
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectGroup>
                          {NETMASKS.map((m) => (
                            <SelectItem key={m} value={m} className="font-mono">
                              {m} (/{prefixOf(m)})
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                    <FieldError errors={[fieldState.error]} />
                  </Field>
                )}
              />
            </div>
            <FieldSeparator />
            <Controller
              control={form.control}
              name="dhcp"
              render={({ field }) => (
                <Field orientation="horizontal">
                  <Switch
                    id="lan-dhcp"
                    checked={field.value}
                    onCheckedChange={field.onChange}
                  />
                  <FieldLabel htmlFor="lan-dhcp">
                    {t("network.lan.settings.dhcp")}
                  </FieldLabel>
                </Field>
              )}
            />
            <div className="grid gap-4 sm:grid-cols-3">
              {(
                [
                  [
                    "dhcp_start",
                    t("network.lan.settings.dhcpStart"),
                    "2-254",
                    "numeric",
                  ],
                  [
                    "dhcp_limit",
                    t("network.lan.settings.dhcpLimit"),
                    "1-253",
                    "numeric",
                  ],
                  [
                    "leasetime",
                    t("network.lan.settings.leasetime"),
                    t("network.lan.settings.leasetimeHint"),
                    "text",
                  ],
                ] as const
              ).map(([name, label, hint, mode]) => (
                <Controller
                  key={name}
                  control={form.control}
                  name={name}
                  render={({ field, fieldState }) => (
                    <Field
                      data-invalid={fieldState.invalid}
                      data-disabled={!dhcp || undefined}
                    >
                      <FieldLabel htmlFor={`lan-${name}`}>{label}</FieldLabel>
                      <Input
                        id={`lan-${name}`}
                        inputMode={mode}
                        autoComplete="off"
                        disabled={!dhcp}
                        aria-invalid={fieldState.invalid}
                        {...field}
                      />
                      {fieldState.error ? (
                        <FieldError errors={[fieldState.error]} />
                      ) : (
                        <FieldDescription>{hint}</FieldDescription>
                      )}
                    </Field>
                  )}
                />
              ))}
            </div>
            {dhcp && range && (
              <FieldDescription>
                {t("network.lan.settings.pool")}
                <span className="tabular-nums">{range}</span>
              </FieldDescription>
            )}
          </FieldGroup>
        </form>
      </FormDialog>
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(o) => !o && setPending(null)}
        title={t("network.lan.settings.confirm.title")}
        confirmText={t("network.lan.settings.confirm.confirm")}
        destructive
        description={
          <>
            <span>{t("network.lan.settings.confirm.description")}</span>
            {pending?.address ? (
              <span>
                {t("network.lan.settings.confirm.consoleAt")}
                <span className="tabular-nums break-all">
                  {consoleUrlAt(pending.address)}
                </span>
              </span>
            ) : (
              <span>
                {t("network.lan.settings.confirm.newNetmask")}
                <span className="tabular-nums">{pending?.netmask}</span>
              </span>
            )}
          </>
        }
        onConfirm={() => pending && save.mutate(pending)}
      />
    </>
  );
}

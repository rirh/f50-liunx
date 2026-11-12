import { useMemo, useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
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

export function LanSettingsForm({
  info,
  onMoved,
}: {
  info: LanInfo;
  onMoved: (url: string) => void;
}) {
  const qc = useQueryClient();
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
    <Card>
      <CardHeader>
        <CardTitle>{t("network.lan.settings.title")}</CardTitle>
        <CardDescription>
          {t("network.lan.settings.description")}
        </CardDescription>
      </CardHeader>
      <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
        <CardContent>
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
                <span className="font-mono">{range}</span>
              </FieldDescription>
            )}
          </FieldGroup>
        </CardContent>
        <CardFooter className="mt-4 flex flex-wrap justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={save.isPending || !form.formState.isDirty}
            onClick={() => form.reset(initial)}
          >
            {t("common.action.reset")}
          </Button>
          <PendingButton type="submit" pending={save.isPending}>
            {save.isPending
              ? t("common.action.saving")
              : t("network.shared.saveApply")}
          </PendingButton>
        </CardFooter>
      </form>
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
                <span className="font-mono break-all">
                  {consoleUrlAt(pending.address)}
                </span>
              </span>
            ) : (
              <span>
                {t("network.lan.settings.confirm.newNetmask")}
                <span className="font-mono">{pending?.netmask}</span>
              </span>
            )}
          </>
        }
        onConfirm={() => pending && save.mutate(pending)}
      />
    </Card>
  );
}

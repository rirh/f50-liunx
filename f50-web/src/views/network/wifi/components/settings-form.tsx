import { useEffect, useMemo, useState } from "react";
import { EditButton, FormDialog } from "@/components/f50/form-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t, type Key } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { PasswordInput } from "../../shared/password-input";
import { pskHint } from "../../shared/text-rules";
import { setWifi, wifiKeys } from "../api";
import { channelsFor, widthsFor } from "../channels";
import {
  WIFI_FIELDS,
  type WifiForm,
  wifiArgs,
  wifiDefaults,
  wifiSchema,
} from "../schema";
import type { WifiInfo, WifiSetArgs } from "../types";

const ARG_LABEL = {
  ssid: "network.wifi.settings.arg.ssid",
  password: "network.wifi.settings.arg.password",
  band: "network.wifi.settings.arg.band",
  channel: "network.wifi.settings.arg.channel",
  width: "network.wifi.settings.arg.width",
  hidden: "network.wifi.settings.arg.hidden",
  country: "network.wifi.settings.arg.country",
} satisfies Record<keyof WifiSetArgs, Key>;

function argText(k: keyof WifiSetArgs, v: unknown): string {
  if (k === "password") return t("network.wifi.settings.newPassword");
  if (k === "hidden") return v ? t("common.state.yes") : t("common.state.no");
  if (k === "band") return `${String(v)} GHz`;
  if (v === "auto") return t("common.state.auto");
  if (k === "width") return `${String(v)} MHz`;
  return String(v);
}

/**
 * The hotspot's name, password, band, channel and country: the status card shows them already, so they open from its
 * header in a dialog. The notice that saving restarts the radio sits above the fields, and applying still asks once
 * more with the list of what changes.
 */
export function WifiSettingsDialog({
  info,
  password,
}: {
  info: WifiInfo;
  /** The saved password, shown in the form; empty when it could not be read. */
  password: string;
}) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const initial = useMemo(() => wifiDefaults(info, password), [info, password]);
  const schema = useMemo(() => wifiSchema(initial), [initial]);
  const form = useForm<WifiForm>({
    resolver: zodResolver(schema),
    defaultValues: initial,
    mode: "onTouched",
  });
  const [pending, setPending] = useState<WifiSetArgs | null>(null);

  useEffect(() => {
    if (open && !form.getFieldState("password").isDirty) {
      form.resetField("password", { defaultValue: password });
    }
  }, [open, password, form]);

  const save = useMutation({
    mutationFn: setWifi,
    onSuccess: () => {
      toast.success(t("network.wifi.settings.saved"));
      setOpen(false);
      qc.invalidateQueries({ queryKey: wifiKeys.info });
      qc.invalidateQueries({ queryKey: wifiKeys.credentials });
    },
    onError: (e) => reportFormError(e, form.setError, WIFI_FIELDS),
  });

  const [band, country, channel] = useWatch({
    control: form.control,
    name: ["band", "country", "channel"],
  });
  const channels = channelsFor(band, country);
  const channelOptions = ["auto", ...channels.map(String)];
  if (!channelOptions.includes(channel)) channelOptions.push(channel);
  const widths = widthsFor(band, country, channel);
  const saeNow = info.security === "wpa3" || info.security === "wpa2-wpa3";

  /** A channel or width the new band/country no longer offers goes back to automatic. */
  const fitChannel = (nextBand: WifiForm["band"], cc: string, ch: string) => {
    const opts = /^[A-Z]{2}$/.test(cc) ? channelsFor(nextBand, cc) : [];
    if (ch !== "auto" && !opts.includes(Number(ch))) {
      form.setValue("channel", "auto", { shouldDirty: true });
      ch = "auto";
    }
    if (!widthsFor(nextBand, cc, ch).includes(form.getValues("width"))) {
      form.setValue("width", "auto", { shouldDirty: true });
    }
  };

  const onSubmit = (v: WifiForm) => {
    const args = wifiArgs(v, initial);
    if (Object.keys(args).length === 0) {
      toast.info(t("network.shared.noChanges"));
      return;
    }
    setPending(args);
  };

  return (
    <>
      <EditButton
        onClick={() => {
          form.reset(initial);
          setOpen(true);
        }}
      />
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("network.wifi.settings.title")}
        description={t("network.wifi.settings.description")}
        formId="wifi-settings"
        pending={save.isPending}
        dirty={form.formState.isDirty}
        submitText={t("network.shared.saveApply")}
        size="lg"
      >
        <form
          id="wifi-settings"
          onSubmit={form.handleSubmit(onSubmit)}
          noValidate
          className="flex flex-col gap-4"
        >
          <Alert>
            <AlertTitle>{t("network.wifi.restartAlert.title")}</AlertTitle>
            <AlertDescription>
              {t("network.wifi.restartAlert.description")}
            </AlertDescription>
          </Alert>
          {info.client_mode && (
            <Alert>
              <AlertTitle>
                {t("network.wifi.settings.clientMode.title")}
              </AlertTitle>
              <AlertDescription>
                {t("network.wifi.settings.clientMode.description")}
              </AlertDescription>
            </Alert>
          )}
          {saeNow && (
            <Alert>
              <AlertTitle>{t("network.wifi.settings.noWpa3.title")}</AlertTitle>
              <AlertDescription>
                {t("network.wifi.settings.noWpa3.description")}
              </AlertDescription>
            </Alert>
          )}
          <FieldGroup className="grid gap-4 md:grid-cols-2">
            <Controller
              control={form.control}
              name="ssid"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-ssid">
                    {t("network.wifi.settings.ssid")}
                  </FieldLabel>
                  <Input
                    id="wifi-ssid"
                    autoComplete="off"
                    aria-invalid={fieldState.invalid}
                    {...field}
                  />
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="password"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-password">
                    {t("network.wifi.settings.password")}
                  </FieldLabel>
                  <PasswordInput
                    id="wifi-password"
                    autoComplete="new-password"
                    placeholder={
                      info.password_set
                        ? t("network.wifi.settings.passwordSet")
                        : t("network.wifi.settings.passwordUnset")
                    }
                    aria-invalid={fieldState.invalid}
                    {...field}
                  />
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      {pskHint()}
                      {info.security === "open"
                        ? t("network.wifi.settings.openToWpa2")
                        : ""}
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="band"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel>{t("network.wifi.settings.band")}</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => {
                      if (!v) return;
                      const b = v as WifiForm["band"];
                      field.onChange(b);
                      if (b !== initial.band) {
                        form.setValue("channel", "auto", { shouldDirty: true });
                        form.setValue("width", "auto", { shouldDirty: true });
                      } else {
                        fitChannel(
                          b,
                          form.getValues("country"),
                          form.getValues("channel"),
                        );
                      }
                    }}
                  >
                    <ToggleGroupItem value="2.4">2.4 GHz</ToggleGroupItem>
                    <ToggleGroupItem value="5">5 GHz</ToggleGroupItem>
                  </ToggleGroup>
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="country"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-country">
                    {t("network.wifi.settings.country")}
                  </FieldLabel>
                  <Input
                    id="wifi-country"
                    maxLength={2}
                    autoComplete="off"
                    className="uppercase"
                    aria-invalid={fieldState.invalid}
                    {...field}
                    onChange={(e) => {
                      const cc = e.target.value
                        .toUpperCase()
                        .replace(/[^A-Z]/g, "")
                        .slice(0, 2);
                      field.onChange(cc);
                      if (cc.length === 2)
                        fitChannel(
                          form.getValues("band"),
                          cc,
                          form.getValues("channel"),
                        );
                    }}
                  />
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      {t("network.wifi.settings.countryHint", {
                        channels: ap5Text(country),
                      })}
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="channel"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-channel">
                    {t("network.wifi.settings.channel")}
                  </FieldLabel>
                  <Select
                    value={field.value}
                    onValueChange={(v) => {
                      field.onChange(v);
                      if (
                        !widthsFor(
                          form.getValues("band"),
                          form.getValues("country"),
                          v,
                        ).includes(form.getValues("width"))
                      ) {
                        form.setValue("width", "auto", { shouldDirty: true });
                      }
                    }}
                  >
                    <SelectTrigger
                      id="wifi-channel"
                      className="w-full"
                      aria-invalid={fieldState.invalid}
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        {channelOptions.map((c) => (
                          <SelectItem key={c} value={c}>
                            {c === "auto"
                              ? t("common.state.auto")
                              : channels.includes(Number(c))
                                ? c
                                : t(
                                    "network.wifi.settings.channelUnavailable",
                                    {
                                      channel: c,
                                    },
                                  )}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      {t("network.wifi.settings.channelHint")}
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="width"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel>{t("network.wifi.settings.width")}</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    {["auto", "20", "40", "80"].map((w) => (
                      <ToggleGroupItem
                        key={w}
                        value={w}
                        disabled={!widths.includes(w)}
                      >
                        {w === "auto" ? t("common.state.auto") : `${w} MHz`}
                      </ToggleGroupItem>
                    ))}
                  </ToggleGroup>
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      {t("network.wifi.settings.widthHint")}
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="hidden"
              render={({ field }) => (
                <Field orientation="horizontal" className="md:col-span-2">
                  <Switch
                    id="wifi-hidden"
                    checked={field.value}
                    onCheckedChange={field.onChange}
                  />
                  <FieldLabel htmlFor="wifi-hidden">
                    {t("network.wifi.settings.hidden")}
                  </FieldLabel>
                </Field>
              )}
            />
          </FieldGroup>
        </form>
      </FormDialog>
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(o) => !o && setPending(null)}
        title={t("network.wifi.settings.confirmTitle")}
        confirmText={t("common.action.apply")}
        description={
          <>
            <span>{t("network.wifi.settings.confirmDescription")}</span>
            {pending && (
              <span>
                {Object.entries(pending)
                  .map(([k, v]) =>
                    t("network.shared.pair", {
                      label: t(ARG_LABEL[k as keyof WifiSetArgs]),
                      value: argText(k as keyof WifiSetArgs, v),
                    }),
                  )
                  .join(t("network.shared.sep"))}
              </span>
            )}
          </>
        }
        onConfirm={() => pending && save.mutate(pending)}
      />
    </>
  );
}

function ap5Text(cc: string): string {
  if (!/^[A-Z]{2}$/.test(cc)) return t("network.wifi.settings.ap5.enterCode");
  const l = channelsFor("5", cc);
  return l.length
    ? t("network.wifi.settings.ap5.list", { channels: l.join(" ") })
    : t("network.wifi.settings.ap5.none");
}

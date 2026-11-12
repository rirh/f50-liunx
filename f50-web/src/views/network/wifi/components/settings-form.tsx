import { useMemo, useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
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

export function WifiSettingsForm({ info }: { info: WifiInfo }) {
  const qc = useQueryClient();
  const initial = useMemo(() => wifiDefaults(info), [info]);
  const schema = useMemo(() => wifiSchema(initial), [initial]);
  const form = useForm<WifiForm>({
    resolver: zodResolver(schema),
    defaultValues: initial,
    mode: "onTouched",
  });
  const [pending, setPending] = useState<WifiSetArgs | null>(null);

  const save = useMutation({
    mutationFn: setWifi,
    onSuccess: () => {
      toast.success(t("network.wifi.settings.saved"));
      form.reset({ ...form.getValues(), password: "" });
      qc.invalidateQueries({ queryKey: wifiKeys.info });
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
    <Card>
      <CardHeader>
        <CardTitle>{t("network.wifi.settings.title")}</CardTitle>
        <CardDescription>
          {t("network.wifi.settings.description")}
        </CardDescription>
      </CardHeader>
      <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
        <CardContent className="flex flex-col gap-4">
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
                  <Input
                    id="wifi-password"
                    type="password"
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
    </Card>
  );
}

function ap5Text(cc: string): string {
  if (!/^[A-Z]{2}$/.test(cc)) return t("network.wifi.settings.ap5.enterCode");
  const l = channelsFor("5", cc);
  return l.length
    ? t("network.wifi.settings.ap5.list", { channels: l.join(" ") })
    : t("network.wifi.settings.ap5.none");
}

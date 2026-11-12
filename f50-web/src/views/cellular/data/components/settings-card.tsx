import { useMemo } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Badge } from "@/components/ui/badge";
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
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t, type Key } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { dataKeys, setData } from "../api";
import { type DataForm, dataSchema } from "../schema";
import type { AuthWrite, DataSettings, DataStatus } from "../types";

const AUTH_NOTE = {
  both: "cellular.data.settings.authBoth",
  invalid: "cellular.data.settings.authInvalid",
} as const satisfies Record<string, Key>;

function defaults(d: DataStatus): DataForm {
  return {
    apn: d.apn,
    auth: d.auth,
    username: d.username,
    password: "",
    pdp: d.pdp,
    roaming: d.roaming,
  };
}

/** The APN settings and data roaming of the SIM in use; only changed fields are sent. */
export function SettingsCard({ d }: { d: DataStatus }) {
  const qc = useQueryClient();
  const form = useForm<DataForm>({
    resolver: zodResolver(useMemo(() => dataSchema(), [])),
    defaultValues: defaults(d),
  });
  const save = useMutation({
    mutationFn: setData,
    onSuccess: (r) => {
      toast.success(
        r.redial
          ? t("cellular.data.settings.savedRedial")
          : t("cellular.shared.saved"),
      );
      qc.invalidateQueries({ queryKey: dataKeys.get });
    },
    onError: (e) => reportFormError(e, form.setError),
  });
  const { errors, dirtyFields, isDirty } = form.formState;
  const auth = useWatch({ control: form.control, name: "auth" });
  const creds = auth !== "none";

  const submit = form.handleSubmit((v) => {
    const out: DataSettings = {};
    if (dirtyFields.apn) out.apn = v.apn;
    if (dirtyFields.auth && v.auth !== "both" && v.auth !== "invalid")
      out.auth = v.auth;
    if (dirtyFields.username) out.username = v.username;
    if (v.password !== "") out.password = v.password;
    if (dirtyFields.pdp) out.pdp = v.pdp;
    if (dirtyFields.roaming) out.roaming = v.roaming;
    if (Object.keys(out).length === 0)
      return toast.info(t("cellular.shared.noChanges"));
    save.mutate(out);
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.data.settings.title")}</CardTitle>
        <CardDescription>
          {t("cellular.data.settings.description", { slot: d.slot })}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form id="data-settings" onSubmit={submit}>
          <FieldGroup>
            <Field data-invalid={!!errors.apn}>
              <FieldLabel htmlFor="data-apn">APN</FieldLabel>
              <Input
                id="data-apn"
                autoComplete="off"
                aria-invalid={!!errors.apn}
                {...form.register("apn")}
              />
              <FieldDescription>
                {t("cellular.data.settings.apnHint")}
              </FieldDescription>
              <FieldError errors={[errors.apn]} />
            </Field>
            <Field data-invalid={!!errors.pdp}>
              <FieldLabel>{t("cellular.data.settings.pdp")}</FieldLabel>
              <Controller
                control={form.control}
                name="pdp"
                render={({ field }) => (
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    className="flex-wrap"
                    value={field.value}
                    aria-invalid={!!errors.pdp}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    <ToggleGroupItem value="ipv4">IPv4</ToggleGroupItem>
                    <ToggleGroupItem value="ipv6">IPv6</ToggleGroupItem>
                    <ToggleGroupItem value="ipv4v6">IPv4v6</ToggleGroupItem>
                  </ToggleGroup>
                )}
              />
              <FieldError errors={[errors.pdp]} />
            </Field>
            <Field data-invalid={!!errors.auth}>
              <FieldLabel>{t("cellular.data.settings.auth")}</FieldLabel>
              <Controller
                control={form.control}
                name="auth"
                render={({ field }) => (
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    className="flex-wrap"
                    value={field.value}
                    aria-invalid={!!errors.auth}
                    onValueChange={(v) => v && field.onChange(v as AuthWrite)}
                  >
                    <ToggleGroupItem value="none">
                      {t("cellular.data.settings.authNone")}
                    </ToggleGroupItem>
                    <ToggleGroupItem value="pap">PAP</ToggleGroupItem>
                    <ToggleGroupItem value="chap">CHAP</ToggleGroupItem>
                  </ToggleGroup>
                )}
              />
              {(auth === "both" || auth === "invalid") && (
                <FieldDescription>{t(AUTH_NOTE[auth])}</FieldDescription>
              )}
              <FieldError errors={[errors.auth]} />
            </Field>
            {creds && (
              <div className="grid gap-4 sm:grid-cols-2">
                <Field data-invalid={!!errors.username}>
                  <FieldLabel htmlFor="data-username">
                    {t("cellular.data.settings.username")}
                  </FieldLabel>
                  <Input
                    id="data-username"
                    autoComplete="off"
                    aria-invalid={!!errors.username}
                    {...form.register("username")}
                  />
                  <FieldError errors={[errors.username]} />
                </Field>
                <Field data-invalid={!!errors.password}>
                  <FieldLabel
                    htmlFor="data-password"
                    className="flex items-center gap-2"
                  >
                    {t("cellular.data.settings.password")}
                    <Badge variant={d.password_set ? "secondary" : "outline"}>
                      {d.password_set
                        ? t("cellular.data.settings.passwordSet")
                        : t("cellular.data.settings.passwordUnset")}
                    </Badge>
                  </FieldLabel>
                  <Input
                    id="data-password"
                    type="password"
                    autoComplete="new-password"
                    placeholder={
                      d.password_set
                        ? t("cellular.data.settings.passwordKeep")
                        : ""
                    }
                    aria-invalid={!!errors.password}
                    {...form.register("password")}
                  />
                  <FieldError errors={[errors.password]} />
                </Field>
              </div>
            )}
            <FieldSeparator />
            <Field orientation="horizontal" data-invalid={!!errors.roaming}>
              <FieldContent>
                <FieldLabel htmlFor="data-roaming">
                  {t("cellular.data.settings.roaming")}
                </FieldLabel>
                <FieldDescription>
                  {d.roaming_held
                    ? t("cellular.data.settings.roamingHeld")
                    : t("cellular.data.settings.roamingHint")}
                </FieldDescription>
                <FieldError errors={[errors.roaming]} />
              </FieldContent>
              <Controller
                control={form.control}
                name="roaming"
                render={({ field }) => (
                  <Switch
                    id="data-roaming"
                    checked={field.value}
                    aria-invalid={!!errors.roaming}
                    onCheckedChange={field.onChange}
                  />
                )}
              />
            </Field>
          </FieldGroup>
        </form>
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <PendingButton
          type="submit"
          form="data-settings"
          pending={save.isPending}
          disabled={!isDirty}
        >
          {t("common.action.save")}
        </PendingButton>
        <Button
          variant="outline"
          disabled={!isDirty || save.isPending}
          onClick={() => form.reset(defaults(d))}
        >
          {t("common.action.reset")}
        </Button>
      </CardFooter>
    </Card>
  );
}

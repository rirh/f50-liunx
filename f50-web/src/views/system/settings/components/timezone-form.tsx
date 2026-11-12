import { useMemo } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { formatTime } from "@/lib/format";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { infoKey, setTimezone } from "../api";
import { type TimezoneValues, timezoneSchema, zoneList } from "../schemas";

export function TimezoneForm({
  current,
  time,
}: {
  current: string | null;
  time: number;
}) {
  const qc = useQueryClient();
  const zones = useMemo(() => {
    const list = zoneList();
    return current && !list.includes(current) ? [current, ...list] : list;
  }, [current]);
  const schema = useMemo(() => timezoneSchema(), []);
  const form = useForm<TimezoneValues>({
    resolver: zodResolver(schema),
    values: { zone: current ?? "UTC" },
  });
  const save = useMutation({
    mutationFn: (v: TimezoneValues) => setTimezone(v.zone),
    onSuccess: (r) => {
      toast.success(t("system.settings.timezone.saved", { zone: r.timezone }));
      qc.invalidateQueries({ queryKey: infoKey });
    },
    onError: (e) => reportFormError(e, form.setError, { zone: "zone" }),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.settings.timezone.title")}</CardTitle>
        <CardDescription>
          {t("system.settings.timezone.description", {
            time: formatTime(time),
          })}
        </CardDescription>
      </CardHeader>
      <form
        onSubmit={form.handleSubmit((v) => save.mutate(v))}
        className="contents"
      >
        <CardContent>
          <FieldGroup>
            <Controller
              name="zone"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="timezone">
                    {t("system.settings.timezone.label")}
                  </FieldLabel>
                  <Combobox
                    items={zones}
                    value={field.value}
                    onValueChange={(v) =>
                      field.onChange((v as string | null) ?? "")
                    }
                  >
                    <ComboboxInput
                      id="timezone"
                      placeholder={t("system.settings.timezone.placeholder")}
                      aria-invalid={fieldState.invalid}
                      onBlur={field.onBlur}
                    />
                    <ComboboxContent>
                      <ComboboxEmpty>
                        {t("system.settings.timezone.empty")}
                      </ComboboxEmpty>
                      <ComboboxList>
                        {(z: string) => (
                          <ComboboxItem key={z} value={z}>
                            {z}
                          </ComboboxItem>
                        )}
                      </ComboboxList>
                    </ComboboxContent>
                  </Combobox>
                  <FieldDescription>
                    {t("system.settings.timezone.hint")}
                  </FieldDescription>
                  {fieldState.invalid && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
          </FieldGroup>
        </CardContent>
        <CardFooter>
          <PendingButton
            type="submit"
            pending={save.isPending}
            disabled={!form.formState.isDirty}
          >
            {t("common.action.save")}
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}

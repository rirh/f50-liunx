import { useMemo, useState } from "react";
import { EditButton, FormDialog } from "@/components/f50/form-dialog";
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
import { SettingRow } from "./setting-row";
import { infoKey, setTimezone } from "../api";
import { type TimezoneValues, timezoneSchema, zoneList } from "../schemas";

/**
 * The time zone as a row of the General card, with the device's clock under it (so a wrong zone shows at a glance),
 * edited in a dialog. A device without a zone set runs on UTC, so that is what the row and the form start from.
 */
export function TimezoneSetting({
  current,
  time,
}: {
  current: string | null;
  time: number;
}) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const zones = useMemo(() => {
    const list = zoneList();
    return current && !list.includes(current) ? [current, ...list] : list;
  }, [current]);
  const schema = useMemo(() => timezoneSchema(), []);
  const form = useForm<TimezoneValues>({
    resolver: zodResolver(schema),
    defaultValues: { zone: current ?? "UTC" },
  });
  const save = useMutation({
    mutationFn: (v: TimezoneValues) => setTimezone(v.zone),
    onSuccess: (r) => {
      toast.success(t("system.settings.timezone.saved", { zone: r.timezone }));
      setOpen(false);
      qc.invalidateQueries({ queryKey: infoKey });
    },
    onError: (e) => reportFormError(e, form.setError, { zone: "zone" }),
  });
  return (
    <>
      <SettingRow
        label={t("system.settings.timezone.title")}
        value={current ?? "UTC"}
        hint={t("system.settings.timezone.now", { time: formatTime(time) })}
        action={
          <EditButton
            onClick={() => {
              form.reset({ zone: current ?? "UTC" });
              setOpen(true);
            }}
          />
        }
      />
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("system.settings.timezone.title")}
        description={t("system.settings.timezone.description", {
          time: formatTime(time),
        })}
        formId="timezone-form"
        size="sm"
        pending={save.isPending}
        dirty={form.formState.isDirty}
      >
        <form
          id="timezone-form"
          onSubmit={form.handleSubmit((v) => save.mutate(v))}
        >
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
        </form>
      </FormDialog>
    </>
  );
}

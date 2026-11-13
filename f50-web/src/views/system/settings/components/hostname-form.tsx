import { useMemo, useState } from "react";
import { EditButton, FormDialog } from "@/components/f50/form-dialog";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { SettingRow } from "./setting-row";
import { infoKey, setHostname } from "../api";
import { type HostnameValues, hostnameSchema } from "../schemas";

/**
 * The hostname as a row of the General card, edited in a dialog. Edit resets the form to the device's current name,
 * so the dialog never shows a stale draft from an earlier, cancelled edit.
 */
export function HostnameSetting({ current }: { current: string }) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const schema = useMemo(() => hostnameSchema(), []);
  const form = useForm<HostnameValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: current },
  });
  const save = useMutation({
    mutationFn: (v: HostnameValues) => setHostname(v.name),
    onSuccess: (r) => {
      toast.success(t("system.settings.hostname.saved", { name: r.hostname }));
      setOpen(false);
      qc.invalidateQueries({ queryKey: infoKey });
    },
    onError: (e) => reportFormError(e, form.setError, { name: "name" }),
  });
  return (
    <>
      <SettingRow
        label={t("system.settings.hostname.title")}
        value={<span className="font-mono">{current}</span>}
        action={
          <EditButton
            onClick={() => {
              form.reset({ name: current });
              setOpen(true);
            }}
          />
        }
      />
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("system.settings.hostname.title")}
        description={t("system.settings.hostname.description")}
        formId="hostname-form"
        size="sm"
        pending={save.isPending}
        dirty={form.formState.isDirty}
      >
        <form
          id="hostname-form"
          onSubmit={form.handleSubmit((v) => save.mutate(v))}
        >
          <FieldGroup>
            <Controller
              name="name"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="hostname">
                    {t("system.settings.hostname.label")}
                  </FieldLabel>
                  <Input
                    {...field}
                    id="hostname"
                    autoComplete="off"
                    spellCheck={false}
                    aria-invalid={fieldState.invalid}
                  />
                  <FieldDescription>
                    {t("system.settings.hostname.hint")}
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
